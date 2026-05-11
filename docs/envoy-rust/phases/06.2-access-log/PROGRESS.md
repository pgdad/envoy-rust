# Phase 06.2 — Implementation Progress

Per-task narrative log for sub-phase 06.2 (`envoy-accesslog` foundation + Envoy default-format access-log emitter + HCM access-log wiring + fixture 0012 + BEHAVIOR_CONTRACT.md `Access log field mapping` first-time population). Mirrors the 06.1 / 05.x PROGRESS.md cadence (one section per task; appended at task commit time; quotes meaningful command output inline).

The companion artifacts:
- **SPEC.md** — `docs/envoy-rust/phases/06.2-access-log/SPEC.md` (1130 lines; committed at the parent-06 state-1+state-2 combined-recovery commit `1f7661a`; the design contract).
- **PLAN.md** — `docs/envoy-rust/phases/06.2-access-log/PLAN.md` (committed at this sub-phase's state-2 commit, alongside this PROGRESS.md skeleton; the per-task task list).

## PLAN-write posture (recorded at sub-phase 06.2 state-2 commit, before any task commits)

### LoC drift posture (per 06.2 SPEC §6 signpost 17 + parent-06 SPEC §5 alternative (vi))

The 06.2 SPEC's §3 D1.2-D5.2 deliverable estimates total **~1580 LoC**; the PLAN-time refinement to 11 tasks projects **~1875 LoC** (a ~20% drift over the SPEC's projection, mostly from TDD step decomposition adding boilerplate the SPEC's bare-counts did not anticipate). Per parent-06 SPEC §5 alternative (vi):

> Nested splits of an already-split sub-phase are explicitly rejected. The PLAN-write planner accepts the LoC drift and proceeds.

The 11-task count is comfortably under the §6.1 25-task gate; the LoC overage is genuine (concentrated in D1.2's multi-module envoy-accesslog decomposition with thorough golden-test surface for the hand-rolled ISO-8601 + Gregorian helper, and D4.2.b's hand-rolled tokenizer per architecture decision 9). The named trims listed in PLAN's "Task summary" section — (i) defer in-process backstop, (ii) fold BEHAVIOR_CONTRACT edit into Task 9, (iii) defer fuzz seed — were considered at PLAN-write time and **not applied** (the trims weaken the gate without sufficient LoC reduction; the doctrinally cleaner posture is to accept the estimate). **Acceptance posture: do NOT trim; do NOT nest-split.** This PROGRESS entry is the documented record of the planner's decision per the established 06.1 / 05.x cadence.

### PLAN-write SPEC corrections (recorded for the executor)

The PLAN.md's preamble section "SPEC corrections recorded at PLAN-write time" lists 4 minor projection inaccuracies in the 06.2 SPEC that the planner verified against HEAD `55fe62d`. Reproduced here for stranger-readability:

1. **No single `write_response` call site in H1 `serve_connection`.** The SPEC's pseudocode in §3 D3.2 + §5 suggests one site; the actual code at `crates/envoy-http1/src/hcm.rs` fans across 5 writer paths (synth + 4 proxy-error/happy: `hcm.rs:266-268`, `:286-291`, `:336-341`, `:353-358`, `:364-370`). PLAN resolves by factoring the access-log dispatch to a **single join point** AFTER the `match outcome` block ends, BEFORE the keep-alive loop continuation at `hcm.rs:375-377`. Task 6 implements.

2. **H2 empty-body path ends via `send_response(.., end_of_stream=true)` not `send_data`.** The SPEC §3 D3.2 says "AFTER `send_data(.., end_of_stream=true)`" but the actual `send_envoy_response` at `crates/envoy-http2/src/response.rs:62-76` skips `send_data` on the empty-body branch (`send_response(head, end_of_stream=resp.body.is_empty())` at line 68 ends the stream when the body is empty). PLAN resolves by landing the H2 dispatch AFTER `send_envoy_response` returns (at `crates/envoy-http2/src/hcm.rs:251`), covering both branches uniformly. Task 7 implements.

3. **`Driver` variant is `TcpEcho`, not `Tcp`.** SPEC §4 references a `Tcp` variant; the actual enum at `tests/differential/src/lib.rs:38-112` has no such variant (the TCP-shaped driver is `Driver::TcpEcho`; the full pre-06.2 variant set is `TcpEcho | HttpGet | TlsTcp | TlsTcpProbeList | Http1 | Http1ProbeList | Http2 | AdminScrape` — 8 variants). Minor naming fix; no code impact.

4. **`HttpConnectionManagerConfig` uses `#[serde(deny_unknown_fields)]` so the new `access_log` field needs `#[serde(default)]`.** SPEC §3 D2.2.a's example does declare `#[serde(default)]` but the rationale is silent. The struct at `crates/envoy-config/src/bootstrap.rs:356-370` carries `#[serde(deny_unknown_fields)]`; without `#[serde(default)]` on `access_log`, the 5 existing HCM-bearing fixtures (`0007/0008/0009/0010/0011`) would fail to parse. Task 5 lands `#[serde(default)]` to keep the absent-block parse green.

**5th clarifying correction (recorded for the executor):** **`envoy-http2` DOES need a direct path-dep on `envoy-accesslog`.** The SPEC §3 D1.2 architectural Rule 1 says *"`envoy-http2` does NOT add a direct dep on `envoy-accesslog`"* on the grounds that *"the alias carries the new `access_log: Vec<Arc<FileSink>>` field transparently"*. But the SPEC §3 D3.2 dispatch pseudocode for the H2 path calls `sink.emit(&record).await` on each `Arc<FileSink>` element of `config.access_log` — a concrete-type method call on `envoy_accesslog::FileSink`, which requires the concrete type to be resolvable at compile time in `envoy-http2`, which requires the path-dep. Task 7 adds `envoy-accesslog = { path = "../envoy-accesslog" }` to `crates/envoy-http2/Cargo.toml`'s `[dependencies]`.

These are minor projection inaccuracies; the SPEC remains in-tree unedited per D-3.5.

### Architecture decisions locked at PLAN-write time

Per the user's standing preference `feedback_pick_recommendation`, every signpost in 06.2 SPEC §6 resolves to its recommendation. Recorded here for stranger-readability (full list in PLAN.md's "Architecture decisions locked at PLAN-write time" section):

