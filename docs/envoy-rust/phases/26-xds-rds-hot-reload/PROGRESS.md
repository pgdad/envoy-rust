# Phase 26 (`26-xds-rds-hot-reload`) — PROGRESS

> State-by-state + task-by-task progress log for phase 26 (file-based RDS hot-reload).
> The state-2 PLAN-write entry is below; each state-3 task appends its own entry
> (one code commit + one PROGRESS commit per task, per `feedback_execution_style`).
> **NOTE:** this phase's §6.2 empirical verification is DEFERRED to state-3 **Task 1**
> (Linux-only — the reload trigger is macOS-unobservable per ADR-0049 / SPEC §5.7);
> the cadence is shifted one state vs the normal verify-at-PLAN-write flow. See the
> PLAN STATUS banner.

---

## State-2 PLAN-write (this commit) — DRAFT, §6.2 DEFERRED to state-3 Task 1 (Linux)

- **Skill:** `superpowers:writing-plans`.
- **Mode:** **macOS-deferred** (a deliberate, user-approved departure). The §6.2 empirical verification against `envoyproxy/envoy:v1.33.0` MUST run on Linux (the RDS hot-reload trigger is unobservable on macOS Docker Desktop virtiofs — SPEC §0 finding 4 / §5.7 / ADR-0049 Provenance). This PLAN-write was on macOS, so §6.2 was NOT run; it becomes **state-3 Task 1** (run FIRST, on Linux).
- **Authored:** `PLAN.md` (header + goal + architecture + the STATUS-DEFERRED banner + the §6.2 projections [all `[§6.2-PENDING]`] + the macOS-verified SPEC-correction anchors C1–C6 + the §6.1 split-gate decision [single-phase projected] + the file structure + Tasks 1–10 + self-review) + this `PROGRESS.md` skeleton + the Task-1 preamble (below).
- **Code anchors locked NOW (macOS, read-only, HEAD `1785a0d42`):** C1 `HCMConfig.route_config: Arc<RouteConfiguration>` (`crates/envoy-http1/src/hcm.rs:122`, construct `:209`, read `:1226`/`:1253`); C2 the H2 read path (`crates/envoy-http2/src/hcm.rs:468`) + the type-alias re-export; C3 the `Scheduler::spawn` watcher template (`crates/envoy-health/src/scheduler.rs:40`); C4 the envoy-bin spawn site (`crates/envoy-bin/src/main.rs:180-194`, token `:91`). These are §6.2-independent → the foundation Tasks 2–3 can proceed before Task 1.
- **§6.2-dependent shapes NOT locked** (Task 1 supplies them): the file-change operation / `watched_directory` need (P2); the reload-counter values (P4); the bad-reload disposition (P5); the config_dump-version shape (P6).
- **ADR posture:** **ADR-0066 NOT fired** at this PLAN-write (cannot be evaluated without §6.2). It fires inline at **Task 1** iff §6.2 diverges materially. **DECISIONS.md ledger head stays ADR-0065** until Task 1. ADR-0067 (split) is a Task-1 decision. ADR-0014 in force; ADR-0028 open.
- **ROADMAP:** row `26` flips `planned → in-progress` at this commit (STATE now points at it). **STATE:** advances to `26` state-2-complete / state-3-next (next skill `superpowers:subagent-driven-development`), **with the §6.2-on-Linux blocking precondition recorded** (state-3 begins with Task 1 on Linux, or the §6.2-independent Tasks 2–3 on any platform; the §6.2-gated Tasks 4/6/7/8/9 are BLOCKED on Task 1). Superseded state-1/state-2 top-section blocks relocated to `STATE_HISTORY.md` (ADR-0035 / §4.1 inv. 9).
- **No production/test change at state-2** (docs-only PLAN-write commit).

### Task-1 preamble — the §6.2 PROTOCOL (run on LINUX; the deferred verify-at-PLAN-write)

