# Phase 06.1 — Implementation Progress

Per-task narrative log for sub-phase 06.1 (`envoy-stats` foundation + `envoy-admin` HCM-backed listener migration + Prometheus exposition + fixture 0011). Mirrors the 05.x PROGRESS.md cadence (one section per task; appended at task commit time; quotes meaningful command output inline).

The companion artifacts:
- **SPEC.md** — `docs/envoy-rust/phases/06.1-stats-and-admin/SPEC.md` (committed at the parent-06 state-1+state-2 combined-recovery commit `1f7661a`; the design contract).
- **PLAN.md** — `docs/envoy-rust/phases/06.1-stats-and-admin/PLAN.md` (committed at this sub-phase's state-2 commit, alongside this PROGRESS.md skeleton; the per-task task list).

## PLAN-write posture (recorded at sub-phase 06.1 state-2 commit, before any task commits)

### LoC drift posture (per 06.1 SPEC §6 signpost 20)

The 06.1 SPEC's §3 D1–D7 deliverable estimates total **~1960 LoC**, a ~50% drift over the parent-06 SPEC §3's projection of ~1300 LoC. The PLAN-time refinement to 14 tasks projects **~2010 LoC** in line with the SPEC's estimate. Per 06.1 SPEC §6 signpost 20:

> Per parent-06 SPEC §5, do not nest-split a sub-phase that was itself produced by a split. The PLAN-write planner accepts the estimate and proceeds; if the actual PLAN-time refinement crosses 25 tasks, the planner invokes `superpowers:systematic-debugging` first to confirm the scope is genuine, not creep.

The 14-task count is comfortably under the §6.1 25-task gate; the LoC overage is genuine (concentrated in D1's multi-module envoy-stats decomposition with thorough torture-test surface, and D2's per-endpoint-per-method admin handler test surface). The named trims listed in STATE.md's prior "Next expected skill" guidance — (i) defer `Driver::AdminScrape`'s optional shape, (ii) defer the in-process backstop test to 06.3, (iii) defer the new fuzz-corpus seed — were considered at PLAN-write time and **not applied**:

- **(i) defer `Driver::AdminScrape`'s optional shape** — would scatter fixture-0011's implementation across sub-phases; saves ~50 LoC at the cost of doctrine clarity.
- **(ii) defer in-process backstop to 06.3** — would lose the local regression-equivalence guard for the admin migration that runs without Docker; saves ~120 LoC at the cost of catch-rate for migration regressions on dev machines without Docker.
- **(iii) defer fuzz-corpus seed** — saves ~30 LoC; the seed is small and the corpus-walk acceptance test absorbs it for free; the seed is the primary parse-acceptance evidence for the new `Admin.access_log_path` field.

Total potential savings: ~200 LoC. Even with all three trims applied, the projection (~1810 LoC) would still exceed the 1500 LoC gate. The trims weaken the gate without sufficient LoC reduction; the doctrinally cleaner posture per signpost 20 is to accept the estimate. **Acceptance posture: do NOT trim; do NOT nest-split.** This PROGRESS entry is the documented record of the planner's decision per the established 05.2 / 05.3 cadence.

### Signpost-9 schema correction (per 06.1 SPEC §6 signpost 9)

Parent-06 SPEC §3 D5.1 phrased `HttpConnectionManagerConfig.stat_prefix` as `Option<String> parse-and-consume`. At HEAD `1f7661a` the field is **already required**: `pub stat_prefix: String` at `crates/envoy-config/src/bootstrap.rs:351`. Confirmed via `grep -n 'stat_prefix' crates/envoy-config/src/bootstrap.rs`.

06.1 D5 lands NO schema change for `stat_prefix`; instead, Task 10 consumes the existing required field at HCM construction time (it threads into the per-HCM `HCMStats` registration namespace via `format!("http.{stat_prefix}.downstream_rq_total")`). The SPEC's projection is corrected here in PROGRESS rather than via SPEC edit per D-3.5 (append-only).

### PLAN-write SPEC corrections (recorded for the executor)

The PLAN.md's preamble section "SPEC corrections recorded at PLAN-write time" lists 4 minor projection inaccuracies in the 06.1 SPEC that the planner verified against HEAD `1f7661a`. Reproduced here for stranger-readability:

1. **`envoy_listener::ConnectionHandler::handle` return type.** SPEC §3 D2 projects `BoxFuture<'_, std::io::Result<()>>`. The actual trait at `crates/envoy-listener/src/lib.rs:29` returns `BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>`. PLAN uses the actual signature for `AdminHandler`'s `ConnectionHandler` impl.

2. **`envoy_listener::Listener::serve` signature.** SPEC §3 D3 step 4 projects `envoy_listener::Listener::serve(lst, admin_handler, shutdown)` — a 3-arg free function. The actual `Listener::serve(self, shutdown)` is method-on-self, where `Listener` was constructed via `Listener::bind(&envoy_config::Listener, Arc<dyn ConnectionHandler>)`. Since admin doesn't have an `envoy_config::Listener` (it's an `envoy_config::Admin`), PLAN does NOT route through `envoy_listener::Listener` for the admin path. Instead, **`envoy-admin` exposes its own `serve(listener: tokio::net::TcpListener, handler: Arc<AdminHandler>, shutdown: impl Future<Output = ()> + Send + 'static) -> Result<(), AdminError>` free function** that mirrors the existing `crates/envoy-bin/src/admin.rs::serve` accept-loop pattern. envoy-bin calls this directly. Future phases may unify the admin and data-plane serve loops if a need surfaces.