- **Sink trait deferred** per option (c); FileSink ships concretely; `HCMConfig.access_log: Vec<Arc<FileSink>>` typed concretely; `crates/envoy-accesslog/src/sink.rs` ships as a doc-comment-only placeholder.
- **HCM dispatch posture: synchronous-after-write** (option (b) per Rule 4); the HCM awaits `sink.emit().await` at the factored join point; emission errors logged via `tracing::warn!` and discarded.
- **ISO-8601 emitter buffer shape:** `&mut String` (signpost 1).
- **Gregorian calendar helper inline** in `default_format.rs` (signpost 2).
- **FileSink concurrency:** `Arc<tokio::sync::Mutex<File>>` (signpost 3).
- **`AccessLogRecord` ownership:** owned `String`s (signpost 5).
- **FileSink path validation:** none beyond OS-level open (signpost 6).
- **`O_APPEND` semantics:** no truncate; no rotation handling (signpost 7).
- **Harness tokenizer:** hand-rolled state machine (signpost 8); no regex dep.
- **`%DURATION%` units:** integer milliseconds via `Duration::as_millis()` (signpost 9).
- **`%UPSTREAM_HOST%` format:** `SocketAddr` Display impl (signpost 11).
- **Fuzz seed:** single-entry (signpost 12).
- **Test logging capture:** custom in-process tracing layer (signpost 15).
- **No `Default` impl on `AccessLogRecord`** (signpost 14).
- **No new top-level Cargo deps** (signpost 20); the new `envoy-accesslog` crate's deps (`tokio`/`bytes`/`tracing`/`thiserror`/`envoy-http1` path-dep) are all already in the workspace's resolved graph; `tempfile = "3"` already a dev-dep in 6 other workspace crates.
- **No ADRs anticipated to land** in 06.2 (per §7 ADR projection); conditional ADR-0030 / ADR-0031 stay available.
- **H1 dispatch site:** factored join point AFTER the `match outcome` block in `serve_connection` (PLAN-write SPEC correction 1).
- **H2 dispatch site:** at `crates/envoy-http2/src/hcm.rs:251` AFTER `send_envoy_response` returns (PLAN-write SPEC correction 2).

### Task ordering note

The 11 PLAN tasks are numbered for documentation. The recommended **execution order** is `1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11` (linear; no reordering needed). Task 1 lands at this state-2 commit (the PROGRESS.md preamble). Tasks 2-4 build the `envoy-accesslog` crate (scaffold → emitter → sink). Task 5 lands the `envoy-config` schema. Tasks 6-7 wire the HCM (H1 + H2). Task 8 lands the in-process backstop. Task 9 extends the differential harness. Task 10 lands the fixture + BEHAVIOR_CONTRACT.md edit. Task 11 verifies state-4. No task has a non-numeric dependency on a later task; the linear order is the recommended execution order.

## Task 1 — PROGRESS.md preamble + LoC drift posture + 4 SPEC corrections + signpost choices

(THIS section. Lands at sub-phase 06.2 state-2 commit alongside PLAN.md and the STATE.md / ROADMAP.md advance.)

## Tasks 2 through 11

Appended at execution time, one section per task commit, mirroring the 06.1 / 05.x per-task cadence.

## Task 2 — envoy-accesslog crate scaffold + record + error + sink placeholder

