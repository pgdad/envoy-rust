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

## Task 8 — fixture `0034-xds-rds-hot-reload` + Docker wrapper (Linux-CI-authoritative) + in-process backstop [BLOCKED on Task 1]

_(pending)_

## Task 9 — CONDITIONAL: `ConfigSource.watched_directory` field + fuzz seed (fires only if Task 1 P2 requires it)

_(pending — conditional)_

## Task 10 — state-4 phase-done verification + STATE advance to state-5-next

_(pending)_
