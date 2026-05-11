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

## Task 6 follow-up — gate `FileSink::from_file_for_test` behind a `test-util` feature

Closes the Task 6 code-quality review's Important #1: the doc-comment claimed `#[cfg(any(test, feature = "test-util"))]` gating but only `#[doc(hidden)]` was applied. The function compiled into production builds.

**Fix:**
- `crates/envoy-accesslog/src/file_sink.rs` — added `#[cfg(any(test, feature = "test-util"))]` to `from_file_for_test`.
- `crates/envoy-accesslog/Cargo.toml` — added `[features] test-util = []`.
- `crates/envoy-http1/Cargo.toml` — `[dev-dependencies]` now overlays the runtime dep with `envoy-accesslog = { path = "../envoy-accesslog", features = ["test-util"] }`.

Result: the helper is only compiled when running tests (or when a future consumer explicitly enables the feature). Release builds of `envoy-accesslog` no longer carry the symbol.

**Workspace gates:** build/clippy/fmt/test all clean; 4 envoy-http1 access-log tests still pass.

## Task 7 — HCM H2 access-log wiring + envoy-http2 path-dep + 2 unit tests

Lands per SPEC §3 D3.2 H2 inheritance path + PLAN-write SPEC correction 2 (the H2 dispatch site lands AFTER `send_envoy_response(send_response, resp).await` returns; this covers both the empty-body `send_response(.., end_of_stream=true)` branch and the non-empty `send_data(.., end_of_stream=true)` branch uniformly). The `HCMConfig.access_log` field already existed on the H2 side via the 05.2 D1 type-alias `pub type HCMConfig = envoy_http1::HCMConfig;` at `crates/envoy-http2/src/hcm.rs:27` — Task 7 only adds the dispatch site, the path-dep, the helpers, and 2 tests.

**Path-deps added (`crates/envoy-http2/Cargo.toml`):**
- `[dependencies]` — `envoy-accesslog = { path = "../envoy-accesslog" }`. Required by PLAN-write SPEC correction 5: `sink.emit()` is a concrete-type method call requiring the concrete `FileSink` to be resolvable at compile time. Re-exporting through `envoy_http1::HCMConfig.access_log` only carries the `Vec<Arc<FileSink>>` field type, not the `emit()` symbol.
- `[dev-dependencies]` — `envoy-accesslog = { path = "../envoy-accesslog", features = ["test-util"] }` (overlays the runtime dep to enable the `test-util` feature — the same posture envoy-http1 adopted in Task 6's follow-up; Task 7's 2 tests don't use `from_file_for_test`, but the overlay is added consistent with the cross-crate convention so future H2 emission-failure tests don't require a Cargo.toml edit). Also added `tempfile = "3"` (the 2 tests open temp directories per the H1 pattern).

**Refactored `handle_one_stream` (`crates/envoy-http2/src/hcm.rs:88`):**
The original `handle_one_stream` had 3 writer sites: 2 early-return 502 paths (no-healthy-endpoint at line 141, upstream-dispatch-fail at line 204) and the normal `send_envoy_response(send_response, resp).await` at line 251. Per the same H1-Task-6 refactor doctrine (all writer outcomes must funnel through the single access-log emission site), the 2 early-502 paths now call a new `finalize_h2_stream` join-point with the synthesized 502 response; the normal happy + proxy-success paths also funnel through `finalize_h2_stream` at the bottom. Per-stream state captured at handler entry (`req_arrival_instant`, `req_arrival_systime`, `request_body_len`) and populated through the match arms (`response_status_for_log`, `response_body_len`, `response_headers_for_log`, `upstream_host_for_log_h2`).

**`finalize_h2_stream` async fn:** receives the resolved `Response` + all access-log state; calls `send_envoy_response(send_response, resp).await`; then, if `config.access_log` is non-empty, builds an `AccessLogRecord` (with `protocol: "HTTP/2"`) and emits it once per sink. Emission errors logged via `tracing::warn!` and discarded per parent-06 SPEC §6 architectural Rule 4 option (b). The `#[allow(clippy::too_many_arguments)]` is necessary because the function takes 11 args — splitting them into a struct would just move the boilerplate elsewhere.