Lands the new `crates/envoy-accesslog/` workspace member per SPEC §3 D1.2.

**Modules created:**
- `crates/envoy-accesslog/Cargo.toml` — 5 deps (tokio fs+io-util+sync, bytes, tracing, thiserror, envoy-http1 path-dep); `tempfile = "3"` as dev-dep per architecture decision 16.
- `crates/envoy-accesslog/src/lib.rs` — crate root with `#![forbid(unsafe_code)]` + 5 module declarations + 3 public re-exports.
- `crates/envoy-accesslog/src/record.rs` — 15-field `AccessLogRecord` struct + 2 unit tests.
- `crates/envoy-accesslog/src/error.rs` — 3-variant `AccessLogError` (`Open`, `Write`, `InvalidPath`).
- `crates/envoy-accesslog/src/sink.rs` — doc-comment-only placeholder explaining option-(c) deferral.
- `crates/envoy-accesslog/src/default_format.rs` — placeholder for Task 3.
- `crates/envoy-accesslog/src/file_sink.rs` — placeholder (`pub struct FileSink;` stub for `pub use`).

**Workspace registration:** `Cargo.toml` `[workspace] members` += `"crates/envoy-accesslog"` (alphabetically first).

**Tests:** `cargo test -p envoy-accesslog` → `test result: ok. 2 passed; 0 failed` (the two `record::tests::*` cases).

**Workspace gates:** `cargo build --workspace --all-targets` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `cargo fmt --all -- --check` clean (after a `cargo fmt --all` pass — initial fmt-check reported drift in the auto-generated module ordering of `lib.rs`, struct-variant expansion in `error.rs`, and the long `assert!` wrap in `record.rs`; `cargo fmt --all` applied, re-verified clean per R-9).

**LoC:** ~165 LoC (the 5 module files + Cargo.toml + workspace `members` line).

## Task 3 — envoy-accesslog default_format emitter + ISO-8601 + Gregorian helper

Lands `crates/envoy-accesslog/src/default_format.rs` per SPEC §3 D1.2 + §6 signpost 1 (ISO-8601 emitter takes `&mut String`) + signpost 2 (Gregorian helper inline, not separate module) + signpost 9 (`%DURATION%` rendered in integer milliseconds).

**Functions landed:**
- `pub fn format(record: &AccessLogRecord) -> String` — 14-token Envoy default format, no trailing newline.
- `pub(crate) fn format_iso8601(s: &mut String, t: SystemTime)` — appends 24 ASCII bytes `YYYY-MM-DDTHH:MM:SS.sssZ`.
- `fn epoch_seconds_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32)` — Gregorian calendar arithmetic with full leap-year handling (4/100/400 rule).
- Helpers: `push_or_dash`, `is_leap_year`, `days_in_year`, `days_in_month`.

**Tests:** `cargo test -p envoy-accesslog --lib default_format::tests` → `test result: ok. 8 passed; 0 failed`. Test 5 (`format_iso8601_known_date`) validates the leap-day boundary (2024-02-29T12:34:56.789Z); test 6 (`epoch_seconds_to_ymd_hms_known_dates`) is table-driven across 7 known epochs including the Y2K leap day boundary + the year-2100-non-leap-year boundary + the Y2K38 boundary. `cargo test -p envoy-accesslog` full crate → `10 passed` (2 record + 8 default_format).

**Workspace gates:** `cargo build --workspace --all-targets` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (after two execution-time clippy fixups detailed below); `cargo fmt --all -- --check` clean (after a `cargo fmt --all` pass — initial fmt-check reported drift on three long-line assert! wraps + the days_in_year body single-line collapse; `cargo fmt --all` applied, re-verified clean per R-9); `cargo test --workspace --lib` 422 passed (no regression elsewhere).

**Execution-time deviations from PLAN's verbatim implementation (recorded for stranger-readability):**
1. **PLAN's `1709209096` constant is stale by 1000 seconds.** The PLAN's Step 1 test scaffold encodes `1_709_209_096` as the epoch seconds for `2024-02-29T12:34:56Z` (in both `format_iso8601_known_date` and the table-driven `epoch_seconds_to_ymd_hms_known_dates` Test 6). Independent verification via Python `datetime.fromtimestamp(1709209096, tz=utc).isoformat()` yields `2024-02-29T12:18:16+00:00`; the correct epoch seconds for `2024-02-29T12:34:56Z` is `1_709_210_096`. The implementation correctly decodes `1709209096` to `12:18:16` (the algorithm is sound; the test constant was wrong). Fixed by replacing `1_709_209_096` with `1_709_210_096` in both test sites. All other 6 epoch constants in the table verified correct against Python.
2. **`clippy::manual_is_multiple_of` (3 hits on `is_leap_year`).** The PLAN's verbatim predicate `(year % 4 == 0) && (year % 100 != 0 || year % 400 == 0)` trips this new (Rust 1.95+) lint. Rewrote using `year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))` — semantically identical; predicate ordering 4→100→400 preserved (load-bearing for the 2100-not-leap and 2000-is-leap tests; documented in a code comment).
3. **`clippy::type_complexity` on the test cases slice.** The PLAN's verbatim `&[(u64, (u32, u32, u32, u32, u32, u32))]` trips this lint. Resolved with a local `type YmdHms = (u32, u32, u32, u32, u32, u32);` inside the test fn (no public surface added).