3. **`Admin` struct does not derive `PartialEq`.** SPEC §3 D5.a's example schema adds `PartialEq` to the derives. PLAN's parse tests compare via direct field access (`assert_eq!(parsed.admin.unwrap().access_log_path, Some(...))`), which avoids the derive churn entirely; if a follow-up phase needs `Admin: PartialEq`, it lands then.

4. **`HttpConnectionManagerConfig.stat_prefix` is already required (not Option).** Per 06.1 SPEC §6 signpost 9 — confirmed at HEAD `1f7661a` line 351: `pub stat_prefix: String`. No schema change needed; HCM consumes the existing field at construction time per Task 10.

These are minor projection inaccuracies; the SPEC remains in-tree unedited per D-3.5.

### Task ordering note

The 14 PLAN tasks are numbered for documentation. The recommended **execution order** is `1 → 2 → 3 → 4 → 5 → 9 → 6 → 7 → 8 → 10 → 11 → 12 → 13 → 14` because Task 9 lands the `Admin.access_log_path` field that Task 6's `AdminConfig::from_envoy_config` reads. Tasks 1-5 build envoy-stats; Task 9 lands the schema; Tasks 6-8 build envoy-admin atop both; Task 10 wires consumers; Task 11 migrates the admin handler; Tasks 12-13 add the harness extensions and fixture; Task 14 verifies.

## Task 1 — PROGRESS.md preamble + LoC drift + SPEC corrections

(THIS section. Lands at sub-phase 06.1 state-2 commit alongside PLAN.md and the STATE.md / ROADMAP.md advance.)

## Task 2 — envoy-stats crate scaffold