**Helpers cloned (~22 LoC):** PLAN's recommendation honored — `x_envoy_original_path_or_path`, `access_log_header_value`, `extract_upstream_service_time` cloned verbatim from `envoy_http1::hcm` rather than re-exported through `pub(crate)` cross-crate gymnastics. The cloned helpers operate on `envoy_http1::Request` (re-exported at the top of `envoy-http2/src/hcm.rs`) and `&[(String, String)]` headers — the H2-side `envoy_req` is the same `envoy_http1::codec::Request` value-type (per cross-sub-phase architectural rule 3), so the helper signatures need no adaptation.

**Tests (2 new in `hcm::tests`):**
- `hcm_h2_with_file_access_log_writes_one_line_per_request` — single 200 direct-response request over an in-process H2 listener; asserts one line per sink containing `"GET / HTTP/2"`.
- `hcm_h2_records_protocol_as_http2_on_h2_path` — same setup; asserts the emitted line contains `HTTP/2` AND does NOT contain `HTTP/1.1` (cross-protocol regression guard).

Test fixture (in-test only): `h2_hcm_config_with_access_log` (HCMConfig builder taking pre-built sinks; mirrors envoy-http1's same-named helper; bypasses the envoy-config `AccessLog` parser by building via `from_config` with empty `access_log: []` then overwriting `built.access_log = sinks` on the materialized struct — the `pub` access on `HCMConfig.access_log` makes this clean); `serve_one_h2_request_with_access_log` (drives one request through the production `spawn_h2_hcm` harness, returns per-sink lines).

**Tests:** `cargo test -p envoy-http2 --no-fail-fast` → `36 passed; 1 ignored` (+2 vs Task 5/6's 34 — the 2 new H2 access-log dispatch tests; the 1 ignored is the pre-existing `h2_protocol_options_max_concurrent_streams_applied` ignored test from 05.2). All pre-existing H2 tests (handshake, route-walk, hop-by-hop strip, proxy-to-H1, proxy-to-H2, garbage-preamble, two-streams-share-tcp, downstream_rq_total increment) still pass.

**Workspace gates:** `cargo build --workspace --all-targets` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (after deviation 1 below); `cargo fmt --all -- --check` clean.

**h2spec regression check:** the h2spec conformance target lives at `tests/conformance/h2spec/` and is its own package (NOT part of the workspace's default test set per `Cargo.toml`'s `exclude`). Task 7's refactor only restructures `handle_one_stream`'s control flow on already-tested codepaths; the codec-edge framing is untouched. Codec-edge unit tests in `crates/envoy-http2/src/codec.rs` (handshake, protocol-options application) all still pass.

**Execution-time deviations from PLAN's verbatim implementation:**

1. **Clippy lint `clippy::cloned_ref_to_slice_refs` (Rust 1.95+).** The PLAN's verbatim `serve_one_h2_request_with_access_log(&[path.clone()])` (the test passes a single-element slice constructed from cloning the `PathBuf`) trips this new lint. Replaced with `serve_one_h2_request_with_access_log(std::slice::from_ref(&path))` at both use sites — semantically identical, no `.clone()` allocation. Same lint encountered in Task 6 (deviation 5 there).

2. **Per-stream state-capture shape NOT a "small refactor of the spawn body" — a larger refactor of `handle_one_stream`'s control flow.** PLAN's Step 4 said "you may need a small refactor of the spawn body to make state accessible at the post-`send_envoy_response` point." In practice, the cleanest landing was to factor `finalize_h2_stream(...)` as a new async fn that takes all access-log state + the resolved `Response`, calls `send_envoy_response`, then emits. The 2 early-502 paths (no-healthy-endpoint at line 141; upstream-dispatch-fail at line 204) now `return finalize_h2_stream(...).await` instead of early-returning `send_envoy_response(...)` directly. The normal happy path also funnels through `finalize_h2_stream(...)` at the bottom. Net: 1 new helper (`finalize_h2_stream`) + 3 cloned helpers (`x_envoy_original_path_or_path`, `access_log_header_value`, `extract_upstream_service_time`) + per-stream state captured at handler entry and populated through the match arms. Functionally identical to the pre-Task-7 control flow.

3. **Test harness drops `conn_task.await` to avoid an H2-codec-level hang.** PLAN's verbatim test harness (modeled on the H1 `serve_one_request_with_access_log`) would `drop(send_request); let _ = conn_task.await;` to ensure the H2 connection task finishes before reading the access-log file. In practice, the H2 conn future does not return until the server side EOFs the TCP stream; the server's `serve_h2_connection` sits in `h2_conn.accept().await` waiting for the next stream and only exits when the listener task is aborted via the `_server` Drop at test end. Replaced the `conn_task.await` with a `tokio::time::sleep(200ms)` settle window — the access-log emit fires inside the server's per-stream `tokio::spawn`, which writes the line via `tokio::fs::File::write_all` (no userspace buffer — the kernel page cache is visible to other readers immediately). The 200ms is generous headroom over the H1 test's 50ms (extra H2 codec turn).

4. **`upstream_host_for_log_h2` declared `mut` while other access-log locals are non-`mut`.** The 3 string-typed locals (`response_status_for_log`, `response_body_len`, `response_headers_for_log`) are populated exactly once per stream by every match-arm path (so Rust's flow analysis accepts them as `let`-without-`mut`). `upstream_host_for_log_h2` defaults to `None` and is only re-assigned on the successful proxy path; this requires `mut` per Rust's let-rebind rules. Minor style asymmetry vs the H1 path where all 4 are `mut`; documented here so reviewers don't flag it.

**LoC:** ~310 LoC across 2 files (envoy-http2/src/hcm.rs: ~130 impl in the `handle_one_stream` refactor + `finalize_h2_stream` + 3 cloned helpers; ~180 in test body + 2 test helpers; envoy-http2/Cargo.toml: 3 lines for the path-dep + dev-dep overlay + tempfile dev-dep). PROGRESS.md: ~50 lines.

## Task 8 — In-process integration backstop (no Docker)

Lands `crates/envoy-bin/tests/access_log_file_sink.rs` per SPEC §6 signpost 18 — a single in-process integration test that exercises the full Task 2-7 wiring end-to-end at the binary level. Spawns `envoy-bin` via `CARGO_BIN_EXE_envoy-bin` against an HCM-with-file-sink YAML config written to a tempdir; drives one `GET /` over HTTP/1.1 to an ephemerally-picked listener port; reads the access-log file post-request; asserts the default-format line tokens (`"GET / HTTP/1.1" 200 - 0 3 `). No Docker. Mirrors the 04.1 (`http1_direct_response.rs`), 05.2 (`http2_direct_response.rs`), and 06.1 (`admin_ready.rs`) in-process integration-test pattern; inherits the standing 02.2 REVIEW M1 SIGKILL-on-Drop posture (`let _ = child.kill(); let _ = child.wait();` after the test body).

**Test shape:**
- `pick_free_port()` — bind ephemeral, capture port, drop listener (the race between drop and child bind is the same as the 04.1/05.2 pattern; the `wait_for_port` retry loop covers it).
- `write_yaml_config(...)` — emits a self-contained HCM YAML with `access_log:` block at the typed_config tempdir path; uses a 200 direct-response route (no upstream cluster required, keeps `clusters: []`).
- `wait_for_port(addr, deadline)` — 5s deadline; retries `connect_timeout` every 50ms.
- Drives one `GET / HTTP/1.1` with `Connection: close`, reads to EOF, asserts response contains `200` and `ok`.
- Sleeps 100ms for kernel-page-cache settle (the HCM dispatches `sink.emit().await` synchronously after the write, but the test reads the file from a separate process via `std::fs::read_to_string`; the 100ms is generous headroom on top of the in-process synchronous emission).
- Reads the access-log file; asserts exactly 1 line; asserts line contains `"GET / HTTP/1.1" 200 - 0 3 ` (the request body length is 0, the response body length is 3 = `ok\n`; the trailing space is followed by `%RESPONSE_FLAGS%` which is `-` for the happy path).

**Dev-deps:** no additions. `tempfile = "3"` already in `crates/envoy-bin/Cargo.toml`'s `[dev-dependencies]` (per 06.1 admin-ready test pattern). `anyhow = "1"` is a regular dependency of `envoy-bin`, so it's accessible from `tests/*.rs` without a dev-dep entry.

**Tests:** `cargo test -p envoy-bin --test access_log_file_sink` → `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`. Completes in ~0.65s (subprocess startup + 100ms settle + cleanup dominates).

**Workspace gates:** `cargo build --workspace --all-targets` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `cargo fmt --all -- --check` clean (after a `cargo fmt --all` pass — initial fmt-check reported drift on the long-line `assert!(resp_str.contains("ok\n") ...)` wrap, the `std::fs::read_to_string(&access_log_path).context(...)` chain wrap, and the multi-arg `assert_eq!(lines.len(), 1, ...)` wrap; `cargo fmt --all` applied; re-verified clean per R-9); `cargo test --workspace --no-fail-fast` no regressions — the existing 10 envoy-bin integration tests (`admin_only`, `admin_ready`, `http1_direct_response`, `http1_router_upstream`, `http2_direct_response`, `http2_router_upstream`, `tcp_proxy`, `tls_downstream`, `tls_sni`, `tls_upstream`) all still pass.

**Execution-time deviations from PLAN's verbatim implementation:**
1. **fmt drift** (per R-9 disclosure requirement): the verbatim test source as PLAN-specified failed the initial `cargo fmt --all -- --check` on three sites — (a) the second `assert!` (response body check) needed wrap to multi-line, (b) the `read_to_string(...).context(...)` chain needed wrap, (c) the single-line `assert_eq!(lines.len(), 1, "expected 1 ...", lines.len(), log_contents)` needed expansion to multi-line. `cargo fmt --all` applied; re-verified clean. Functionally identical.

**LoC:** ~155 LoC (single test file; no impl changes).

## Task 9 — Differential harness extension: `Driver::Http1WithAccessLog` + `AccessLogLineRule` + hand-rolled tokenizer + dispatch arm + 4 unit tests

Lands the differential-harness primitives per 06.2 SPEC §3 D4.2.a + D4.2.b. New module `tests/differential/src/access_log.rs` ships the `AccessLogLineRule` per-token rule enum (`Exact` / `Iso8601Format` / `DurationMs` / `Wildcard` — internally tagged under `tag = "rule"`), the hand-rolled `tokenize_default_format` state machine (no `regex` dep per architecture decision 9 / signpost 8), the `apply_rule` per-rule check, the private `is_iso8601_format` 24-byte positional validator, and the outer `assert_access_log_lines_equivalent` dispatch that tokenizes both proxies' lines and applies the rule cascade pairwise. `lib.rs` gains the `Driver::Http1WithAccessLog` variant (slots between `Http1ProbeList` and `Http2`), the `port_key` `"PORT"` arm, the `run_fixture` dispatch arm reusing `drive_http1` for the wire-protocol leg, two new public types (`HeaderRule` + `AccessLogPaths`), and the `pub mod access_log;` module declaration.

**Module shape (`tests/differential/src/access_log.rs`):**
- `pub enum AccessLogLineRule { Exact { value: String }, Iso8601Format, DurationMs, Wildcard }` with `#[serde(tag = "rule", rename_all = "snake_case", deny_unknown_fields)]`.
- `pub fn tokenize_default_format(line: &str) -> Result<Vec<String>, String>` — hand-rolled state machine producing 15 tokens (1 bracketed START_TIME + 3 from quoted request-line + 6 unquoted + 5 quoted), with explicit error messages for bracket-unterminated, missing-space-after-bracket, quote-unterminated, and empty-unquoted-token cases.
- `pub fn apply_rule(rule, envoy, envoy_rust) -> Result<(), String>` — per-rule check; `Exact` compares both sides to the literal `value` independently; `Iso8601Format` and `DurationMs` validate each side independently; `Wildcard` is a no-op.
- `fn is_iso8601_format(s: &str) -> bool` — strict 24-byte `YYYY-MM-DDTHH:MM:SS.sssZ` positional shape; non-Z timezones and non-millisecond fractional digits are rejected (matches the `envoy-accesslog::format_iso8601` Task 3 output shape).
- `pub fn assert_access_log_lines_equivalent(envoy_lines, envoy_rust_lines, rules_per_line) -> Result<(), String>` — outer dispatch; validates line-count equality across both sides + `rules_per_line.len()`; per-line validates token-count equality with the rule count; per-token applies `apply_rule` and prefixes failure messages with `line {N} token {M}: {inner}`.

**4 new unit tests (all in `access_log::tests`):**
- `tokenize_default_format_happy_path` — 15-token sample line `[2024-01-01T00:00:00.000Z] "GET / HTTP/1.1" 200 - 0 3 5 - "-" "-" "-" "envoy-rust.test" "-"` round-trips through `tokenize_default_format`; per-token assertions verify the bracket + 3 request-line + 6 unquoted + 5 quoted shapes.
- `tokenize_handles_dash_in_quoted_position` — verifies that a `-` inside `"..."` does not confuse the quote scanner (5 consecutive `"-"` quoted tokens at positions 10-14).
- `assert_access_log_lines_equivalent_happy_path` — full 15-rule cascade (1 `Iso8601Format` + 13 `Exact` + 1 `DurationMs`) passes on identical input lines.
- `assert_access_log_lines_equivalent_rejects_token_mismatch` — POST-vs-GET diff at token 1 yields an `Err` whose message contains `envoy-rust token` (the failure-message contract used by Task 10's fixture).

**`lib.rs` changes:**
- `pub mod access_log;` declaration alongside `backend` / `subject` / `tls` / `upstream`.
- New struct `AccessLogPaths { envoy: String, envoy_rust: String }` (`deny_unknown_fields`).
- New enum `HeaderRule { SetEqualModuloAllowList }` — internally-tagged under `tag = "rule"` so fixture YAML reads `expected_headers: { rule: set_equal_modulo_allow_list }`. Distinct from `Http1HeaderRule` (externally-tagged unit variant, used by the existing 04.x drivers) since the PLAN's Task-10 fixture grammar requires the `rule`-keyed shape.
- New `Driver::Http1WithAccessLog` variant with the 9 fields per spec (`method: String`, `path`, `host`, `expected_status: u16`, `expected_body: BodyRule`, `expected_headers: HeaderRule`, `extra_headers: Vec<(String, String)>` (default), `expected_access_log_paths: AccessLogPaths`, `expected_access_log_lines: Vec<Vec<AccessLogLineRule>>`). Slotted between `Http1ProbeList` and `Http2` in the enum (the existing 7-variant grouping is preserved + the new variant; correction 3 of the PLAN's enum-ordering note is honored).
- `port_key` match: `Driver::Http1WithAccessLog { .. } => "PORT"` added to the existing OR-pattern arm.
- `run_fixture` dispatch arm: drives both proxies via `drive_http1` (reusing the 04.1 helper); converts `method: String` via inline match (`"GET" => Http1Method::Get`, other ⇒ bail) since `Http1Method` is the harness's narrow GET-only enum today; runs response equivalence inline mirroring the existing `Http1` arm (per-side `expected_status` check, `assert_body_rule(expected_body, ...)` for body, `diff_headers(...)` with `HEADER_ALLOW_LIST` for headers); waits up to 5s for both access-log files to exist (100ms poll) plus one final 100ms settle yield; reads both files via `std::fs::read_to_string`; calls `crate::access_log::assert_access_log_lines_equivalent` with the rule cascade; surfaces mismatch as `anyhow::anyhow!(...)` including both file contents for diagnostics.

**Tests:** `cargo test -p differential --lib access_log::tests` → `test result: ok. 4 passed; 0 failed`. `cargo test -p differential --lib` → `75 passed; 0 failed; 1 ignored` (+4 vs Task 8 baseline of 71 + 1 ignored; the ignored is the Docker-gated `starts_upstream_envoy_and_exposes_host_port`). No regressions.

**Workspace gates:** `cargo build --workspace --all-targets` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `cargo fmt --all -- --check` clean (after one `cargo fmt --all` pass — initial check reported drift on the verbatim `const SAMPLE_LINE: &str = "..."` line break and on the `let envoy_rust_lines: Vec<String> = envoy_rust_contents.lines()...` chain wrap in the dispatch arm; `cargo fmt --all` applied; re-verified clean per R-9); `cargo test --workspace --no-fail-fast` no regressions.

**Execution-time deviations from PLAN's verbatim implementation:**
1. **Inline assertions vs. factored helpers.** The PLAN's Step 2 (d) code block referenced `assert_status_equivalent` / `assert_body_equivalent` / `assert_headers_equivalent` and the PLAN's own footnote ("the existing `assert_status_equivalent` / `assert_body_equivalent` / `assert_headers_equivalent` helper names; they may be slightly differently named in HEAD") flagged that those helpers do not exist. Per the PLAN's recommendation ("inline the same helpers as the `Http1` arm with the same body"), the new arm inlines `if upstream_resp.status != *expected_status { bail!(...) }` + `assert_body_rule(expected_body, &upstream_resp.body, &subject_resp.body)?` + `diff_headers(..., HEADER_ALLOW_LIST).context(...)?` matching the existing `Driver::Http1` arm at lines 1462-1546. Factoring deferred — the existing four arms (`Http1` / `Http1ProbeList` / `Http2` / new `Http1WithAccessLog`) share the same inline shape and a future refactor pass can collapse them in one motion without disrupting any single arm.
2. **`Http1Method::try_from(method.as_str())` → inline match.** The PLAN's verbatim code used `Http1Method::try_from(method.as_str())` but `Http1Method` does not implement `TryFrom<&str>` today. Replaced with the same inline `match method.as_str() { "GET" => Http1Method::Get, other => bail!(...) }` shape used by `drive_admin_scrape` (lib.rs line 1039). Functionally identical for the GET-only surface; adding `TryFrom` is out of Task 9 scope.
3. **`upstream_addr` / `subject_addr` vs. PLAN's `envoy_addr` / `envoy_rust_addr`.** The PLAN's verbatim code referenced `envoy_addr` / `envoy_rust_addr` but the existing `run_fixture` locals are `upstream_addr` / `subject_addr` (used uniformly across all 7 existing dispatch arms). Renamed in the new arm to match the harness's standing naming convention.
4. **New `HeaderRule` enum** (PLAN-implicit). The PLAN's variant spec lists `expected_headers: HeaderRule` and the Task-10 fixture YAML uses `expected_headers: { rule: set_equal_modulo_allow_list }`. The existing `Http1HeaderRule` is externally-tagged-unit (`set_equal_modulo_allow_list` as a bare scalar), so a new `HeaderRule` enum with `#[serde(tag = "rule", rename_all = "snake_case", deny_unknown_fields)]` was added to match the PLAN's intended fixture shape. The single variant `SetEqualModuloAllowList` mirrors `Http1HeaderRule`'s surface.
5. **fmt drift** (per R-9 disclosure requirement): the verbatim PLAN source for both `access_log.rs` (the `SAMPLE_LINE` constant) and `lib.rs` (the `envoy_rust_lines` chain wrap in the dispatch arm) failed the initial `cargo fmt --all -- --check`; `cargo fmt --all` applied to fix; re-verified clean per R-9. Functionally identical.

**LoC:** ~360 LoC in `tests/differential/src/access_log.rs` (rule enum + tokenizer + apply_rule + ISO-8601 validator + assert helper + 4 tests; matches the PLAN's estimate); ~125 LoC added to `tests/differential/src/lib.rs` (`AccessLogPaths` struct + `HeaderRule` enum + `Http1WithAccessLog` variant + `port_key` arm + `run_fixture` dispatch arm + `pub mod access_log;` declaration).

## Task 10 — Fixture 0012 (5 files) + Docker-gated wrapper + BEHAVIOR_CONTRACT.md `Access log field mapping` first-time population

Lands per 06.2 SPEC §3 D4.2.c + D5.2 (folded per signpost recommendation). Five fixture files at `tests/fixtures/0012-access-log-file-sink/` — `envoy.yaml` (Envoy side, bind `0.0.0.0`, admin port 0, `generate_request_id: false`, file access-log at `/tmp/0012-envoy-access.log`); `envoy-rust.yaml` (envoy-rust side, bind `127.0.0.1`, no admin, file access-log at `/tmp/0012-envoy-rust-access.log`); `inputs/payload.bin` (0-byte); `expectations.yaml` (15-rule per-token cascade: 1× `Iso8601Format` + 12× `Exact` + 1× `Wildcard` (User-Agent) + 1× `DurationMs`); `README.md` (per-side divergences + driver shape + cross-references). Docker-gated wrapper at `tests/differential/tests/access_log_file_sink.rs` (matches the `admin_stats_prometheus.rs` shape: `PathBuf` join + `differential::run_fixture(&dir).await.expect("fixture green")`). `docs/envoy-rust/BEHAVIOR_CONTRACT.md` `Access log field mapping` section populated for the first time in the project's history — 14 default-format-token rows with value-exact / name-required-value-may-differ dispositions per parent-06 SPEC §2.2.

**Docker availability locally:** YES. `docker ps` returns the empty container list cleanly. The fixture ran locally and went green on the second attempt (see deviations below).

**Local Docker-gated run:**

```
running 1 test
test access_log_file_sink ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.72s
```

**Workspace gates:** `cargo build --workspace --all-targets` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `cargo fmt --all -- --check` clean; `cargo test -p differential --lib` → `75 passed; 0 failed; 1 ignored` (no regressions; Task 9 baseline unchanged).

**Execution-time deviations from PLAN's verbatim implementation:**

1. **Container↔host filesystem bridge for the Envoy-side access-log file (PLAN-implicit, SPEC-implicit).** The PLAN's Task-10 spec assumed the harness could read the upstream Envoy's `/tmp/0012-envoy-access.log` path on the host directly, but the upstream Envoy runs inside a Docker container under testcontainers (per the existing `tests/differential/src/upstream.rs` shape), so its `/tmp/0012-envoy-access.log` write surfaces inside the container, not on the host. The first local run failed with `read envoy access-log file at /tmp/0012-envoy-access.log: No such file or directory (os error 2)` (the harness's `std::fs::read_to_string(envoy_path)` could not see the container-internal file). Fixed by extending `upstream::start` with an `access_log_mounts: &[(host_path, container_path)]` parameter that adds a `Mount::bind_mount(host, container)` per pair — for Driver::Http1WithAccessLog the harness pre-creates both host-side log files (truncating any prior content per SPEC §6 signpost 7) and bind-mounts the envoy-side path into the container at the same path. The envoy-rust side runs as a subprocess and writes directly to the host path, so no mount is needed for it. This bridge is mechanical / surface-level and resolves a SPEC gap rather than reshaping any architectural decision; consistent with the SPEC §3 D4.2.c statement that the harness reads `the file path lives at /tmp/<fixture-id>-envoy-access.log`. No ADR — the bind-mount surface is already used for envoy.yaml and TLS PEMs at `upstream::start`. ~25 LoC across `upstream.rs` (new param + mount loop) and `lib.rs` (per-driver mount construction + pre-create files).

2. **`%REQ(USER-AGENT)%` rule: `wildcard` (per PLAN's footnote).** The PLAN's expectations.yaml footnote (SPEC §3 D4.2.c line 751: *"may need to be `wildcard` if `drive_http1` adds a default user-agent"*) called the question. Adopted `wildcard` upfront — `drive_http1` (the 04.1 helper) sets `User-Agent` to a default; the wildcard rule accepts either side independently and does not require value equivalence. The first run succeeded with `wildcard` in place; no per-token tightening necessary on first run.

3. **No `wildcard` for User-Agent in the BEHAVIOR_CONTRACT.md row.** The BEHAVIOR_CONTRACT row for `%REQ(USER-AGENT)%` remains marked **value-exact** (the canonical disposition: when both proxies see the same request bytes, both render the same `User-Agent`). The fixture-specific `wildcard` rule in expectations.yaml captures the emitter-side projection difference of `drive_http1` injecting a default, not a contract loosening — per the task spec's note that the dot-tree contract remains authoritative.

**Files created (5 fixture + 1 wrapper + 2 doc/code):**
- `tests/fixtures/0012-access-log-file-sink/envoy.yaml`
- `tests/fixtures/0012-access-log-file-sink/envoy-rust.yaml`
- `tests/fixtures/0012-access-log-file-sink/inputs/payload.bin` (0 bytes)
- `tests/fixtures/0012-access-log-file-sink/expectations.yaml`
- `tests/fixtures/0012-access-log-file-sink/README.md`
- `tests/differential/tests/access_log_file_sink.rs`

**Files modified:**
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (14-row table replaces the `_(empty; populated starting phase 06)_` placeholder; format-reference + closing paragraph)
- `tests/differential/src/upstream.rs` (+`access_log_mounts` parameter on `start`; bind-mount loop)
- `tests/differential/src/lib.rs` (+upstream access-log mount construction at `run_fixture`; pre-create host log files for `Driver::Http1WithAccessLog`)

**LoC:** ~95 LoC of fixture YAML/Markdown; ~22 LoC of Rust (wrapper test + harness extension); ~32 LoC of BEHAVIOR_CONTRACT.md doc.