**LoC:** ~310 LoC (149 impl + 160 tests; the test set is unusually dense per the SPEC §3 D1.2 14-test projection split across 8 tests in default_format + 2 in record + 4 in file_sink).

## Task 4 — envoy-accesslog FileSink

Lands `crates/envoy-accesslog/src/file_sink.rs` per SPEC §3 D1.2 + signpost 3 (`Arc<tokio::sync::Mutex<File>>` posture preserves append-semantic atomicity inside the process).

**API landed:**
- `pub struct FileSink { path, handle: Arc<Mutex<File>> }` (derives `Debug` — see deviation 1 below).
- `pub async fn FileSink::new(path: PathBuf) -> Result<Self, AccessLogError>` — opens with `append(true).create(true)`; maps `io::Error` → `AccessLogError::Open`.
- `pub async fn FileSink::emit(&self, record: &AccessLogRecord) -> Result<(), AccessLogError>` — formats via `default_format::format`, writes line + `\n` under the mutex, maps `io::Error` → `AccessLogError::Write`.

**Tests:** `cargo test -p envoy-accesslog --lib file_sink::tests` → `test result: ok. 4 passed; 0 failed`. The serialize-concurrent-emissions test spawns 10 concurrent emissions on one `Arc<FileSink>` and verifies the resulting file contains 10 complete lines with no interleaving.

**Crate-wide tests:** `cargo test -p envoy-accesslog` → `14 passed` (2 record + 8 default_format + 4 file_sink).

**Workspace gates:** `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (after deviation 2 below); `cargo fmt --all -- --check` clean (after a `cargo fmt --all` pass — initial fmt-check reported drift on the long-line `assert!` wrap in `file_sink_writes_one_record` plus the `AccessLogError::Open` struct-pattern block in `file_sink_emit_returns_error_on_invalid_path`; `cargo fmt --all` applied, re-verified clean per R-9); `cargo test --workspace --lib` 426 passed (no regression — +4 vs Task 3's 422 = the 4 new file_sink tests).

**Execution-time deviations from PLAN's verbatim implementation (recorded for stranger-readability):**
1. **`#[derive(Debug)]` on `FileSink`.** The PLAN's Step 1 test `file_sink_emit_returns_error_on_invalid_path` calls `.expect_err("expected open error")` on the `Result<FileSink, AccessLogError>` from `FileSink::new`. `expect_err` requires `Debug` on the `Ok` variant. The PLAN's Step 3 impl block defined `FileSink` without `#[derive(Debug)]`; added it. `Arc<tokio::sync::Mutex<File>>` + `PathBuf` both implement `Debug`, so the derive compiles cleanly. No semantic change.
2. **`SystemTime` unused-import lint.** The PLAN's Step 1 test scaffold imports `use std::time::{Duration, SystemTime, UNIX_EPOCH};` but `make_record()` only references `UNIX_EPOCH` (the field type for `AccessLogRecord::start_time` is inferred, so `SystemTime` does not need to be in scope). Trips `-D warnings` via the `unused_imports` lint. Removed `SystemTime` from the import — semantics-preserving (per doctrine, source fix preferred over `#[allow]`).
3. **fmt drift** (per R-9 disclosure requirement): `cargo fmt --all -- --check` flagged two sites after Step 3 impl landed — (a) the long-line `assert!(line.ends_with(...))` in `file_sink_writes_one_record` rewrapped to multi-line, (b) the inline `AccessLogError::Open { path: got_path, source: _ } => { ... }` arm rewrapped to a vertically-stacked struct-pattern block. `cargo fmt --all` applied; re-check clean.

**LoC:** ~225 LoC (~70 impl including derive + ~155 tests; the concurrent-emissions test alone is ~45 LoC including the spawned-task plumbing).

## Task 5 — envoy-config access_log schema + validator + fuzz seed

Lands the parse-side access-log schema per SPEC §3 D2.2 + PLAN-write SPEC correction 4 (the `#[serde(default)]` posture is load-bearing for the 5 existing HCM-bearing fixtures 0007/0008/0009/0010/0011, which do not declare an `access_log:` block).

