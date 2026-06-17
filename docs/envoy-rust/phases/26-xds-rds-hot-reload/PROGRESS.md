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

## Task 1 — §6.2 empirical verification on Linux (ADR-0066 FIRED)

**Done (state-3, Linux, 2026-06-16).** Ran the SPEC §6.2 7-item checklist on a Linux host
against `envoyproxy/envoy:v1.33.0` (digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) via a read-only
probe rig (the ADR-0063 sidecar methodology). **ADR-0066 FIRES** — P2, P5(iii), and P6 diverged
materially from the SPEC projections. No production/test code changed this task (docs +
DECISIONS only): PLAN.md `[§6.2-PENDING]` projections replaced with verified facts, the
BEHAVIOR_CONTRACT §2.1 RDS-reload-semantics rows + §2.2 hot-reload subsection authored, and
ADR-0066 landed.

**Probe rig + the load-bearing environment caveat (REINFORCES §5.7 / ADR-0049).** The Linux host
runs **Docker Desktop (a VM)**: (i) `--network host` binds the VM's net not the real host →
used bridge networking + published ports; (ii) **host bind-mounts do NOT propagate inotify into
the container** (virtiofs — the SAME limitation as macOS) → Envoy saw new file *content* but
never got a watch event, so NO reload fired for host-side edits. The probe got real reloads only
by putting the RDS file in a **Docker volume** and editing it **inside the container via
`docker exec`** (writes on the container's real ext4, where inotify works). Rig: admin `9901`,
H1 HCM listener `10000` (`stat_prefix: ingress_http`, `rds: { route_config_name: local_route,
config_source: { path_config_source: { path: /data/rds.yaml }}}`, router-only filter chain), two
STATIC clusters `backend_a`/`backend_b` → one backend container (`STRICT_DNS backend:18080`),
distinguished by `cluster.<name>.upstream_rq_total`. **CONSEQUENCE for Task 8: fixture 0034's
differential reload is Linux-CI-authoritative on a NATIVE-Linux runner** (real bind-mount
inotify), NOT locally observable under Docker Desktop; local verification = the in-process
backstop.

**Findings (P1–P7) — verbatim evidence:**

- **P1 (reload + readiness) — MATCHES.** Atomic-rename backend_a→backend_b under 60 concurrent
  probes: `RELOAD settled (update_success 1->2) after .048110191s`; routing after =
  `backend_a:3 / backend_b:61`; listener codes during reload = `404 ×60, ZERO 000` (no drop).
  Settle ~50 ms (6–60 ms range).
- **P2 (file-change operation) — DIVERGES (load-bearing).** Default config (no `watched_directory`),
  edits INSIDE the container: **in-place truncate-rewrite** (`cat >`, `python open('w')`,
  truncate-append) → **3/3 NO reload** (counters frozen at 1). **atomic-rename** (`cp tmp; mv -f
  tmp rds.yaml`) → **3/3 RELOAD** (~6–60 ms, counters +1, routing flips). `watched_directory`
  re-test (it lives under `path_config_source`, NOT `config_source` — the latter fails boot
  `no such field`): in-place STILL 0/3, atomic still works. **⇒ atomic-rename ONLY;
  `watched_directory` NOT required and does NOT rescue in-place.** Inverts the projection.
- **P3 (distinguishable backends) — MATCHES.** After reload#2 (→backend_a): `backend_a 3→5`,
  `backend_b` held at 61. Per-cluster `upstream_rq_total` discriminates cleanly; one endpoint
  per cluster suffices.
- **P4 (counter values) — MATCHES.** `update_attempt/update_success/update_failure/update_rejected/config_reload`
  = `1/1/0/0/1` (boot) → `2/2/0/0/2` (reload#1) → `3/3/0/0/3` (reload#2). Each successful reload
  ticks attempt+success+config_reload by exactly 1. (Other Envoy-only `rds.local_route.*`:
  `version` [64-bit hash], `version_text: ""`, `update_time`, `config_reload_time_ms`,
  `update_duration`, `update_empty: 0`, `init_fetch_timeout: 0` — unasserted.)
- **P5 (bad-reload disposition) — PARTIALLY DIVERGES.** While serving backend_b, atomic-rename to:
  (i) malformed YAML → `update_failure +1`, last-good KEPT (backend_b, no drop) — MATCHES;
  (ii) wrong `route_config_name` → `update_rejected +1`, last-good KEPT — MATCHES;
  (iii) route → unknown cluster → **`update_success +1` + `config_reload +1` (Envoy ACCEPTS +
  APPLIES the broken table)**, then `/probe` → **503** with `http.ingress_http.no_cluster`
  incrementing, last-good NOT kept (re-confirmed 2×) — **DIVERGES**. Envoy does NOT cross-check
  cluster existence on a filesystem-RDS update. **envoy-rust DECISION (ADR-0066): re-validate +
  warm-REJECT (`update_rejected` + keep last-good)** — because the request path
  `.expect()`s cluster existence at `hcm.rs:818` (installing the route would panic, worse than
  503); recorded divergence, unobservable in fixture 0034 (backstop-only).
- **P6 (config_dump) — MINOR DIVERGENCE (already-correct in envoy-rust).** `dynamic_route_configs[]`
  keys = `[@type, route_config, last_updated]`. **`version_info` ABSENT** (never populated for
  file-RDS — already the phase-20 envoy-rust shape). `last_updated` changes (`2026-06-16T08:37:53.875Z`
  → `…08:42:47.532Z`); `route_config.virtual_hosts[].routes[].route.cluster` reflects
  `backend_a`→`backend_b`. 0026/0027 indices unaffected.
- **P7 (in-flight isolation) — MATCHES (resolved).** Routed `/probe`→`backend_slow` (5 s delay),
  started request, atomic-reloaded →backend_a mid-flight: `SLOW-RESP|HTTP=200|t=5.002849`;
  `backend_a:0 / backend_slow:1`. The in-flight request completed under the OLD table.

**ADR posture.** **ADR-0066 FIRED** (commit title carries `[ADR-0066]`). DECISIONS ledger head
→ **ADR-0066** (count 67). Decisions: (1) Task 9 N/A — no `watched_directory`, no schema change;
(2) Task-7 harness uses atomic-rename; (3) the warm-reject taxonomy {IO/parse→failure, name-absent
→rejected, unknown-cluster→rejected [recorded divergence]}; (4) Task 6 narrows to read-through-handle
(`version_info` already absent); (5) §6.1 single phase CONFIRMED — **ADR-0067 stays UNFIRED**.
Containers/volume/network all cleaned up; no repo files touched by the probe (all scratch in
`/tmp` + Docker objects).

**Task 9 N/A — atomic-rename triggers the reload; `watched_directory` not required** (P2 / ADR-0066).

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

## Task 4 — reload pipeline (re-parse → re-validate → atomic swap; warm-reject) [§6.2-LOCKED] — Task 5 FOLDED IN

**Done (state-3, Linux).** Implemented the real RDS reload pipeline replacing the Task-3 no-op
stub, TDD'd (tests first → FAIL → implement → PASS), clippy-clean, two-stage reviewed
(spec-compliance ✅ + code-quality ✅ after one fix round). **Commits:** `355e2b3`
`feat(http): implement RDS reload pipeline …` (code) + `8439261`
`fix(http): review fixes for RDS reload pipeline …` (the code-quality-review fixes). **Task 5
(thread the phase-20 `rds.*` stat handles into the watcher target) was FOLDED IN here** — the
reload cannot tick counters without the handles, so doing both together is the only coherent
intermediate state.

**What landed.**
- **`envoy-config` (`rds.rs`):** new `pub fn reparse_and_select_route_config(path, route_config_name,
  known_cluster: &dyn Fn(&str) -> bool) -> Result<RouteConfiguration, ConfigError>` — read (IO err →
  `RdsFileError`) → `parse_rds_file` (parse err → `RdsParseError`) → select by name (absent →
  `RdsRouteConfigNotFound`) → re-validate route→cluster refs via the predicate (unknown → `UnknownCluster`).
  **Design (cycle-avoidance, accepted at review):** the cluster check is a `&dyn Fn(&str)->bool`
  predicate, NOT `&ClusterManager` — `envoy-cluster` depends on `envoy-config`, so passing the manager
  would form a dependency cycle; the watcher passes `|n| cluster_mgr.get(n).is_some()`. 6 unit tests
  (happy / IO / parse / name-not-found / unknown-cluster / direct-response-needs-no-cluster).
- **`envoy-http1` (`rds_watcher.rs`):** the real `reload` — reparse/revalidate **OUTSIDE the write lock**
  (carry-forward (a)), then on `Ok` a single `store.store_route_config(Arc::new(rc))` + tick
  `update_attempt`+`update_success`+`config_reload`; on `Err` KEEP the table + tick `update_attempt` +
  the failure/rejected counter per the §6.2-LOCKED taxonomy {IO/parse → `update_failure`;
  name-absent / unknown-cluster → `update_rejected`}. New `RdsCounters` struct (5 `Arc<Counter>`) on
  `WatchTarget`. The error classifier uses explicit arms + an `unreachable!()` final arm (no silent
  wildcard — review fix M2). 4 reload tests assert the swap (`2/2/0/0/2`) + `Arc::ptr_eq` byte-unchanged
  last-good after each bad-reload class.
- **`envoy-bin` (`main.rs`) + `envoy-listener` (`lib.rs`):** the target-walk re-resolves the 5 `rds.*`
  counter handles from the `StatsRegistry` by name (idempotent register-by-name ⇒ the SAME handles
  `register_rds_stats` created at initial load, continuing the `1/1/0/0/1` series → scrape-visible).
  A shared `pub fn envoy_listener::rds_counter_base(stat_prefix, route_config_name) -> String` is the
  single source of truth for the base name, routed through all three call sites (`register_rds_stats`
  + the target-walk + the tests) so the names cannot drift (review fix M1).
- **Carry-forwards folded in:** **(a)** the `store_route_config` poison-recovery-safety comment +
  reparse-outside-the-lock discipline; **(b)** the stale `resolve_route` "we yield BOTH …" comment
  tidied to reflect the single owned `ResolvedRoute`.

**THE RECORDED DIVERGENCE (ADR-0066 P5(iii)).** An unknown-cluster reload is **warm-rejected**
(`update_rejected` + keep last-good), NOT applied — deliberately diverging from real Envoy
(which accepts + serves 503/`no_cluster`) because the H1/H2 request path resolves the route's
cluster via `cluster_mgr.get(name).expect(…)` (`hcm.rs:818`); installing an unknown-cluster route
would PANIC the proxy. Test `reload_unknown_cluster_keeps_last_good_and_ticks_rejected`.

**Verification.** `cargo test -p envoy-config` 431 / `-p envoy-http1` 120 (incl. all 10 new tests);
`cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; isolated builds
`-p envoy-config -p envoy-http1 -p envoy-http2 -p envoy-bin` all green; no `unsafe`. (The Docker
differential + the `0034` reload are the state-4 / native-Linux-CI gate, not run per-task.)

**CARRY-FORWARD recorded at this task (the code-quality review's I1, controller-resolved as a
deliberate deferral):** the reload re-validates ONLY the cluster-EXISTENCE reference. Phase-20's
initial-LOAD validator's `RouteAction::Route` arm enforces a SECOND check — `Http2ClusterFromHttp1Listener`
(an H1/AUTO listener routing to an H2-only upstream cluster, ADR-0028). The reload does NOT re-validate
that gate, so a hot-reload can install an H1→H2-only route that the initial bootstrap load would have
rejected as fatal. **Resolution (controller decision):** this is a deliberate deferral consistent with
the project-wide OPEN ADR-0028 deferral (H1×H2 dispatch is unimplemented project-wide). The principled
line: the reload REJECTS what would PANIC the request path (the unknown-cluster `.expect()`), but DEFERS
(per ADR-0028) what would merely misnegotiate silently (the H1→H2-only case — the pre-06.3 behavior, no
panic, no stability threat). Threading the listener codec into the watch target for a full re-validation
is deferred with ADR-0028. The `reparse_and_select_route_config` doc comment states this honestly (no
overclaim). Revisit if/when ADR-0028 (H1×H2 dispatch) is engaged.

## Task 5 — per-HCM `rds.*` counters tick per reload (thread phase-20 handles) — FOLDED INTO Task 4

**Done (folded into Task 4 above).** The watcher target carries the SAME `rds.*` `Arc<Counter>`
handles the HCM registered at construction (`register_rds_stats`), re-resolved from the
`StatsRegistry` by name in the envoy-bin target-walk (idempotent register-by-name), so a reload
increments the scrape-visible registry counters (continuing the initial-load `1/1/0/0/1` series),
NOT a private copy. The base name is the shared `envoy_listener::rds_counter_base` (no drift). No
separate commit — folded into `355e2b3` + `8439261` because the reload pipeline (Task 4) cannot tick
counters without the handles.

## Task 6 — `/config_dump` `RoutesConfigDump` through the swappable handle [§6.2-LOCKED P6] — NARROWED to read-through-handle

**Done (state-3, Linux).** `GET /config_dump`'s `RoutesConfigDump` now renders the **live,
hot-reloaded** route table — read through the runtime swappable handle
(`HCMConfig::current_route_config()`) — instead of the frozen startup bootstrap snapshot, so a
`/config_dump` AFTER an RDS reload reflects the NEW table. TDD'd (test first → FAIL → implement →
PASS), clippy-clean, two-stage reviewed (spec ✅ + quality ✅ Approve, two Minor hardening fixes
folded in). **Commits:** `13d035c` `feat(admin): render /config_dump RoutesConfigDump through live
swappable route handle [phase 26 Task 6]` (code) + `4b647fc` `fix(admin): warn-on-miss + uniqueness
comment …` (the code-quality review fixes).

**§6.2-LOCKED narrowing (P6 / ADR-0066).** Task 6 shrank to ONE thing — read-through-handle —
because §6.2 confirmed: NO `version_info` for file-RDS (already absent in the phase-20
`RoutesConfigDump` shape, added NOTHING), and `last_updated` is already render-time `now()` (already
changes per dump). So NO version field and NO timestamp plumbing this task.

**What landed (3 files; renderer is a pure reader — request/data path untouched).**
- **`envoy-admin` (`handler.rs`):** new `AdminHandler.live_route_configs: Vec<(String, Arc<envoy_http1::HCMConfig>)>`
  (the live swappable route-table sources, keyed by `rds.route_config_name`) + the `pub(crate)
  live_route_configs()` accessor. `AdminHandler::new` widened 7-arg → **8-arg by APPENDING** the
  trailing param (the documented additive-widening lineage — no reorder; every prior phase relied on
  it). The per-connection `ConnectionHandler` struct-literal clone populates the new field
  (`live_route_configs: self.live_route_configs.clone()`) so cloned handlers also reflect reloads.
  One `#[allow(clippy::too_many_arguments)]` (8 args; a params struct would obscure the widening
  lineage — reviewer concurred this is the right call, not a struct).
- **`envoy-admin` (`endpoint.rs`):** `render_config_dump` materializes an OWNED `Vec<RouteSnapshot>`
  (a local enum `Live(Arc<RouteConfiguration>)` / `Bootstrap(&RouteConfiguration)`) that **outlives
  serialization**, then builds the borrowing `DynamicRouteConfigEntry` list from it. Per rds HCM: look
  up the live handle by `rds.route_config_name` → render `current_route_config()` (read-once Arc snapshot);
  **fallback** to the bootstrap `route_config` borrow when no handle exists. **Key finding: `envoy_config::RouteConfiguration`
  is NOT `Clone`** (only `Debug/Serialize/Deserialize/PartialEq` + the private `clone_route_config`
  hand-clone), so the fallback BORROWS from `bootstrap` (which outlives the fn) rather than deep-cloning
  into an `Arc` — the `RouteSnapshot` enum is what reconciles the owned-live vs borrowed-fallback arms
  under one `&RouteConfiguration` serialize surface. Emission stays rds-conditional; ordering, `last_updated`,
  and the absent `version_info` unchanged; 0026/0027 untouched.
- **`envoy-bin` (`main.rs`):** build `live_route_configs` from `rds_targets` (clone `route_config_name`
  + `Arc::clone(&store)`) **BEFORE** `RdsWatcher::spawn(rds_targets, …)` consumes them — the SAME
  `Arc<HCMConfig>` swap-owners the watcher holds, so the admin reader and the watcher writer share the
  one `RwLock<Arc<RouteConfiguration>>` cell. Passed as the new trailing `AdminHandler::new` arg.

**Code-quality review fixes folded in (Minor #1 + #2, `4b647fc`).** (1) **Observability:** a live-handle
lookup miss against a NON-empty handle set means the wiring drifted (e.g. a `route_config_name` mismatch
between envoy-bin and the renderer) and the dump would SILENTLY fall back to the stale startup table —
the exact failure this task removes. Now emits a `tracing::warn!` on that path (an EMPTY handle set stays
the legitimate tests / non-rds path → no warn). (2) Documented the first-wins-by-`route_config_name`
uniqueness assumption on the linear `find`. (Minor #3 — the `#[allow(too_many_arguments)]` — needed no
action; reviewer agreed it is correct.)

**Verification.** New TDD test `routes_config_dump_tests::config_dump_reflects_hot_reloaded_route_table`
(`#[tokio::test]`): builds a real `HCMConfig::from_config` with an initial table (marker vhost
`vh_initial`), wires it via `live_route_configs`, asserts the dump shows `vh_initial`, then
`store_route_config(Arc::new(…vh_reloaded…))` swaps it and asserts the next dump reflects `vh_reloaded`
(JSON pointer `/dynamic_route_configs/0/route_config/virtual_hosts/0/name`) — observed FAIL pre-change
(read the bootstrap copy: `vh_initial` vs `vh_reloaded`), PASS post-change. `cargo test -p envoy-admin`
**96** / `-p envoy-http1` **120** / `-p envoy-bin` **8** (the widened call sites compile+pass); `cargo clippy
--workspace --all-targets --all-features -- -D warnings` clean; isolated builds
`-p envoy-config -p envoy-http1 -p envoy-http2 -p envoy-admin -p envoy-bin` green. No `unsafe`. (The Docker
differential + the `0034` reload are the state-4 / native-Linux-CI gate, not run per-task.)

## Task 7 — harness mid-test file-rewrite (ATOMIC-RENAME) + bounded wait-for-convergence [§6.2-LOCKED P1/P2]

**Done (state-3, Linux).** Added the differential-harness "reload step": a `Driver::Http1RdsReload`
variant that runs pre-reload probes (bilateral), atomic-renames the post-reload RDS YAML over the
watched path on BOTH proxy sides, waits — bounded, on a discriminating observable, NOT a fixed sleep —
for both proxies to converge on the new table, then runs post-reload probes (bilateral). TDD'd on the
locally-testable mechanics (3 harness-unit tests first → FAIL → implement → PASS), clippy-clean,
two-stage reviewed (spec ✅ + quality ✅ Approve, two Minor hardening fixes folded in). **Commits:**
`585e844` `feat(differential): add Http1RdsReload driver — atomic-rename reload step + bounded
wait-for-convergence [phase 26 Task 7]` + `bb41eb8` `fix(differential): surface last drive error on
reload-convergence timeout + document single-reload temp-name assumption [phase 26 Task 7]`.

**§6.2-LOCKED (P2 / ADR-0066) — atomic-rename is mandatory on BOTH sides.** Envoy's default file-watch
ignores in-place truncate-rewrite and reloads ONLY on atomic-rename (verified at Task 1). So:
- **Subject side (envoy-rust):** `atomic_rename_over(target, new_content)` — write a SAME-DIR sibling
  `<target>.reload-tmp` then `std::fs::rename` (same-fs atomic swap, never a cross-device copy). The
  RDS file is the host temp `subject_rds_path` (`{tmpdir}/rds-subject.yaml`).
- **Upstream side (Envoy container):** `UpstreamProxy::reload_rds_atomic(new_content)` — base64-encode
  on the host, then `docker exec sh -c "set -e; printf %s '<b64>' | base64 -d > <path>.reload-tmp;
  mv -f <tmp> <RDS_CONTAINER_PATH>"`. The rewrite MUST happen INSIDE the container (virtiofs bind-mount
  inotify does not propagate — the §5.7 finding), and `mv -f` on a sibling of the watched file is the
  same-fs atomic swap. `set -e` aborts before the `mv` if `base64 -d` fails, so a decode failure leaves
  the live file untouched (no partial/garbage atomic install); errors on non-zero exit code.

**What landed (4 files — `tests/differential/src/lib.rs` +413 / `upstream.rs` +40 / `Cargo.toml` +base64 / `Cargo.lock`).**
- **Schema:** `Driver::Http1RdsReload { pre_probes: Vec<Http1Probe>, reload: RdsReloadStep, post_probes:
  Vec<Http1Probe> }` (matched the enum's `#[serde(tag="kind", rename_all="snake_case", deny_unknown_fields)]`
  discipline) + `RdsReloadStep { reload_file [default `rds-reload.yaml`], settle_budget_ms, discriminator:
  Http1Probe }`. The `Http1Probe` type is reused verbatim for probes + the discriminator.
- **`wait_for_reload_convergence(addr, probe, budget)`:** drives the discriminator probe at a 25 ms poll
  until its `expected_status` (+ optional `expected_body`) match, bounded by `budget` (= `settle_budget_ms`);
  a drive error mid-reload is "not converged yet" (retry), and the LAST drive error is folded into the
  budget-exhaustion `bail!` (review Minor #1 — diagnosability on the CI-only path). This is the 12.2
  wait-for-convergence pattern on a routed-to observable, NOT a fixed sleep.
- **`run_http1_probe_bilateral(...)`:** extracted the per-probe bilateral status/body/header equivalence
  cascade from the existing `Http1ProbeList` arm (which was LEFT UNCHANGED — no behavior change to any
  unrelated arm) so the new arm reuses it for pre+post probes with a `pre`/`post` label.
- **Dispatch arm** in `run_fixture`: pre_probes (bilateral) → read `fixture_dir/<reload_file>` + render
  per-side (`upstream_kvs_refs`/`subject_kvs_refs`, residual-marker guards) → `atomic_rename_over` subject
  + `upstream.reload_rds_atomic` container → `wait_for_reload_convergence` on BOTH `upstream_addr` and
  `subject_addr` → post_probes (bilateral) → teardown. Bails if the fixture is not file-based-RDS
  (`upstream_rds_path.is_none()`). On an error-path `?` the subject (Drop → SIGKILL) and the container
  (testcontainers Drop) are reclaimed by RAII — no leak, same graceful-shutdown-skip as the existing arm.
  Also added the variant to the `port_key` `"PORT"` arm.
- **Dep:** `base64 = "0.22"` added to `tests/differential/Cargo.toml` (already in the workspace lock).

**Verification.** 3 new harness-unit tests (TDD, all LOCAL / no docker): `driver_http1_rds_reload_round_trips_through_serde`
(serde round-trip exercising the `reload_file` default), `atomic_rename_over_swaps_content_and_leaves_no_temp`
(content swapped + exactly one dir entry = no leftover temp), `rds_reload_template_renders_per_side` (per-side
render distinctness). `cargo test -p differential --lib` **140 passed / 2 ignored**; `cargo clippy -p differential
--all-targets --all-features -- -D warnings` clean; `cargo build -p differential --all-targets` green. No `unsafe`.

**The docker-only path is NOT exercised locally — by design.** `reload_rds_atomic` + the dispatch arm's
reload/convergence sequence COMPILE but require docker AND a native-Linux runner (under Docker Desktop
virtiofs the upstream reload is unobservable — §5.7 / ADR-0066). They run on **native-Linux CI** via Task 8's
fixture `0034`; locally the host-side half of every mechanism (atomic-rename, per-side render, schema) IS
unit-tested and the container-side half is a thin, documented mirror. Code-quality review confirmed the
shell-script robustness (base64 alphabet has no shell metacharacters, the path is a constant, no arg-length
risk for KB-scale RDS, `set -e` prevents partial installs) — the review IS the safety net for the CI-only path.

## Task 8 — fixture `0034-xds-rds-hot-reload` (Linux-CI-authoritative differential) + in-process backstop [§6.2-LOCKED]

**Done (state-3, Linux).** The two end-to-end reload proofs: a Docker-gated **differential fixture** (the
native-Linux-CI-authoritative bilateral proof, using the Task-7 `Http1RdsReload` driver) + an **in-process
backstop** (the deterministic LOCAL complement that boots real `envoy-bin` and scrapes stats). Split into
two separately-reviewed units (8a fixture+wrapper, 8b backstop), each two-stage reviewed (spec ✅ + quality
✅ Approve). **Commits:** `7b2ba94` `test(differential): add fixture 0034-xds-rds-hot-reload + Docker-gated
wrapper …` (8a) + `5c6ebc1` `test(envoy-bin): add in-process RDS hot-reload backstop …` (8b).

**DESIGN ADAPTATION (recorded — diverges from the PLAN's literal "two distinguishable clusters in the
fixture").** The Task-7 `Http1RdsReload` driver converges on probe **status/body** (no `expected_stats`/
`admin_scrapes` fields), and the differential harness spawns a SINGLE echo backend — so two clusters cannot
be distinguished in a fixture response. The work therefore splits the §6.2 proofs by harness strength:
- **The differential fixture (0034)** proves the reload BILATERALLY via a `direct_response` body change
  (`rds-v1`→`rds-v2`) on the SAME `/probe` path — a genuine route-table swap, byte-exact and identical on
  both sides with ZERO upstream-header noise (no clusters/backend, so none of fixture-0028's header-stripping
  knobs are needed). This is the clean bilateral "the table reloaded" proof.
- **The in-process backstop** carries the cluster-distinguishability (P3, via `cluster.<name>.upstream_rq_total`),
  the **counter taxonomy** (P4), the **config_dump live-table reflect** (P6), the **warm-reject negative paths**,
  and **in-flight isolation** — everything that needs deterministic stat-scraping, which the Linux-CI-only
  differential can't cleanly drive. The PLAN's "assert P4 counters + P6 config_dump" is satisfied HERE.

**8a — fixture `tests/fixtures/0034-xds-rds-hot-reload/` + `tests/differential/tests/xds_rds_hot_reload.rs`
(commit `7b2ba94`, 7 files).** `rds.yaml` (initial: `local_route`, `/probe`→`direct_response` body `rds-v1\n`)
+ `rds-reload.yaml` (post: same shape, body `rds-v2\n`) + `envoy-rust.yaml` + `envoy.yaml` (both: admin + one
RDS-configured H1 listener + router filter, `clusters: []`, no CDS) + `expectations.yaml`
(`Driver::Http1RdsReload`: pre `rds-v1` → reload [`settle_budget_ms: 5000`, discriminator `/probe`→`rds-v2`]
→ post `rds-v2`) + `README.md` (the three-phase sequence, the direct_response rationale, and the
**NATIVE-LINUX-CI-AUTHORITATIVE** / Docker-Desktop-virtiofs-unobservable caveat) + the Docker-gated wrapper
(mirrors `xds_file_based_rds.rs`). **Locally verified to the max without Docker:** the SUBJECT side was booted
(`envoy-bin -c <rendered envoy-rust.yaml>`) → `GET /probe` → 200 `rds-v1`, atomic-renamed `rds-reload.yaml`
over the watched path → `/probe` flips to `rds-v2` (the reload pipeline works end-to-end); `expectations.yaml`
deserializes via `differential::load_expectations` → `Driver::Http1RdsReload` (default `reload_file`
`rds-reload.yaml`); the wrapper compiles; clippy clean. The full bilateral `run_fixture` runs on native-Linux CI.

**8b — `crates/envoy-bin/tests/xds_rds_hot_reload.rs` (commit `5c6ebc1`, 723 lines, 5 tests ALL PASS).**
Boots real `envoy-bin` as a native subprocess (the poll-based mtime watcher, ~1s cadence, observes a host-side
atomic-rename — the virtiofs limitation is container-only), reloads via `atomic_rename_rds` (same-dir sibling +
`std::fs::rename`), and `wait_for_stat`-gates each assertion on the right convergence counter. Two static
clusters `backend_a`/`backend_b` → one echo backend, distinguished by `upstream_rq_total`:
- `happy_reload_flips_route_and_ticks_counters` — `1/1/0/0/1` → atomic-rename →`backend_b` → wait
  `update_success==2` → `backend_b.upstream_rq_total>=1`, counters `2/2/0/0/2`, and `/config_dump`
  `RoutesConfigDump` LIVE table walks `/probe`→`backend_b` (P6 reflect).
- `malformed_reload_warm_rejects_and_keeps_last_good` — malformed YAML → `update_failure` (`2/1/1/0/1`),
  routing still `backend_a` (`backend_b` total stays 0 ⇒ last-good provably kept).
- `name_absent_reload_warm_rejects_and_keeps_last_good` — `route_config_name` mismatch → `update_rejected`
  (`2/1/0/1/1`), routing still `backend_a`.
- `unknown_cluster_reload_warm_rejects_recorded_divergence` — route→cluster `nope` → `update_rejected`
  (`2/1/0/1/1`), routing still `backend_a`. **THE RECORDED DIVERGENCE** (commented in-test): real Envoy
  ACCEPTS + serves 503/`no_cluster`; envoy-rust warm-REJECTS because the request path `.expect()`s cluster
  existence (`hcm.rs:818`) — installing the route would PANIC. Unobservable in the differential ⇒ proven here.
- `in_flight_request_completes_under_old_table` — a 2 s slow backend via `backend_slow`; a `/slow` request
  started in-flight, reload DROPS `/slow` mid-flight, the in-flight request still completes **200** under the
  snapshotted old table (end-to-end confirmation of the Task-2 §5.4 read-once; stable across 4 suite runs).
Review verified NO spurious passes: each test waits on the convergence counter BEFORE asserting post-reload
state, and the chosen gate is correct because `reload()` ticks `update_attempt` at entry + swaps the table
BEFORE ticking `update_success` (so `success==2` ⇒ the swap landed; `attempt==2` ⇒ a reject fully ran and the
table was never touched). No assertion was loosened; the reload pipeline matched the §6.2-locked taxonomy on
the first run.

**Verification.** `cargo test -p envoy-bin --test xds_rds_hot_reload` **5 passed**; `cargo test -p differential
--lib` 140/2-ignored + the wrapper compiles; `cargo clippy --workspace --all-targets --all-features -- -D
warnings` clean. No `unsafe` (`#![forbid(unsafe_code)]` in the backstop). The fixture differential itself is the
native-Linux-CI gate (Task 10 / CI), not run locally.

## Task 9 — CONDITIONAL: `ConfigSource.watched_directory` field + fuzz seed — ❌ DOES NOT FIRE (N/A)

**N/A — no work, confirmed at Task 1.** The §6.2 Linux verification (Task 1, ADR-0066 P2) proved Envoy reloads
on **atomic-rename with NO `watched_directory`** (and `watched_directory` does not even rescue the in-place
truncate-rewrite case). The Task-7 harness + the Task-8 fixture/backstop all use atomic-rename, which
envoy-rust's mtime poll detects. **NO config-schema change, NO new fuzz seed.** Recorded in ADR-0066 + PLAN
§"Task 9". Skip to Task 10.

## Task 10 — state-4 phase-done verification + STATE advance to state-5-next

**In progress (state-4, Linux, 2026-06-17).** Ran the full §7.5 phase-done gate. **Skill:**
`superpowers:verification-before-completion`. The local deterministic gates are ALL GREEN; the
AUTHORITATIVE differential anchor is the native-Linux CI run (fixture 0034's reload is
Docker-Desktop-virtiofs-unobservable on this host — §5.7 / ADR-0049), which had been RED for the
entire phase and is unblocked by the fmt fix landed at this task.

**THE LOAD-BEARING FINDING — CI was red at `fmt` for the whole phase.** `cargo fmt --all -- --check`
(§7.5(e)) surfaced fmt drift in SIX files committed across Tasks 6/7/8 (`crates/envoy-admin/src/endpoint.rs`,
`crates/envoy-bin/src/main.rs`, `crates/envoy-bin/tests/xds_rds_hot_reload.rs`,
`crates/envoy-config/src/rds.rs`, `crates/envoy-http1/src/hcm.rs`, `crates/envoy-http1/src/rds_watcher.rs`).
The per-task discipline ran clippy per task but DEFERRED `fmt --check` to this state-4 gate — so the
drift was invisible until now. **Consequence:** the CI `build + test + lint` job is gated behind its
`fmt` step (`.github/workflows/ci.yml:34`), so every phase-26 push (Tasks 1/4/8) FAILED at `fmt` and
clippy/build/**test(differential incl. 0034)**/deny NEVER RAN on CI. The authoritative differential
anchor therefore never executed. Verified by `gh run view 27608410401`: `X fmt` → `- clippy …`
`- test …` (all skipped). The fix: `cargo fmt --all` (purely cosmetic line-wrapping, rustfmt 1.95.0,
the pinned toolchain — no semantic change), committed as `e052fc6`. Re-check `cargo fmt --all -- --check`
→ CLEAN (exit 0).

**Local §7.5 gate evidence (fresh, this session):**
- `cargo fmt --all -- --check` → **clean** (exit 0) after `e052fc6`.
- Standalone-crate builds `cargo build -p envoy-config -p envoy-http1 -p envoy-http2 -p envoy-bin`
  → **green** (exit 0) — the isolated-crate build blind spot.
- `cargo build --workspace --all-targets` → **green** (exit 0, finished in 16.34s).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → **clean** (exit 0).
- `cargo test --workspace --exclude differential --exclude h2spec-conformance` → **all pass**
  (exit 0): every crate group `0 failed` (envoy-config 431, envoy-http1 120, envoy-http2 72/1-ignored,
  envoy-admin 96, the envoy-bin `xds_rds_hot_reload` backstop **5 passed**, + all other crates + doc-tests).
  The `differential` + `h2spec-conformance` crates are excluded LOCALLY (Docker/h2spec-dependent) and run
  authoritatively on CI per the established per-task convention + §5.7.
- `cargo deny check` → **`advisories ok, bans ok, licenses ok, sources ok`** (exit 0; the 3
  `license-not-encountered` lines are pre-existing unused-allowance warnings, non-fatal). cargo-deny
  v0.19.9 installed locally for this check.
- **Fuzz (§7.5(d)):** phase 26 added **NO new fuzz target** (Task 9 N/A — no schema change, no new
  parse surface). The pre-existing fuzzers (`parse_bootstrap` + `jwt_parse`) run in CI's SEPARATE `fuzz`
  job, which is independent of the `fmt` gate and was already GREEN on the last push
  (`✓ fuzz (parse_bootstrap + jwt_parse, 30s each) in 2m9s`, run `27608410401`). No new-fuzzer obligation
  this phase.

**Differential — the authoritative anchor (§7.5(a)(b)).** All 34 fixtures (the 33 pre-existing +
`0034-xds-rds-hot-reload`) run via `cargo test --workspace` on native-Linux CI (`ubuntu-latest`), where
real bind-mount inotify makes the 0034 reload observable (unlike this host's Docker Desktop virtiofs —
§5.7 / ADR-0066). The fmt fix `e052fc6` unblocks that path for the first time this phase. **STATE does
NOT advance to state-5-next until that CI run is GREEN** (verification-before-completion: the authoritative
differential must be green, not merely triggered). The CI run for this push is recorded + confirmed below
at the STATE-advance commit.

**CI ITERATION 1 — the gate caught a fixture bug that was NEVER locally exercisable (`superpowers:systematic-debugging`).** The first unblocked CI run (`27707986757`) reached the `test` step for the first time this phase: `fmt ✓ clippy ✓ build ✓`, fuzz job GREEN, but `test` FAILED at fixture `0034`'s differential wrapper `xds_rds_hot_reload_fixture` (panic in 0.27s — far too fast for a reload; an early bail). Root cause (evidence, not guess) from the upstream Envoy container log:
`[critical][main] [source/server/server.cc:416] error initializing config '/etc/envoy/envoy.yaml': yaml-cpp: error at line 2, column 70: bad conversion`. Line 2 col 70 of the rendered reference `envoy.yaml` is `{{ADMIN_PORT}}` — **left unsubstituted**, so Envoy parsed the literal `{{ADMIN_PORT}}` as a `port_value` integer → bad conversion → exit during init → the testcontainers wait-for-ready-log saw EOF before the ready message.
- **Why unsubstituted:** `run_fixture`'s `needs_admin_port` gate (`tests/differential/src/lib.rs:2589`) reserves+substitutes `{{ADMIN_PORT}}` ONLY for `AdminScrape | Http1KeepAlive | Http2KeepAlive` — the admin-SCRAPING drivers. Fixture 0034 uses `Driver::Http1RdsReload`, which converges on `/probe` status/body and **never scrapes admin** — so the gate (correctly) does not substitute. The BUG is in the fixture YAMLs: the author copied 0028's `{{ADMIN_PORT}}` admin block (0028 is the admin-SCRAPING RDS sibling), instead of the non-scraping convention used by every non-admin-scraping fixture.
- **Why never caught earlier:** the full `run_fixture` had NEVER executed for 0034 — locally the upstream reload is Docker-Desktop-virtiofs-unobservable (§5.7) so Task 8 only booted the SUBJECT side (which never parses `{{ADMIN_PORT}}` because the bug bites the reference first), and CI was red at `fmt` the whole phase. This CI run is the FIRST end-to-end execution. The §7.5 gate did exactly its job.
- **Fix (project convention, fixture-only — `tests/fixtures/0034-xds-rds-hot-reload/`):** the non-admin-scraping convention (fixtures 0007/0031/0033) is: reference `envoy.yaml` → `admin` with `port_value: 0` (kernel-ephemeral); subject `envoy-rust.yaml` → **NO admin block**. Applied both, with explanatory comments. The harness `needs_admin_port` gate is left UNCHANGED (it is correct — `Http1RdsReload` does not scrape admin). Subject-side validated locally: `envoy-bin -c <rendered envoy-rust.yaml>` boots with no admin block and serves `/probe` → `200 rds-v1`. The reference-side + full reload differential remain native-Linux-CI-authoritative (virtiofs). Commit `9e1216e`; re-run CI = iteration 2.

**CI ITERATION 2 — the admin fix worked; a second never-locally-exercisable harness bug surfaced (`superpowers:systematic-debugging`).** Run `27708541690`: `fmt ✓ clippy ✓ build ✓`, fuzz GREEN, Envoy now BOOTS (the admin fix landed) and the pre-reload probes run — but `test` FAILED at fixture 0034's reload step:
`fixture passes: atomic-rename of reloaded upstream container RDS file / Caused by: in-container RDS atomic-rename reload exited with code None`. Root cause (evidence + crate-source read, not guess): `UpstreamProxy::reload_rds_atomic` (`tests/differential/src/upstream.rs`) ran the in-container `docker exec` (`sh -c "set -e; … base64 -d … ; mv -f …"`) via `testcontainers::core::ExecCommand::new(...)`, whose `cmd_ready_condition` DEFAULTS to `CmdWaitFor::Nothing` (verified in `testcontainers-0.23.3/src/core/image/exec.rs:15`). With `Nothing`, `async_container::exec()` returns as soon as the command is STARTED (it only blocks-until-finished for the `ExitCode`/`StdOut/StdErrMessage` conditions — `async_container.rs:229-264`), so the immediately-following `result.exit_code().await` reads Docker's `ExitCode: null` on a still-running exec → `None` → the harness bailed "exited with code None" even though the rename itself succeeds. Like iteration 1's bug, this code path had NEVER executed (the container reload is Docker-Desktop-virtiofs-unobservable locally, and CI was red at fmt all phase) — Task 7's PROGRESS even flagged `reload_rds_atomic` as "NOT locally unit-tested (requires Docker) — exercised by the Task-8 fixture on native-Linux CI."
- **Fix (harness, `tests/differential/src/upstream.rs`):** add `.with_cmd_ready_condition(CmdWaitFor::exit_code(0))` to the `ExecCommand` so `exec()` polls `inspect_exec` until a non-None exit code appears (and errors on non-zero), guaranteeing the rename completed before the exit-code read; the existing manual `exit_code()` check is kept as a belt-and-suspenders guard (now returns `Some(0)`). `cargo build/clippy -p differential --all-targets` green; `cargo fmt --check` clean. Commit `ad40b29`; re-run CI = iteration 3.

_(STATE advance to state-4-complete / state-5-next follows on a GREEN CI run — recorded in the STATE-advance commit.)_