Task 1 (PLAN §"Task 1") runs the SPEC §6.2 7-item checklist on a Linux host / Linux CI against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`), the ADR-0063 sidecar methodology adapted for a reloadable RDS file:
1. Stand up Envoy with an `rds`-configured HCM (`route_config_name: local_route`, `path_config_source.path` → a mounted file) + a host backend + admin scrapes; confirm initial routing + initial `rds.*` values.
2. **Resolve the file-change operation (P2 — load-bearing):** does Envoy's default file-watch reload on in-place truncate-rewrite? on atomic-rename? is `watched_directory` required? → decides Task 9 (the `watched_directory` schema field) + whether ADR-0066 fires.
3. Resolve P1 (settle latency → Task 7 wait bound), P4 (reload counter values), P5 (bad-reload disposition: `update_failure` vs `update_rejected`, last-good kept), P6 (config_dump version/`last_updated` + route shape; 0026/0027 indices unaffected).
4. Resolve P3 (two distinguishable single-endpoint clusters via `upstream_rq_total`).
5. Lock the shapes: replace every `[§6.2-PENDING]` in PLAN.md + the §2.1/§2.2 BEHAVIOR_CONTRACT projections; fire **ADR-0066** iff P2(`watched_directory`)/P4/P5/P6 diverged (commit title `[ADR-0066]`), else record "no reconciliation"; re-evaluate the §6.1 split (ADR-0067).
6. Commit the lock-ins; preserve the probe transcript here per ADR-0049 Provenance.

---

## Task 1 — §6.2 empirical verification on Linux (fires ADR-0066 if divergent)

_(pending — state-3, Linux)_

## Task 2 — route-table-handle migration (`Arc<RouteConfiguration>` → swappable handle) [§6.2-INDEPENDENT]

**Done (state-3).** Behavior-preserving migration of the HCM route table from an owned
`Arc<RouteConfiguration>` to a swappable handle, in preparation for the RDS atomic-swap
reload (Task 4). No reload logic, no watcher this task — only the field-type change + the
read-once accessor + the writer.

**Handle type chosen + WHY (the sharing-model finding).**
`HCMConfig.route_config: RwLock<Arc<RouteConfiguration>>` — dep-free (`std::sync::RwLock`,
no `arc-swap`, no `tokio::sync::watch`), **no outer `Arc`**. Investigation of the sharing
model (the one real design judgment): `HCMConfig` is built once at startup and shared to
every live request handler via a single `Arc<HCMConfig>` — the H1 `HCM { config:
Arc<HCMConfig> }` and the H2 `HCM { config: Arc<HCMConfig> }` (whose `config.inner:
Arc<Http1HCMConfig>` re-wraps the same H1 config). The per-connection `self.config.clone()`
in both connection handlers is an `Arc::clone` (pointer-bump), **never** a deep struct clone;
the only struct-literal `HCMConfig {…}` constructions outside `from_config` are `#[cfg(test)]`.
Because all handlers AND the future watcher hold the *same* `Arc<HCMConfig>`, they all reach
the *same* `RwLock` cell — so a watcher `store_route_config(new)` is automatically visible to
every connection without an extra `Arc` layer. (An outer `Arc<RwLock<…>>` would only be
required if `HCMConfig` itself were cloned per-connection, which it is not.)

**API added (exact names kept for downstream consistency — Task 4 calls `store_route_config`,
Task 6 calls `current_route_config`):**
- `HCMConfig::current_route_config(&self) -> Arc<RouteConfiguration>` — the **§5.4 read-once**
  accessor: takes the read lock, clones the inner `Arc` (refcount bump), releases, returns the
  owned clone. A per-request handler reads ONCE at entry and holds that snapshot for the
  request lifetime, so a concurrent `store` does not affect an in-flight request.