**Types landed (in `crates/envoy-config/src/bootstrap.rs`):**
- `pub struct AccessLog { name: String, typed_config: AccessLogTypedConfig }` — `#[serde(deny_unknown_fields)]`; mirrors Envoy's `envoy.config.accesslog.v3.AccessLog`.
- `pub enum AccessLogTypedConfig { FileAccessLog(FileAccessLog) }` — `#[serde(tag = "@type", deny_unknown_fields)]`; single variant in 06.2 (file logger only). Unknown `@type` URLs surface as `ConfigError::Yaml` at serde-deserialize time; the validator does NOT re-check the URL. The enum exists so future observability phases can add stdout / gRPC / OpenTelemetry loggers without reshaping the schema.
- `pub struct FileAccessLog { path: String }` — `#[serde(deny_unknown_fields)]`; 06.2 consumes only `path` (format-string customization is OUT of scope per parent-06 SPEC §4 + 06.2 SPEC §4 — the emitter uses the default Envoy v3 format string).
- `HttpConnectionManagerConfig` gains `#[serde(default)] pub access_log: Vec<AccessLog>` — `default` is load-bearing per PLAN-write correction 4 (HCM carries `#[serde(deny_unknown_fields)]`, so omitting the field is the only way the 5 existing HCM-bearing fixtures can parse back-compat).

**ConfigError variants added (in `crates/envoy-config/src/lib.rs`):**
- `UnsupportedAccessLogType { actual: String }` — fired by the validator when `access_log[*].name != "envoy.access_loggers.file"`.
- `InvalidAccessLogPath` — fired by the validator when `FileAccessLog.path` is empty. The sink-side `FileSink::new` would also fail on `""`, but rejecting at parse time gives a clearer diagnostic.

**Re-exports:** `AccessLog`, `AccessLogTypedConfig`, `FileAccessLog` added to the `pub use bootstrap::{...}` block in `crates/envoy-config/src/lib.rs` (alphabetical insertion).

**Validator:** new free function `validate_access_logs(&[AccessLog]) -> Result<(), ConfigError>` hoisted next to `validate_http2_protocol_options_ranges` (same shape — mutates nothing, returns first error); called from `validate_hcm` after the http2-options range check and before the http_filters cardinality check. The hoisting style follows 05.3's `validate_http2_protocol_options_ranges` precedent.

**Tests (6 new in `bootstrap::tests`):**
- `parses_hcm_with_file_access_log` — happy path: file logger with `/tmp/access.log`; asserts the structural projection (name + path).
- `parses_hcm_with_no_access_log_block` — back-compat: HCM YAML with no `access_log:` key at all; `#[serde(default)]` produces empty Vec.
- `parses_hcm_with_empty_access_log_array` — `access_log: []` parses to empty Vec.
- `rejects_hcm_with_unsupported_access_log_name` — `name: envoy.access_loggers.stdout` → `ConfigError::UnsupportedAccessLogType { actual: "envoy.access_loggers.stdout" }`.
- `rejects_hcm_with_unsupported_access_log_type_url` — unknown `@type` URL → either `ConfigError::Yaml` (serde-tagged-enum rejection) or `UnsupportedAccessLogType`; both paths accepted.
- `rejects_hcm_with_empty_access_log_path` — empty `path: ""` → `ConfigError::InvalidAccessLogPath`.

**Corpus walk + fuzz seed:**
- New seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_access_log_file.yaml` (full HCM with `access_log` block).
- `fuzz_corpus_seeds_parse_or_reject_cleanly` (acceptance loop in `bootstrap::tests`) extended with the new seed.
- `crates/envoy-config/fuzz/.gitignore` allow-listed `!corpus/parse_bootstrap/hcm_access_log_file.yaml`.

**Tests:** `cargo test -p envoy-config` → `174 passed; 0 failed` (+6 vs Task 4: the 5 new validator tests + the corpus-walk read-back of the new seed; the 6th new test, `parses_hcm_with_no_access_log_block`, replaces no prior test — net is +6 envoy-config tests). Workspace lib regression: `cargo test --workspace --lib` → `432 passed; 0 failed` (+6 vs Task 4's 426 = the new envoy-config tests; envoy-accesslog and other crates unchanged).

**Workspace gates:** `cargo build --workspace --all-targets` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (no new warnings on the single-variant `AccessLogTypedConfig` enum — clippy treats the `#[serde(tag = "@type")]` posture as legitimate); `cargo fmt --all -- --check` clean (after a `cargo fmt --all` pass — disclosure per R-9 below).