Lands the new `crates/envoy-stats/` workspace member with empty placeholder modules. Cargo deps: `bytes = "1"` + `thiserror = "2"` + `tracing = "0.1"` + `[dev-dependencies] tokio = "1"` (for Task 3's torture tests). No `tokio` runtime dep on the library side per SPEC §3 D1's runtime-agnostic posture.

`#![forbid(unsafe_code)]` on `crates/envoy-stats/src/lib.rs` per D-3.8.

`cargo build --workspace --all-targets` green; harmless `never_constructed` / `never_used` warnings on the placeholder types in `counter.rs` / `gauge.rs` / `registry.rs` / `error.rs` (Tasks 3 / 4 land the real types).

**PLAN-write correction discovered at task time:** PLAN.md Step C declares `pub use counter::Counter;` and `pub use gauge::Gauge;` in `lib.rs`, but PLAN.md Step D's placeholder `counter.rs` and `gauge.rs` were docstring-only (no type defined) — `lib.rs` would fail to compile. Mirrored the PLAN's already-established `registry.rs` / `error.rs` placeholder-type pattern by adding `pub struct Counter;` and `pub struct Gauge;` to the placeholder files. Task 3 replaces both files wholesale; the placeholder stubs cost nothing. PLAN.md remains in-tree unedited per D-3.5 (append-only); this PROGRESS entry is the documented record per the established 05.x cadence.

Workspace members at this commit: existing crates + envoy-stats. envoy-admin lands at Task 6.

## Tasks 3 through 14

Appended at execution time, one section per task commit, mirroring the 05.x per-task cadence.

## Task 3 — envoy-stats Counter + Gauge primitives

`Counter` over `AtomicU64`: inc / add / value. Lock-free `Ordering::Relaxed` per SPEC §6 signpost 3.

`Gauge` over `AtomicI64`: set / inc / dec / value. Permits negative values.

Tests: 4 Counter + 4 Gauge = 8 unit tests including 2 multi-thread torture tests (Counter 8×10_000 inc; Gauge 4 inc + 4 dec × 10_000). All 8 pass under `cargo test -p envoy-stats`. Clippy clean.

Task 2's PLAN-correction placeholder stubs (`pub struct Counter;` / `pub struct Gauge;`) are replaced wholesale by this task as anticipated.

**Plan-time deviation (clippy `dead_code`):** `Counter::new()` / `Gauge::new()` are `pub(crate)` per Task 3 discipline (registry-only construction site, Task 4). With the constructors exercised only inside `#[cfg(test)]` modules, the lib build flags both as dead code and `-D warnings` fails. Added a brief `#[allow(dead_code)]` with an inline rationale comment on each constructor; the allow is intended to remain only until Task 4's registry calls them. PLAN.md remains in-tree unedited per D-3.5 (append-only); this entry is the documented record.

LoC: ~55 counter.rs + ~65 gauge.rs (impl + tests + dead_code rationale comment) ≈ ~120 LoC of primitives.

## Task 4 — envoy-stats StatsError + StatsRegistry

`StatsError`: `ConflictingKind { name, expected, got }` + `InvalidName { name, reason }`. No `DuplicateRegistration` variant — same-kind re-registration is idempotent (returns the existing `Arc`).

`StatsRegistry` over `RwLock<BTreeMap<String, StatHandle>>`. `BTreeMap` for deterministic snapshot order per SPEC §6 signpost 6. `register_counter` / `register_gauge` return `Arc<...>`; `.snapshot() -> Vec<(String, StatHandle)>` produces a lexicographic name list.

Stat-name validation per Prometheus rules `[a-zA-Z_:][a-zA-Z0-9_:.-]*`; dots / dashes accepted (Envoy uses dots as separators; emitter translates at emission time).

Tests: 17 total (8 Counter+Gauge from Task 3 + 1 error.rs format + 6 registry behavior + 2 `is_valid_name` accept/reject). All pass under `cargo test -p envoy-stats`; clippy clean.

Closes Task 3's `#[allow(dead_code)]` deviation: removed from both `Counter::new()` and `Gauge::new()` since the registry now calls them at `register_counter` / `register_gauge`.

## Task 5 — envoy-stats Prometheus text-exposition emitter

`pub fn write_exposition(registry: &StatsRegistry, w: &mut bytes::BytesMut)`. Hand-rolled emitter; Envoy stat-tree dots / dashes translate to Prometheus underscores; leading `envoy_` prefix mirrors upstream's emit-side convention.

`# HELP` lines emit as a generic placeholder per SPEC §6 signpost 15; rich per-metric descriptions defer to 06.3+.

Tests: 6 unit tests (empty / counter / gauge / lex-order / dot-translate / dash-translate). All 25 envoy-stats tests pass; clippy clean.

D1 (envoy-stats) complete at this task. Counter/Gauge primitives (Task 3) + StatsError/StatsRegistry (Task 4) + Prometheus emitter (Task 5) total ~470 LoC impl + ~250 LoC tests = ~720 LoC.

Tokio dev-dep removed from envoy-stats Cargo.toml — no test in the crate uses tokio (Task 3's torture tests use `std::thread::spawn`). Cleanup of Task 2's preemptive dep. `cargo build --workspace --all-targets` green after removal.

Added 2 follow-up tests per Task 4 code reviewer's recommendation: `stat_handle_kind_str_returns_correct_label` and `registry_register_counter_contended_same_name_returns_same_arc` (8 threads racing same name → all return same Arc via `Arc::ptr_eq`). These bumped the suite from 23 to 25.

**Plan-time deviation (clippy `write_with_newline`):** PLAN's emitter sketch used `let _ = write!(w, "...{var}...\n")` at four sites; clippy under `-D warnings` flags this as `clippy::write_with_newline` and suggests `writeln!` (no trailing `\n` in the format string). Adopted clippy's suggestion at all four sites (`# HELP`, `# TYPE`, counter line, gauge line). Output is byte-identical (`writeln!` appends `\n`, matching the LF discipline in the planner's reminder). PLAN.md remains in-tree unedited per D-3.5 (append-only); this entry is the documented record.

## Task 9 — envoy-config schema additions (Admin.access_log_path) + fuzz seed

`Admin.access_log_path: Option<String>` parse-and-ignore field per ADR-0026 (precedent: `Listener.listener_filters` from 05.4). `#[serde(default)]`; absent → `None`; present → stored opaquely; envoy-rust never inspects.

3 new validator tests: `parses_admin_with_access_log_path` / `parses_admin_without_access_log_path` / `rejects_admin_with_unknown_field`. All pass. envoy-config suite count: 165 → 168.

1 new fuzz corpus seed: `crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_with_stats_route.yaml`. Allow-list entry added to `crates/envoy-config/fuzz/.gitignore`. The 04.x / 05.x corpus-walk acceptance test (`fuzz_corpus_seeds_parse_or_reject_cleanly`) is an explicit list (not an auto directory walk); appended the new seed name to the success-list array so the gate covers it.

`HttpConnectionManagerConfig.stat_prefix` is **already required** at HEAD (per signpost 9 correction); D5.b is a schema-no-op. Task 10 consumes the existing field at HCM construction time.

Total LoC delta: ~10 schema (struct field + doc comment) + ~50 unit tests + ~38 fuzz seed + 2 sites in envoy-cluster cluster.rs by-hand `Admin {}` test literals patched with `access_log_path: None` (D-3.6 green-build maintenance) ≈ ~100 LoC. In line with SPEC §3 D5's projection.

**Plan-time deviation (corpus-walk shape):** The PLAN described `fuzz_corpus_seeds_parse_or_reject_cleanly` as auto-walking the corpus directory; in reality (HEAD) it is an explicit `&[...]` list of filenames partitioned into success / reject buckets. Added the new seed name to the success bucket array — same gate effect, but required a one-line edit, not zero. PLAN.md remains in-tree unedited per D-3.5 (append-only); this entry is the documented record.

**Plan-time deviation (downstream call sites):** Two by-hand `Admin {}` literals in `crates/envoy-cluster/src/cluster.rs` test code (`from_bootstrap_rejects_empty_cluster` and `from_bootstrap_rejects_duplicate_cluster_name`) needed `access_log_path: None` to compile under `cargo clippy --workspace --all-targets -- -D warnings`. Patched both per D-3.6. envoy-cluster tests (17) still pass.

Sequenced BEFORE Task 6 in execution order (per PLAN's "Executor sequencing notes": 1 → 2 → 3 → 4 → 5 → 9 → 6 → 7 → 8 → 10 → 11 → 12 → 13 → 14) — Task 6's `AdminConfig::from_envoy_config` reads the new `Admin.access_log_path` field landed here.

## Task 6 — envoy-admin crate scaffold + AdminConfig + AdminError

New workspace member `crates/envoy-admin/`. Cargo deps: envoy-config + envoy-http1 + envoy-listener + envoy-stats path-deps + tokio + bytes + thiserror + tracing. **No envoy-http2 dep** per cross-sub-phase architectural rule 3 (admin is HTTP/1.1 only in 06.1).

`AdminConfig::from_envoy_config(&Admin) -> Result<Self, AdminError>` parses `address` to `SocketAddr` and stores `access_log_path` opaquely as `Option<PathBuf>` (parse-and-ignore per ADR-0026). 3 unit tests pass: round-trip-address, carries-access-log-path, rejects-unparseable-address.

`AdminError::{BadAddress, Io}`.

`AdminEndpoint` and `AdminHandler` are placeholder types pending Tasks 7 / 8. The placeholder `serve` returns `unimplemented!()` — Task 8 lands the real implementation. The `_config` / `_registry` field names + `_listener` / `_handler` / `_shutdown` parameter names use the leading-underscore Rust idiom to suppress `dead_code` and `unused_variables` lints; no `#[allow]` annotations needed. Workspace `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean — clippy did not flag the `unimplemented!()` placeholder.

This task is sequenced AFTER Task 9 in execution order so the `Admin.access_log_path` field exists when `AdminConfig` parses it.

`#![forbid(unsafe_code)]` on lib.rs per D-3.8.

Workspace members at this commit: 16 (Task 2's 15 + envoy-admin). The PLAN's reminder noted "15 (Task 2's 14 + envoy-admin)" — Task 2 actually landed the 15th member (envoy-stats); this commit is the 16th. Adjusted per PLAN's "Adjust the workspace count if it differs" instruction.

## Task 7 — envoy-admin AdminEndpoint + per-endpoint render

`AdminEndpoint::{Ready, Stats, StatsPrometheus}` + `from_path(&str) -> Option<Self>` (exact-match per cross-sub-phase rule 5) + `render(&StatsRegistry) -> envoy_http1::Response`.

`render_ready` returns 200 + body `LIVE\n` + `content-type: text/plain`. `render_stats` walks the registry snapshot emitting `name: value\n`. `render_stats_prometheus` calls `envoy_stats::prometheus::write_exposition`.

`render_404` and `render_405` are crate-private helpers used by Task 8's AdminHandler.

Tests: 10 unit tests across path lookup + render shapes + 404/405. All pass; clippy clean.

Plan-time deviations (per D-3.5):
- PLAN's `envoy_http1::codec::Response` path is wrong: `Response` lives in `envoy_http1::response`, not `envoy_http1::codec`. Used the crate-root re-export `envoy_http1::Response` (canonical path; matches existing call sites in envoy-http2).
- PLAN's `Response.reason: String` is wrong: actual shape at HEAD is `reason: Option<&'static str>` (per `crates/envoy-http1/src/response.rs:15`). All `reason: "OK".to_string()` → `reason: Some("OK")`; `reason: "Not Found".to_string()` → `reason: Some("Not Found")`; etc. Test assertion `resp.reason == "OK"` → `resp.reason == Some("OK")`.
- `write!(buf, "{}: {}\n", ...)` → `writeln!(buf, "{}: {}", ...)` for `clippy::write_with_newline` (mirrors Task 5's identical fix in the prometheus emitter; output byte-identical).
- Test name normalization: `render_ready_returns_200_LIVE` → `render_ready_returns_200_live` (lowercase per Rust naming convention; test body / assertion semantics unchanged).
- `render_404` / `render_405` flagged by `-D dead-code` because they're called only from `#[cfg(test)]` and (eventually) Task 8's not-yet-landed `AdminHandler`. Added `#[allow(dead_code)] // wired up by Task 8's AdminHandler::handle_inner` comment, mirroring the same idiom at `crates/envoy-http1/src/codec.rs:51` (`#[allow(dead_code)] // wired up by Task 9's router-proxy arm`).

## Task 8 — envoy-admin AdminHandler + serve accept loop

`AdminHandler::handle(stream)` impls `envoy_listener::ConnectionHandler::handle` (returns `BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>`). Reads HTTP/1.1 request via `httparse` (~150 LoC inline parser); dispatches via `AdminEndpoint::from_path`; renders via `AdminEndpoint::render`; serializes the response inline (~30 LoC; injects `connection: close` so each request closes the connection — no keep-alive in 06.1).

`pub async fn serve(listener, handler, shutdown)` runs the accept loop with shutdown-gated drain (5s budget; matches `Listener::serve`'s behavior). This is envoy-admin's own accept loop — not routed through `envoy_listener::Listener::serve` per PLAN-write SPEC correction #2.

`crates/envoy-admin/Cargo.toml` gains `httparse = "1"` direct dep (consistent with the pre-existing 04.3 REVIEW M-architectural-claim carve-out posture for `httparse`; no new ADR).

`AdminHandler::new` no longer uses `_`-prefix on `config` / `registry` fields (Task 6's placeholder discipline retired now that the handler reads them).

`#[allow(dead_code)]` annotations on `render_404` / `render_405` (added at Task 7) retired — `handle_inner` calls both.

Tests: 4 in-process tests (ready / stats-prometheus / 404 / 405). All 17 envoy-admin tests pass; clippy clean.

Plan-time deviations (per D-3.5):
- The PLAN's draft `let n = stream.read(&mut scratch[..cap.min(scratch.len())]).await?;` triggers E0502 (immutable borrow of `scratch.len()` while mutable-borrowing `&mut scratch[..]`). Hoisted into `let take = cap.min(scratch.len()); let n = stream.read(&mut scratch[..take]).await?;` — same semantics, two-step borrow.
- `clippy::doc_lazy_continuation` required indenting the wrapped second line of the `MAX_REQUEST_HEAD` doc comment by two spaces. Cosmetic; doc text unchanged.

D2 (envoy-admin foundation) complete at this task.
