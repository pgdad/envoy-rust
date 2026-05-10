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

## Task 10 — Stats wiring (D4) + BEHAVIOR_CONTRACT.md initial 3 rows

Three counters wired across the data plane per SPEC §3 D4:
- `listener.<name>.downstream_cx_total` (D4.a; per-accept; `Listener::bind` signature ripple — gains `registry: Arc<StatsRegistry>`; `serve` accept loop calls `cx_total.inc()` on the success arm).
- `cluster.<name>.upstream_cx_total` (D4.b; per-upstream-connect; `from_bootstrap` signature ripple — gains `registry: Arc<StatsRegistry>`; counter Arc is registered at construct time and exposed via `Cluster::cx_total()` + `ClusterHandle::cx_total()`. The increment site lives at the call site — envoy-tcp::TcpProxy::handle, envoy-http1::serve_connection's proxy arm, envoy-http2::handle_one_stream's two upstream-protocol arms — NOT inside envoy-cluster, because the cluster crate is a configuration / load-balancing data structure and does not own a `TcpStream::connect` call site).
- `http.<stat_prefix>.downstream_rq_total` (D4.c; per-request; `HCMConfig::from_config` signature ripple — gains `registry`; `HCMStats { downstream_rq_total: Arc<Counter> }` registered at construct time on `HCMConfig.stats: Arc<HCMStats>`. Increment fires at the entry of `serve_connection`'s per-request loop (H1) and `handle_one_stream` (H2), per signpost 5 — BEFORE route walk, counts attempts including malformed bodies).

`envoy-bin` constructs the global `Arc<StatsRegistry>` once at the top of `run()` and threads it via `Arc::clone(&registry)` into `from_bootstrap`, both `Listener::bind` call sites (tcp_proxy + HCM dispatch arms), and `HCMConfig::from_config`. envoy-bin gains an `envoy-stats` path-dep.

H2 HCM inherits the wiring via the 05.2-established `HCMConfig` type-alias (the H2 listener-side dispatch reads `config.stats.downstream_rq_total` at `handle_one_stream` entry; the H2 router-proxy arm reads `cluster.cx_total()` after each successful Client::connect on either upstream protocol).

BEHAVIOR_CONTRACT.md `Stat-name mapping` first-time populated with 3 rows per SPEC §2 (1 value-exact + 1 name-required-value-may-differ + 1 value-exact). Header allow-list unchanged in 06.1 per cross-sub-phase rule 8.

Tests: 1 listener (`listener_increments_cx_total_on_accept`) + 1 cluster (`cluster_increments_cx_total_on_connect`) + 1 H1 HCM (`hcm1_increments_downstream_rq_total_on_request`) + 1 H2 HCM (`hcm2_increments_downstream_rq_total_on_request`) = 4 new unit tests. All pass; clippy clean.

New error variants: `ListenerError::StatsRegistration(String)`, `ClusterError::StatsRegistration { cluster, message }`, `Http1Error::StatsRegistration { stat_prefix, message }`. Each wraps the registry error's `Display` rendering rather than re-exporting `envoy_stats::StatsError` in the host crate's error surface.

Plan-time deviations (per D-3.5):
- The PLAN's sketch put the cluster-side `cx_total.inc()` inside `cluster.rs` ("connect-site increment (currently `tokio::net::TcpStream::connect(...).await?`) becomes…"), but `envoy-cluster` does NOT own any `TcpStream::connect` call site — connects live in envoy-tcp::TcpProxy + envoy-http1::Client::connect + envoy-http2::Client::connect, and the call-site context (where `ClusterHandle` is in scope) is in their callers. Implemented as: `Cluster::cx_total(&self) -> &Arc<Counter>` accessor (mirrored on `ClusterHandle`), with the increment performed at each call site (envoy-tcp::TcpProxy::handle, envoy-http1::serve_connection's proxy arm Client::connect Ok-arm, envoy-http2::handle_one_stream's two upstream-protocol Client::connect Ok-arms). The cluster-side unit test exercises the cluster's wiring (registration + accessor) by simulating the call-site `cluster.cx_total().inc()` pattern; cross-crate call-site wiring is exercised by the H1 + H2 HCM tests' router-proxy paths and by fixture 0011 (Task 13).
- `ClusterError::StatsRegistration` carries `{ cluster: String, message: String }` rather than the PLAN's `(String)`-shape — the cluster name is useful diagnostic context when registration fails, and uniform with `EmptyCluster { name }` and `EndpointParse { cluster, … }` shapes already in the enum.
- `Http1Error::StatsRegistration` similarly carries `{ stat_prefix, message }` for the same reason (uniform with `UpstreamConnect { addr, source }` shape).
- The PLAN's listener test used `..Default::default()` for `envoy_config::Listener`, but `Listener` does not derive `Default` (verified in `crates/envoy-config/src/bootstrap.rs:192`). The new test reuses the existing `mk_listener_cfg` YAML helper, which spells out all required fields and uses `name: "test_listener"` — the test asserts on the resulting `listener.test_listener.downstream_cx_total` name.
- The H1 HCM test (`hcm1_increments_downstream_rq_total_on_request`) builds the config via the production constructor `HCMConfig::from_config` rather than the existing struct-literal helper `hcm_config_single_route`, because the latter manufactures `HCMStats` from a throwaway registry per `mk_stats(...)` and the counter would not be observable from the test. The 5 existing struct-literal helpers were updated to add `stats: mk_stats("<stat_prefix>")` so all 33 pre-existing H1 HCM tests continue to compile and pass.
- envoy-tcp gains an `envoy-stats` dev-dep (its updated test helper builds a registry to satisfy the new `from_bootstrap(registry)` signature). Production code in envoy-tcp does not need a direct envoy-stats dep — `cluster.cx_total().inc()` reaches `&Arc<envoy_stats::Counter>` via the envoy-cluster re-export.

Stat-name discipline (Task 5 carryforward): `register_counter` call-sites grepped — all 3 templates are alphanumeric + dot + underscore. No `:` introduced.

Total LoC: ~580 insertions across 16 files.


## Task 11 — Phase-01 admin migration + in-process backstop test

`crates/envoy-bin/src/main.rs` admin block replaced. The new block:
1. Builds `envoy_admin::AdminConfig` from `bootstrap.admin`.
2. Binds `tokio::net::TcpListener` to the parsed address.
3. Logs `envoy-rust listening (admin)` with the bound port (preserves the existing log shape so the backstop test can scrape it; `local_addr()` is used so `port_value: 0` resolves to the actual ephemeral port).
4. Constructs `Arc<envoy_admin::AdminHandler>` over the global `Arc<StatsRegistry>` (constructed at Task 10).
5. Spawns `envoy_admin::serve(lst, handler, shutdown)`.

`crates/envoy-bin/src/admin.rs` deleted; the `mod admin;` declaration removed from `main.rs`. envoy-admin's surface fully covers what was previously in-package.

In-process backstop test at `crates/envoy-bin/tests/admin_ready.rs`: spawns envoy-bin via `CARGO_BIN_EXE_envoy-bin` against an admin-only bootstrap with `port_value: 0`, scrapes the bound port from the tracing log, drives `GET /ready` via `std::net::TcpStream`, asserts 200 OK + body `LIVE\n`. SIGKILL on tear-down (mirrors the 04.x / 05.x integration-test posture; phase-02.2 REVIEW M1 awareness-only carryforward continues unchanged).

Fixture 0002 unchanged at the YAML level. The Docker-gated `tests/differential/tests/admin_ready.rs` continues green (the migration preserves `/ready` byte-equivalence per SPEC §5's dual-track guard).

Tests pass (450 passed; 0 failed; 2 ignored across the workspace); clippy clean.

Plan-time deviation (per D-3.5):
- The PLAN's draft test scraped stderr for the `listening (admin)` line, but `tracing_subscriber::fmt()` (as configured in `install_tracing()`) writes to stdout by default — not stderr. The backstop test therefore captures and scrapes `child.stdout` (and `scrape_admin_port` takes `&mut std::process::ChildStdout`). The log line shape (`envoy-rust listening (admin) addr=127.0.0.1:NNNNN`, with optional ANSI color codes around `addr=`) is unchanged; the literal `127.0.0.1:` substring search remains correct.

D3 (admin migration) complete at this task.

### Task 11 follow-up — restore SPEC §3 D3 "non-negotiable" admin response headers

Code-quality review of the original Task 11 commit (`739d0ab`) discovered that envoy-admin's `serialize_response` was missing 4 headers that the deleted in-package `crates/envoy-bin/src/admin.rs::render_response` emitted:

- `cache-control: no-cache, max-age=0`
- `x-content-type-options: nosniff`
- `server: envoy-rust` (ADR-0011 divergence from upstream)
- `date: <RFC 7231 IMF-fixdate>` (sourced from `envoy_http1::date::format_imf_fixdate`)

SPEC §3 D3 lines 953-959 explicitly require these for "non-negotiable mirroring" of the pre-migration shape. The dual-track guard (fixture 0002 + new backstop test) didn't catch the regression: fixture 0002's `expectations.yaml` only diffs status + body; the backstop only asserted `200 OK\r\n` + `LIVE\n`.

**Root cause:** the regression originated at Tasks 6-8 (envoy-admin should have emitted these headers from the start); Task 11 propagated it into the production binary by substituting envoy-admin's response shape for the pre-migration shape without re-checking each header.

**Fix:** all 4 missing headers added to `serialize_response` (uniformly applied to all admin responses including the 400 Bad Request error path). Two new envoy-admin unit tests (`handler_response_carries_server_header` + `handler_response_carries_admin_headers`) plus 4 new assertions in `crates/envoy-bin/tests/admin_ready.rs` lock down the wire shape against future drift.

19 envoy-admin tests pass (17 prior + 2 new). Workspace clean.

## Task 12 — Differential harness extensions (Driver::AdminScrape + BodyRule::PrometheusExposition)

`Driver::AdminScrape { pre_requests, path, expected_status, expected_content_type, expected_body_rule }` — new variant on `differential::Driver`. Drives a sequence of HCM-side `PreRequest`s (so the registry has counters incremented) followed by an admin scrape; asserts on the admin response.

`PreRequest { method, path, host, port_key }` — new struct. `method` is held as `String` (not `Http1Method`) per the PLAN's grammar projection; the dispatch arm converts to `Http1Method` at drive time (only `GET` supported in 06.1; future widening adds more methods at the converter).

`BodyRule::PrometheusExposition { allowlist_envoy_only, allowlist_envoy_rust_only }` — new variant on `differential::BodyRule`. Both fields default to empty so a fixture can declare the rule without pre-seeding (Task 13 territory per signpost 12). The hand-rolled `parse_prometheus_metric_names(body: &[u8]) -> BTreeSet<String>` extracts the metric-name set; the matcher (`assert_body_rule`) asserts on the symmetric difference modulo the per-fixture allow-lists. Does NOT assert on numeric values (06.3 extends).

`drive_admin_scrape(pre_requests, admin_addr, hcm_addrs: &BTreeMap<String, SocketAddr>, path) -> Result<DriveHttp1Result>` — new async helper. Drives pre-requests via `drive_http1`, sleeps 50ms (signpost 11) for Relaxed-ordering visibility, drives the admin scrape via `drive_http1`, returns the response tuple.

`run_fixture` extended:
- New dispatch arm on `Driver::AdminScrape` — reserves a kernel-ephemeral admin host port, plumbs `{{ADMIN_PORT}}` substitution into both per-side kvs maps (subject side: reserved host port; upstream side: `upstream::ADMIN_CONTAINER_PORT`).
- `port_key` resolution extended so AdminScrape uses `"PORT"` for HCM (mirrors the other HCM-shaped drivers); the admin port is plumbed as a separate `{{ADMIN_PORT}}` substitution + a separate `host_admin_port: Option<u16>` reservation.
- New `check_content_type` + `assert_body_rule` helpers consume the dispatch arm's expected_content_type / expected_body_rule fields. `assert_equivalence` was reshaped to call `assert_body_rule` so `BodyRule::PrometheusExposition` works under `equivalence.response_body` too (forward-compatible).

`upstream::start` signature extended with `expose_admin_port: bool`. When true, the testcontainers image exposes `ADMIN_CONTAINER_PORT` (9901) in addition to `CONTAINER_PORT` (10000); the host-mapped admin port is read post-start and exposed via `UpstreamProxy::host_admin_port() -> Option<u16>`. Pre-existing call sites in 0001-0010 fixtures are unaffected (`expose_admin_port = false`); fixture 0011 (Task 13) drives `true` via the AdminScrape dispatch arm.

Tests: 71 lib tests (was 57; +14 new). 466 workspace tests pass; 2 ignored (Docker-gated). Clippy clean.

### Plan-time deviations (per D-3.5)

1. **`BodyRule` serde representation switched from externally-tagged-with-rename_all to internally-tagged via `#[serde(tag = "kind")]`** to accommodate the new struct-form `PrometheusExposition` variant. serde_yaml's externally-tagged form requires explicit YAML tags (`!Variant`) for struct variants — incompatible with the existing `byte_exact` scalar form. The internally-tagged form parses both `{ kind: byte_exact }` (the unit variant) and `{ kind: prometheus_exposition, allowlist_envoy_only: [...], ... }` (the struct variant) uniformly. **Wire-shape consequence**: existing fixtures 0001-0010 had `response_body: byte_exact`; under the new shape they read `response_body: { kind: byte_exact }`. All 10 fixtures updated mechanically in this commit. The 4 inline-YAML test strings in `tests/differential/src/lib.rs` updated similarly. The PLAN's draft (lines 3388-3404) was silent on serde encoding; the executor selected the encoding that minimized blast radius while preserving struct-variant capacity.

2. **`drive_admin_scrape` takes `&BTreeMap<String, SocketAddr>` for the HCM port lookup** rather than the PLAN's draft `&dyn Fn(&str) -> Option<SocketAddr>`. The map is simpler at the call site and matches the existing template-marker discipline (`port_key` strings keying into a per-side address map). Per the PLAN's "Sanctioned trade-offs" note, this adaptation is documented rather than improvised structurally.

3. **`drive_admin_scrape` constructs `(Http1Method, &str, &str, &str, &[])` calls directly** rather than the PLAN's draft `Http1Probe { ..Default::default() }` form. `Http1Probe` carries a required `name: String` (and other per-probe fields) that have no analog in `PreRequest`; `drive_http1` is the lower-level helper that takes the four method/path/host/headers args directly, so the helper calls it unwrapped. Documented as a fit-to-actual-shape adaptation rather than a new wrapper layer.

4. **In-process round-trip test uses `subject::locate_envoy_bin()` rather than `env!("CARGO_BIN_EXE_envoy-bin")`**. Same reason as the prior `drive_http2_round_trip_against_in_process_listener` (line 2320 `lib.rs`): `CARGO_BIN_EXE_envoy-bin` is only set for integration tests *of the package owning the binary*, not for the differential crate. Mirrors the existing convention.

5. **Container-side admin port plumbing** (`upstream::ADMIN_CONTAINER_PORT = 9901`, `expose_admin_port` parameter on `upstream::start`, `host_admin_port: Option<u16>` field on `UpstreamProxy`) was beyond the PLAN's harness-only scope but mechanically necessary for the dispatch arm to drive the upstream side end-to-end. The pre-existing `_backend` / `_tls_backend` cadence already establishes the "fixture template references {{X}} → harness opportunistically wires up X" pattern; this extension fits inside it without an ADR.

D6.harness complete at this task. Task 13 lands fixture 0011 + the Docker-gated wrapper test that exercises the new dispatch arm end-to-end.

## Task 13 — Fixture 0011-admin-stats-prometheus + Docker-gated wrapper

5 fixture files under `tests/fixtures/0011-admin-stats-prometheus/` (envoy.yaml, envoy-rust.yaml, inputs/payload.bin (0 bytes), expectations.yaml, README.md) + 1 Docker-gated wrapper at `tests/differential/tests/admin_stats_prometheus.rs`.

The fixture exercises:
- 1 HCM listener on `{{PORT}}` serving `direct_response 200 "ok\n"` (HTTP/1.1).
- 1 admin listener on `{{ADMIN_PORT}}` serving `/ready` `/stats` `/stats/prometheus`.
- Harness drives `GET /` against the HCM listener (counters increment) then `GET /stats/prometheus` against the admin listener; the matcher asserts metric-name-set equality between envoy ↔ envoy-rust modulo per-fixture allow-lists (does NOT compare numeric values; 06.3 extends).

### Empirical allow-list seeding (SPEC §6 signpost 12)

First run with empty allow-lists surfaced **204 envoy-only** metric names + **2 envoy-rust-only** names. Final allow-list state:

- `allowlist_envoy_only`: **202 entries**. Categories (counts in parens):
  - `server.*` (29) — server-state stats not yet ported.
  - `http.downstream.*` (60) — HCM stats beyond `downstream_rq_total`.
  - `listener.*` + `listener.admin.*` (46) — auto-emitted listener stats.
  - `listener_manager.*` (12), `cluster_manager.*` (9) — manager book-keeping.
  - `runtime.*` (9) — RTDS layer (defers to xDS family).
  - `filesystem.*` (6) — file I/O stats.
  - `http.tracing.*` (5), `http.passthrough.*` (5), `http.rq.*` (5), `http1.*` (4), `http.no_*`/`rs.*` (3) — HCM-adjacent counters.
  - `overload.*` (3), `main_thread.*` (2), `workers.*` (2), `thread_local.*` (2), `tcmalloc.*` (1) — runtime-overload bookkeeping.
- `allowlist_envoy_rust_only`: **2 entries**. `envoy_http_ingress_http_downstream_rq_total` + `envoy_listener_ingress_http_downstream_cx_total`. **Investigated** per the discipline reminder: these are not typos — they're the dynamic-segment-embedded form of two counters that upstream Envoy emits as bare names with Prometheus labels (`envoy_http_downstream_rq_total{envoy_http_conn_manager_prefix="ingress_http"}` etc.). Both proxies emit the same counters; the Prometheus *shape* differs. Documented in BEHAVIOR_CONTRACT.md "Stat-name mapping" §06.1 ("Prometheus exposition shape divergence"); resolution defers to a later phase that adds a `StatsTagExtractor`-equivalent.

### Carryforward from Task 12 (centralize body-rule dispatch)

The 3 HTTP arms in `tests/differential/src/lib.rs` (Driver::Http1, Driver::Http1ProbeList, Driver::Http2) now route through `assert_body_rule` instead of inline `matches!(BodyRule::ByteExact)` — closes the latent inconsistency from Task 12's code review. Behaviorally equivalent for ByteExact; admits future BodyRule variants without re-touching the arms. Http1ProbeList wraps the per-probe failure context via `with_context(|| format!("probe {}", probe.name))?`.

### Plan-time deviations (per D-3.5)

1. **Upstream Envoy `/stats/prometheus` content-type is `text/plain; charset=UTF-8`** (NOT the Prometheus-spec `text/plain; version=0.0.4; charset=utf-8` that envoy-rust originally emitted). Empirically verified at task-execution time. Per D-3.3 (envoy-rust mirrors upstream Envoy verbatim), the product side was updated: `crates/envoy-admin/src/endpoint.rs` now emits `text/plain; charset=UTF-8` for `/stats/prometheus`. The associated unit test (`render_response_carries_correct_content_type`) was updated to lock the new value.

2. **`drive_http1` lacked `transfer-encoding: chunked` handling.** Surfaced at first-run: upstream Envoy's `/stats/prometheus` ships chunked, so the harness was decoding the body as 0 bytes (default `content_length = 0` when Content-Length absent). Fixed by mirroring `drive_http_get`'s chunked handling: the framing precedence is now `chunked` (drain + `decode_chunked`) → `content-length` (read N) → connection-close (read-to-EOF). This is a Task 12 latent harness bug surfaced (and fixed) at fixture-time per D-3.3 — the latent path was unused before fixture 0011 since no prior fixture asserted on a chunked admin body.

3. **Prometheus name-vs-label shape divergence** documented in BEHAVIOR_CONTRACT.md §06.1 ("Prometheus exposition shape divergence"). Two paired entries on each allow-list bridge the divergence; the dot-tree contract `http.<stat_prefix>.downstream_rq_total = value-exact` remains authoritative.

### Acceptance signal

Docker-gated test green locally (3 stable runs):
```
test admin_stats_prometheus ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo test --workspace` → 467 passed; 0 failed; 2 ignored. `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.

D6 (harness + fixture) complete at this task.