**Execution-time deviations from PLAN's verbatim implementation (recorded for stranger-readability):**
1. **Test-prescribed `match &filter.typed_config` adjusted to `match &filter.typed_config { Some(TypedConfig::HttpConnectionManager(h)) => h, ... }`.** The PLAN's verbatim test code matches `TypedConfig::HttpConnectionManager(h)` directly. `NetworkFilter::typed_config` is actually `Option<TypedConfig>` (introduced pre-Task-5 to make `typed_config` optional for filters that don't carry one). Adapted in `parses_hcm_with_file_access_log` and `parses_hcm_with_no_access_log_block` to match `Some(TypedConfig::HttpConnectionManager(h))`. Pure ergonomic adaptation; the structural assertion still verifies it's an HCM.
2. **Pre-existing test `rejects_unknown_field_in_hcm_config` sentinel update.** This test (added pre-06.2) used `access_log: []` as a "definitely-not-an-HCM-field" sentinel to prove `HttpConnectionManagerConfig`'s `#[serde(deny_unknown_fields)]` rejects unrecognized fields. Task 5 added `access_log` to the schema, so the original sentinel is no longer unknown. Swapped to `bogus_hcm_field: 1` (a name reserved for sentinel duty, mirroring the `bogus_ep_field` pattern in `rejects_endpoint_with_unknown_field` at ~line 2360). The test's intent is preserved; both the YAML and the assertion message updated.
3. **Mechanical struct-literal compat fixes in envoy-http1 + envoy-http2.** Adding the new `access_log` field to `HttpConnectionManagerConfig` broke 5 struct-literal construction sites in test code that initialize the struct directly: 1 site in `crates/envoy-http1/src/hcm.rs` and 4 sites in `crates/envoy-http2/src/hcm.rs`. Each got a one-liner `access_log: vec![],` (mirroring the existing `http2_protocol_options: None,` line). These are pure mechanical zero-semantics fixes required for green build per D-3.6; the actual access-log wiring lands in Task 6 (H1) / Task 7 (H2). Each site carries a `// 06.2 Task 5:` doc-comment explaining the placeholder.
4. **fmt drift** (per R-9 disclosure requirement): `cargo fmt --all -- --check` flagged drift on the re-export block in `crates/envoy-config/src/lib.rs` after inserting `AccessLog, AccessLogTypedConfig, FileAccessLog` (the line wraps shifted). `cargo fmt --all` applied; re-verified clean.

**LoC:** ~265 LoC across 5 files (~95 impl: types + validator + ConfigError variants + re-exports; ~140 tests: 6 new tests + corpus-walk extension; ~30 placeholder one-liners in envoy-http1/envoy-http2 hcm.rs).

## Task 6 — HCM H1 access-log wiring + factored dispatch site

Lands the H1 dispatch path's runtime integration with the access-log subsystem per SPEC §3 D3.2 + PLAN-write SPEC correction 1 (the **factored dispatch site** consolidating the 5 writer outcomes into one access-log emission site at the end of `serve_connection`'s per-request loop iteration).

**HCMConfig surface change (`crates/envoy-http1/src/hcm.rs`):**
- New field `pub access_log: Vec<Arc<envoy_accesslog::FileSink>>` (empty by default; non-empty when the listener YAML carries an `access_log:` block).
- `HCMConfig::from_config` promoted to `async fn` (because `envoy_accesslog::FileSink::new` is async — `tokio::fs::OpenOptions::open` is async). Opens each configured `FileAccessLog` sink at config-load time; surfaces failures as the new `Http1Error::AccessLogOpen { message: String }` variant.

**Http1Error addition (`crates/envoy-http1/src/error.rs`):**
- New variant `AccessLogOpen { message: String }`. Mirrors the 06.1 `StatsRegistration { stat_prefix, message: String }` String-wrapping precedent (the field is `message` not `source` because `thiserror` treats a field named `source` as a nested `std::error::Error` and `String` does not implement `Error` — disclosed below as deviation 2).