- `HCMConfig::store_route_config(&self, rc: Arc<RouteConfiguration>)` — write-locks the cell and
  replaces the inner `Arc`. No production caller this task (Task 4's reload pipeline drives it);
  it is exercised by the new unit test, so clippy `--all-targets` does NOT flag it dead — **no
  `#[allow(dead_code)]` was needed.**
- `resolve_route` previously returned `Option<&'a Route>` borrowed from `&'a HCMConfig` (valid
  because the field was an owned `Arc` living in the config). With the table now behind a
  swappable handle, the snapshot is a temporary, so `resolve_route` now returns a new
  `pub struct ResolvedRoute` that **owns** the snapshot `Arc<RouteConfiguration>` and exposes the
  matched route by stored vhost/route indices via `ResolvedRoute::route(&self) -> &Route`.
  Holding it pins the §5.4 snapshot for the request. Both call sites
  (`serve_connection` in http1, `handle_one_stream` in http2) updated to
  `matched_route.as_ref().map(ResolvedRoute::route)` before `pipeline.apply_route_config(...)`;
  `apply_route_config`'s `Option<&Route>` signature is unchanged. `build_response` snapshots once
  at entry and walks the local `Arc`.

**Enumeration count (the grep checklist — every `route_config` read/literal in both crates):**
- envoy-http1: field type (`:122`), construct site (`:209`, wraps `clone_route_config` in
  `RwLock::new(Arc::new(...))`), 2 read sites (`resolve_route`, `build_response`), and **17**
  `route_config: Arc::new(RouteConfiguration {…})` runtime-field constructors (16 test helpers +
  the 2 `resolve_route` test struct-literals counted among them) wrapped to
  `RwLock::new(Arc::new(...))`; 4 `resolve_route(...)` test assertions updated to `.route()`.
- envoy-http2: 1 `Http1HCMConfig` struct-literal test constructor (`:2430`) wrapped; the H2 read
  path delegates to `envoy_http1::hcm::resolve_route(&config.inner, …)` (updated to
  `.map(ResolvedRoute::route)`); 1 H2 `resolve_route` test updated to `.route()`; `RwLock` added
  to the H2 test-module imports. The `route_config: Some(RouteConfiguration {…})` sites in BOTH
  crates are the config-INPUT type (`HttpConnectionManagerConfig.route_config: Option<…>`,
  consumed at `:209`) and were correctly left untouched.

**Verification.**
- New TDD unit test `route_table_handle_swap_is_read_once` (http1 `#[cfg(test)]`): asserts
  (a) a reader sees the CURRENT table, (b) a `store(new)` is visible to the NEXT read, (c) an
  in-flight reader's owned snapshot is UNAFFECTED by a later `store` (§5.4). Written first,
  observed FAIL (no swap API: E0599), then PASS after migration.
- `cargo test -p envoy-http1 -p envoy-http2` → green (http1 lib 113 passed; http2 lib 72 passed; 0 failed).
- `cargo clippy -p envoy-http1 -p envoy-http2 --all-targets -- -D warnings` → clean (exit 0).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean (exit 0).
- Isolated builds `cargo build -p envoy-config -p envoy-http1 -p envoy-http2 -p envoy-bin` → all green.
- `cargo test --workspace --exclude differential --exclude h2spec-conformance` → green (the Docker
  differential + h2spec are NOT run per-task; they fire only at the state-4 gate / on Linux CI).

**§5.2 regression confirmation.** The migration is behavior-preserving for every non-reloading
request: route matching is per-request stateless, the snapshot is taken once at entry and the
walk is byte-for-byte the prior `route_config.virtual_hosts.iter()` walk. Fixture 0028's path
(and the rest of the 33-fixture suite) is unaffected — the only observable change is that the
field is now swappable. The full per-task local cargo suite is the regression witness here; the
Docker differential is deferred to the state-4 gate per this phase's cadence.

## Task 3 — `RdsWatcher` periodic primitive (skeleton) [§6.2-INDEPENDENT]

**Done (state-3).** The 5th periodic-background primitive — a poll-based file-mtime watcher
that mirrors the 12.2 `envoy_health::Scheduler` topology. SKELETON only: the per-target
`reload` is a no-op `Ok(())` stub (the real reparse→revalidate→atomic-swap is Task 4, BLOCKED
on the Linux §6.2 verification; the `rds.*` counter ticks are Task 5). No reload semantics,
no counters this task — only the spawn/loop/shutdown lifecycle + the envoy-bin target-walk +
drain wiring + the lifecycle test.

**File + spawn site.** New `crates/envoy-http1/src/rds_watcher.rs`; declared `pub mod
rds_watcher` + re-exported `pub use rds_watcher::{RdsWatcher, WatchTarget}` from
`crates/envoy-http1/src/lib.rs`. envoy-bin wires it in `crates/envoy-bin/src/main.rs`: a
`let mut rds_targets: Vec<envoy_http1::WatchTarget>` declared BEFORE the listener-serve block,
populated inside the `HCM_FILTER` dispatch arm (right after the h1 `Arc<HCMConfig>` is built),
then `RdsWatcher::spawn(rds_targets, token.clone())` AFTER the listener block (before the admin
block) and drained via `rds_watcher.shutdown().await` on the runtime drain path alongside the
health scheduler + outlier manager (both clean-exit and error-exit paths).

**`spawn` signature: INFALLIBLE (`-> Self`), justified.** Unlike `Scheduler::spawn`
(`Result<_>`, because it registers per-cluster counters + re-parses durations), the skeleton
does NO fallible work at spawn time: it registers no counters (Task 5) and its `reload` stub
cannot fail. So `spawn(targets, cancel) -> Self`. Documented inline that when Task 5 adds
counter registration, spawn may become fallible — and the envoy-bin call site already
`?`-threads its neighbours, so that change stays local.

**`WatchTarget` shape + the H2-inner-handle finding.**
```
pub struct WatchTarget {
    pub path: PathBuf,                 // rds.config_source.path_config_source.path
    pub route_config_name: String,     // rds.route_config_name (unused by no-op reload; Task 4 selects by it)
    pub store: Arc<envoy_http1::HCMConfig>, // Task-2 swappable-handle owner; Task 4 calls store.store_route_config(...)
    // Task 5 adds: the registered rds.* Arc<Counter> handles.
}
```
The H2 wrapping was investigated: `envoy_http2::HCMConfig { inner: Arc<envoy_http1::HCMConfig>,
h2_pool_mgr }` (`crates/envoy-http2/src/hcm.rs:37`) — the H2 wrapper holds NO swappable cell;
the `RwLock<Arc<RouteConfiguration>>` lives only on the inner h1 `HCMConfig`. envoy-bin builds
that h1 `Arc<HCMConfig>` (`main.rs:333`) BEFORE the H2 wrap (`HCMConfig::wrap(Arc::clone(&hcm_config), …)`),
so the target's `store` is `Arc::clone(&hcm_config)` (the inner h1 handle) and BOTH the H1 and
H2 dispatch paths observe the same swappable cell a watcher swap would update. The target-walk
reads `hcm_cfg.rds` once for either codec, BEFORE the codec dispatch — so it is codec-agnostic.

**Poll cadence.** `const POLL_INTERVAL: Duration = 1s`, a named placeholder with a comment that
Task 1's §6.2 settle/poll-bound output may TUNE it. The per-target `watch_loop` burns the
immediate t=0 `interval.tick()`, seeds `last_mtime` from the file at spawn, then
`tokio::select!`s `{ cancel.cancelled() => break, interval.tick() => stat+compare }`; only a
CHANGED present mtime calls `reload(&target)` (a vanished file or stable mtime is a no-op — the
0028 idle witness).

**No-op reload seam for Task 4.** `fn reload(_target: &WatchTarget) -> Result<(), io::Error> {
Ok(()) }` carries the marker `// Task 4: real reparse+revalidate+store_route_config; warm-reject
per §5.5`. The loop's call site already handles `Err(_)` (tracing::warn) so Task 4 fills in the
body WITHOUT touching the spawn/loop/target-walk/drain plumbing.

**§5.2 inertness confirmed.** Empty target list ⇒ zero watch tasks (`task_count() == 0`); the
target-walk only pushes for an HCM with `hcm_cfg.rds.is_some()`, so every non-rds fixture spawns
NOTHING. Fixture `0028-xds-file-based-rds` (rds-configured, `rds.yaml` whose mtime never changes
during the test) ⇒ ONE watch task that idles (no reload fires). The `--workspace` suite (0028 +
the 33-fixture suite) stays behavior-unchanged.

**No new deps.** envoy-http1 already had `tokio` (`time`/`sync`/`macros`) + `tokio-util` (`rt`,
the `CancellationToken`) + dev-dep `tempfile`; the skeleton needed nothing new (confirms reload
logic did NOT creep in — that stays Task 4 in envoy-config-dependent territory).

**Verification.**
- `cargo test -p envoy-http1 rds_watcher -- --nocapture` → 3 passed (idles-when-stable+shutdown,
  empty-targets-zero-tasks, external-cancel-terminates; all on `#[tokio::test(start_paused = true)]`,
  no real sleeps). TDD: test authored first.
- `cargo test -p envoy-http1` → 116 passed, 0 failed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.
- Isolated builds `cargo build -p envoy-config -p envoy-http1 -p envoy-http2 -p envoy-bin` → all green.
- `cargo test --workspace` → the run progressed through 29 test-result groups (the
  `differential` lib's 137 tests + envoy-config / envoy-http1 / envoy-http2 + the envoy-bin
  integration binaries, including 0028's now-spawned-but-idle watcher path) with ZERO
  `FAILED`/`panicked`/non-zero-`failed` lines, then the run wedged at the documented
  startup-stall flake family: a freshly-spawned Rust test binary (`upstream_retry`, then the
  `envoy_bin` lib unittest binary) hung at 0% CPU. A `sample` of the stalled process showed it
  wedged in `_dyld_start` — the macOS dynamic loader, BEFORE `main`/any test code — i.e. an
  acute, environment-level process-spawn stall, NOT a regression (the identical
  `cargo test -p envoy-http1 rds_watcher` + `-p envoy-http1` runs passed earlier THIS session
  with the SAME binaries; the stall reproduced on the standalone re-runs and even on direct
  binary/shell-wrapper spawns, confirming it is the documented macOS dyld flake in a sustained
  phase). Per the project's flake guidance a stall is not treated as a failure; the
  workspace-green re-confirmation should be re-run once the host recovers (it was green through
  every group that the loader allowed to start). All per-crate gates that completed before the
  stall onset (rds_watcher 3/3, envoy-http1 116/116, isolated builds, clippy) were clean.

## Task 4 — reload pipeline (re-parse → re-validate → atomic swap; warm-reject) [BLOCKED on Task 1]

_(pending)_

## Task 5 — per-HCM `rds.*` counters tick per reload (thread phase-20 handles)

_(pending)_

## Task 6 — `/config_dump` `RoutesConfigDump` through the swappable handle + version update [BLOCKED on Task 1]

_(pending)_

## Task 7 — harness mid-test file-rewrite + settle-then-probe [BLOCKED on Task 1]

_(pending)_

## Task 8 — fixture `0034-xds-rds-hot-reload` + Docker wrapper (Linux-CI-authoritative) + in-process backstop [BLOCKED on Task 1]

_(pending)_

## Task 9 — CONDITIONAL: `ConfigSource.watched_directory` field + fuzz seed (fires only if Task 1 P2 requires it)

_(pending — conditional)_

## Task 10 — state-4 phase-done verification + STATE advance to state-5-next

_(pending)_