**Factored dispatch site (`crates/envoy-http1/src/hcm.rs`):**
- Per-request state locals (`response_status_for_log`, `response_body_len`, `upstream_host_for_log`, `response_headers_for_log`) declared before the `match outcome { ... }` block; each of the 5 writer arms populates them before fall-through.
- The proxy arm's pre-Task-6 `if close { return Ok(()); } continue;` short-circuits (on cluster-no-endpoint / connect-fail / request-fail) are restructured into a single `if let Some(endpoint) = endpoint { ... }` block that always falls through to the dispatch site. The post-match keep-alive `if close { return Ok(()); }` is preserved unchanged.
- Three private helpers next to `parse_content_length`: `x_envoy_original_path_or_path` (REQ(X-ENVOY-ORIGINAL-PATH?:PATH)), `access_log_header_value` (case-insensitive lookup returning owned String), `extract_upstream_service_time` (parses upstream's `x-envoy-upstream-service-time` ms-integer header into a `Duration`).
- Request-arrival timing captured via `Instant::now()` (for `duration`) + `SystemTime::now()` (for `%START_TIME%`) immediately after request-parse-success.
- Per parent-06 SPEC §6 architectural Rule 4 option (b): **synchronous-after-write** dispatch; emission errors logged via `tracing::warn!` and discarded. Never propagates.

**Tests (4 new in `hcm::tests`):**
- `hcm_with_no_access_log_does_not_touch_filesystem` — empty `access_log` Vec; no file created.
- `hcm_with_file_access_log_writes_one_line_per_request` — single 200 direct-response request; asserts one line per sink with the known formatter suffix `"GET / HTTP/1.1" 200 - 0 3 `.
- `hcm_with_file_access_log_emission_failure_does_not_fail_request` — pre-constructed FileSink wrapping a deliberately read-only `tokio::fs::File`; verifies `serve_connection` returns Ok AND the captured warn line contains `access log emission failed` (the WarnCapture fixture installs a `tracing-subscriber` layer via `set_default` per-thread; the `#[tokio::test]` default current-thread runtime keeps the spawned task on the same thread so the subscriber applies).
- `hcm_records_protocol_as_http1_1_on_h1_path` — asserts the emitted line contains `HTTP/1.1`.

Test fixture (in-test only): `WarnCapture` + `CaptureWriter` (thread-safe `Arc<Mutex<Vec<String>>>`-backed `tracing-subscriber` layer); `hcm_config_with_access_log` (HCMConfig builder taking pre-built sinks); `serve_one_request_with_access_log` (drives one request, returns per-sink lines); `serve_one_request_with_pre_constructed_sinks` (variant returning the Result for failure-mode assertion).

**Tests:** `cargo test -p envoy-http1` → `test result: ok. 48 passed; 0 failed` (+4 vs Task 5's 44 = the 4 new access-log dispatch tests).

**Workspace gates:** `cargo build --workspace --all-targets` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (after deviation 5 below); `cargo fmt --all -- --check` clean (after a `cargo fmt --all` pass — disclosure per R-9 below); `cargo test --workspace --lib` → `436 passed` (+4 vs Task 5's 432 — only the 4 new envoy-http1 tests; no regression elsewhere); `cargo test -p differential` → all 14 fixtures pass including the 5 HCM-bearing fixtures `0007-http1-direct-response`, `0008-http1-router-upstream`, `0009-http2-direct-response`, `0010-http2-router-upstream`, `0011-admin-stats-prometheus`.

**Cross-crate ripples (disclosed per task scope):**
- **`envoy-bin`:** the one `HCMConfig::from_config(...)` caller in `crates/envoy-bin/src/main.rs` (inside `async fn run`) added `.await` plus enclosing parens.
- **`envoy-http2`:** 4 `Http1HCMConfig::from_config(...)` callers in `crates/envoy-http2/src/hcm.rs::tests` added `.await`. Two of those callers were inside test-private helpers `synth_h2_hcm_config()` and `synth_h2_hcm_config_proxy(cluster_mgr)`; these helpers were sync and called from async test bodies — promoted both to `async fn` and updated the 7 call sites (`spawn_h2_hcm(synth_h2_hcm_config()).await` → `spawn_h2_hcm(synth_h2_hcm_config().await).await`; 5 calls; and `synth_h2_hcm_config_proxy(cluster_mgr)` → `synth_h2_hcm_config_proxy(cluster_mgr).await`; 2 calls).
- **`envoy-http1` (in-tree):** the one `HCMConfig::from_config(...)` test caller in `hcm.rs::tests::hcm1_increments_downstream_rq_total_on_request` added `.await`.

**Dev-deps added (`crates/envoy-http1/Cargo.toml`):** `tempfile = "3"` (for the temp directory shape used by the 4 new tests + WarnCapture's tempdir-allocation); `tracing-subscriber = "0.3"` (for `tracing-subscriber::registry()` + `fmt::layer()` in the WarnCapture fixture). Both permitted by D-3.2's foundations list.

**Path-dep added (`crates/envoy-http1/Cargo.toml`):** `envoy-accesslog = { path = "../envoy-accesslog" }`.

**API addition in envoy-accesslog (`crates/envoy-accesslog/src/file_sink.rs`):** new `#[doc(hidden)] pub fn FileSink::from_file_for_test(path: PathBuf, file: File) -> Self`. Test-only constructor wrapping a pre-opened `tokio::fs::File`. Disclosed in deviation 4 below — required to make the emission-failure test portable (POSIX semantics keep an open FD writable after parent-dir unlink on both macOS and Linux, so the dir-drop-then-write-fails trick the PLAN's verbatim test originally used is unreliable). The constructor is `#[doc(hidden)]` so it's effectively private but reachable from the in-tree envoy-http1 test code.

**Execution-time deviations from PLAN's verbatim implementation (recorded for stranger-readability):**

1. **envoy-accesslog had a stale `envoy-http1` dep that caused a cyclic-package-dependency error when envoy-http1 added the `envoy-accesslog` path-dep.** `crates/envoy-accesslog/Cargo.toml` carried a leftover `envoy-http1 = { path = "../envoy-http1" }` line from Task 2 (likely scaffolded for early prototyping; no `envoy_http1::` symbol use in envoy-accesslog source — only doc-comment mentions). The cycle surfaced as `cyclic package dependency: package 'envoy-accesslog v0.0.0' depends on itself ... satisfies path dependency 'envoy-http1' (locked to 0.0.0) of package 'envoy-accesslog'` at `cargo build -p envoy-http1 --tests`. Removed the stale dep from envoy-accesslog (per the cross-sub-phase architecture: envoy-accesslog is the foundation; envoy-http1 consumes; not the reverse). Zero-semantics fix in envoy-accesslog (no symbol use to migrate).

2. **`Http1Error::AccessLogOpen` field renamed `source` → `message`.** PLAN's Step 5 verbatim uses `AccessLogOpen { source: String }`. `thiserror` treats a field named `source` as a nested `std::error::Error` and generates an `as_dyn_error` call on it; `String` does not implement `Error`, so the derive fails with `method 'as_dyn_error' cannot be called on '&String' due to unsatisfied trait bounds`. Renamed to `message: String` matching the 06.1 `StatsRegistration { stat_prefix, message: String }` precedent. The display string in the `#[error("failed to open access log sink: {message}")]` attribute uses `{message}` accordingly; the error message is functionally identical to the PLAN's intended shape.

3. **`endpoint` match arm restructure for the access-log fall-through.** The pre-Task-6 cluster-no-endpoint arm of the proxy path used `return Ok(())` / `continue` to short-circuit, but the access-log dispatch site requires that all 5 writer outcomes fall through to a single emission site. Restructured the inner `let endpoint = match cluster.pick_endpoint() {...}` to return `Option<SocketAddr>` (Some on the happy path; None on the 503-synth path, with the 503 response written and state populated before fall-through). The body then runs `if let Some(endpoint) = endpoint { ... }` for the live-endpoint subpath; the no-endpoint case falls through to the dispatch site naturally. Functionally identical to the pre-Task-6 control flow (the 503 path still returns to the loop after the dispatch site); only the local control-flow shape changes.

4. **Test-only `FileSink::from_file_for_test` added to envoy-accesslog.** PLAN's Step 2 test `hcm_with_file_access_log_emission_failure_does_not_fail_request` does `drop(dir);` after `FileSink::new(path)` to remove the file's parent, expecting subsequent `write_all` on the open FD to fail. Verified out-of-band against `tokio::fs::OpenOptions::new().append(true).create(true).open()`: on macOS (POSIX), the open FD remains writable after parent-dir unlink (the inode survives until the last FD closes; writes succeed and data is appended to the orphan inode). The test as PLAN-specified is unreliable on macOS / Linux / any POSIX filesystem. Added a `#[doc(hidden)] pub fn FileSink::from_file_for_test(path: PathBuf, file: File) -> Self` constructor wrapping a pre-opened `tokio::fs::File`; the test then opens a `File` in read-only mode (after a one-shot `tokio::fs::File::create` to touch the file), passes it into the test-only constructor, and the resulting FileSink's `emit` reliably fails at `write_all` with `AccessLogError::Write` on every platform.

5. **Clippy lint `clippy::cloned_ref_to_slice_refs` (Rust 1.95+).** The PLAN's verbatim `serve_one_request_with_access_log(&[path.clone()])` (the test passes a single-element slice constructed from cloning the path) trips this new lint. Replaced with `serve_one_request_with_access_log(std::slice::from_ref(&path))` at both use sites — semantically identical, no `.clone()` allocation.

6. **fmt drift** (per R-9 disclosure requirement): `cargo fmt --all -- --check` flagged drift on two sites after the Step-6 refactor — (a) the long `crate::router::X_ENVOY_UPSTREAM_SERVICE_TIME.to_string()` line in `serve_connection` (the multi-line wrap fmt prefers single-line), (b) the call site in `crates/envoy-http2/src/hcm.rs::h2_proxy_outcome_dispatches_to_h1_upstream_when_cluster_is_http1` whose `let (listener_addr, _hcm) = spawn_h2_hcm(synth_h2_hcm_config_proxy(cluster_mgr).await).await;` line crossed 100 col after the `.await` insertion. `cargo fmt --all` applied; re-verified clean.

**LoC:** ~360 LoC across 5 files (envoy-http1: ~85 impl in `serve_connection` refactor + helpers + ~150 in test body and helpers; envoy-accesslog: ~15 for the test-only constructor; error.rs: ~10 for the AccessLogOpen variant; envoy-bin: ~5 for the `.await` ripple; envoy-http2: ~10 for the 4 `.await` ripples + 2 sync→async helper promotions + their 7 call-site updates).
