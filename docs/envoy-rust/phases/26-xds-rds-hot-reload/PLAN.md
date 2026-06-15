# Phase 26 (`26-xds-rds-hot-reload`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (the project default per `feedback_execution_style`) to implement this plan task-by-task, SERIALLY (per `feedback_serial_subagent_dispatch` — never dispatch implementers in parallel; they race on shared `main`). Steps use checkbox (`- [ ]`) syntax. TDD per task (`superpowers:test-driven-development` — tests first). Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` **PER TASK** (per `project_state3_arc_skips_clippy`). One code commit + one PROGRESS commit per task.

**Goal:** Make file-based RDS route configurations hot-reloadable — a running HCM whose route table is RDS-supplied picks up an edited RDS file WITHOUT a restart, atomically swapping the live route table, with the per-HCM `http.<stat_prefix>.rds.<route_config_name>.{update_attempt,update_success,config_reload}` counters advancing past their initial-load values and `/config_dump`'s `RoutesConfigDump` reflecting the new version — bilaterally verified against upstream Envoy by fixture `0034-xds-rds-hot-reload` (Linux-CI-authoritative per ADR-0049).

**Architecture:** The route table is already `HCMConfig.route_config: Arc<RouteConfiguration>` (`crates/envoy-http1/src/hcm.rs:122`) and route matching is per-request stateless, so hot-reload = an atomic pointer swap. Migrate that owned `Arc` to a shared swappable handle (dep-free `arc_swap`-free `RwLock<Arc<RouteConfiguration>>` or `tokio::sync::watch`), spawn a 5th periodic-background primitive (an `RdsWatcher` mirroring `envoy-health::Scheduler` — poll-based mtime, `CancellationToken` discipline) that re-reads the file on change, re-parses via the existing `rds.rs`, re-validates route→cluster refs against the immutable live cluster set, and atomically stores the new `Arc`. Reload is **warm-reject** (last-good table kept + `update_failure`/`update_rejected` ticked on a bad reload — the one ADR-0049 startup-all-fatal carve-out). No cluster/listener/pool mutation — only the route table mutates.

**Tech Stack:** Rust (stable, pinned). `serde`/`serde_yaml` (existing). `std::fs` (mtime poll + file read — existing). `tokio` + `tokio_util::sync::CancellationToken` (existing — the watcher task). `envoy-stats` (counters — existing, phase-20 registration reused). **No new crate, no new top-level Cargo dep** (poll-based watch; dep-free swap handle).

---

## ⚠️ STATUS: DRAFT — §6.2 EMPIRICAL VERIFICATION DEFERRED TO STATE-3 TASK 1 (LINUX-ONLY)

**This PLAN was authored in macOS-deferred mode (a deliberate, user-approved departure from the verify-at-PLAN-write cadence).** The phase-26 §6.2 empirical verification **MUST run on Linux** — the RDS hot-reload trigger (a file change observed inside the Envoy container) is unobservable on macOS Docker Desktop virtiofs (SPEC §0 finding 4 / §5.7 / ADR-0049 Provenance). The PLAN-write session was on macOS, so §6.2 was NOT run and the §6.2-dependent shapes are NOT yet locked.

**Consequence — the standard cadence is shifted by one state:**

- Normally §6.2 runs at the state-2 PLAN-write and locks the shapes + fires the reconciliation ADR before any code. Here, **§6.2 becomes Task 1 of state-3** — run on Linux, FIRST, before any §6.2-dependent task.
- **ADR-0066 is NOT fired at this PLAN-write** (it cannot be evaluated without §6.2). It is fired **inline during Task 1** if §6.2 diverges materially from the projections below (the `watched_directory` schema field / the reload-counter values / the bad-reload disposition / the config_dump-version shape). DECISIONS.md ledger head stays **ADR-0065** until Task 1.
- The §6.2-INDEPENDENT foundation tasks (Task 2 the route-table-handle migration; Task 3 the watcher poll-loop skeleton) **may be implemented before Task 1** — they depend on the code anchors (verified on macOS below), not on Envoy's reload semantics. The §6.2-DEPENDENT tasks (Task 4 reload-pipeline stat/disposition semantics; Task 6 config_dump version; Task 7 the harness reload operation; Task 8 fixture 0034) are **BLOCKED on Task 1** and carry projected values clearly labelled "§6.2-PENDING — confirm/replace from Task 1's record."
- **Projected values below are the SPEC's projections, NOT verified facts.** Every one marked `[§6.2-PENDING]` is a hypothesis Task 1 confirms or corrects.

**Recommended state-3 ordering:** Task 1 (Linux §6.2) → Task 2 + Task 3 (foundation; can also precede Task 1) → Tasks 4–9 (§6.2-locked).

---

## §6.2 projections (UNVERIFIED — to be confirmed/replaced by state-3 Task 1 on Linux)

The SPEC §6.2 7-item checklist (`docs/envoy-rust/phases/26-xds-rds-hot-reload/SPEC.md`). Projected dispositions the downstream tasks assume until Task 1 records the truth:

- **P1 (reload happens + readiness)** `[§6.2-PENDING]` — editing the RDS file under a running Envoy re-routes traffic without a restart; the listener stays up. Settle latency informs the Task 7 harness wait bound.
- **P2 (file-change operation — THE most consequential)** `[§6.2-PENDING]` — projected: in-place truncate-rewrite triggers Envoy's default file-watch; atomic-rename may be MISSED without `watched_directory`. **If atomic-rename is required → phase 26 adds `ConfigSource.watched_directory` (parse-and-honor) + one fuzz seed (Task 9) + ADR-0066 fires.** The Task 7 harness uses whatever operation Task 1 proves triggers BOTH proxies.
- **P3 (distinguishable backends)** `[§6.2-PENDING]` — projected: two real `http1-echo-server` clusters distinguished by the per-cluster `cluster.<name>.upstream_rq_total` discriminator (no echo-body marker needed). Confirm a single-endpoint-per-cluster pair suffices.
- **P4 (reload counter values)** `[§6.2-PENDING]` — projected `update_attempt/update_success/config_reload = 2/2/2` after one successful reload; `config_reload` ticks on each reload.
- **P5 (bad-reload disposition — locks §5.5 + Task 4)** `[§6.2-PENDING]` — projected: malformed YAML → `update_failure`+last-good kept; missing `route_config_name` / unknown-cluster route → `update_rejected`+last-good kept; no traffic dropped. envoy-rust MATCHES (warm-reject, not all-fatal — the one ADR-0049 carve-out).
- **P6 (config_dump on reload)** `[§6.2-PENDING]` — projected: `dynamic_route_configs[].route_config` reflects the NEW routes; `version_info` and/or `last_updated` change; fixtures 0026/0027 `configs[]` indices unaffected.
- **P7 (in-flight isolation — opportunistic)** `[§6.2-PENDING]` — projected: a request begun pre-reload completes under the old table (backstop-only; §5.4).

---

## PLAN-time SPEC corrections (code anchors verified on macOS at HEAD `1785a0d42`, read-only — these are §6.2-INDEPENDENT and locked NOW)

- **C1.** `HCMConfig.route_config: Arc<RouteConfiguration>` at `crates/envoy-http1/src/hcm.rs:122`; constructed at `:209` (`Arc::new(clone_route_config(cfg.route_config.as_ref().expect(...)))`); `clone_route_config` helper at `:224`. (SPEC §0 finding 1 anchor exact.)
- **C2.** The per-request READ sites (the swap-consumer migration surface): `crates/envoy-http1/src/hcm.rs:1226` + `:1253` (the vhost/route match path); the H2 mirror reaches the route table via `crates/envoy-http2/src/hcm.rs:468` `apply_route_config`. `HCMConfig` lives in `envoy-http1` and is re-exported as a type alias by `envoy-http2` (SPEC §0 finding 1 / hcm.rs:115-118 doc-comment), so BOTH crates read the same field — the migration is a single field-type change rippling through both. (Exact — Task 2 enumerates every site via the SPEC-correction grep.)
- **C3.** The periodic-background-task primitive: `envoy_health::Scheduler::spawn(bootstrap: &Bootstrap, cluster_mgr: Arc<ClusterManager>, registry: Arc<StatsRegistry>, cancel: CancellationToken) -> Result<Self, HealthError>` at `crates/envoy-health/src/scheduler.rs:40`; holds `JoinHandle`s; `cancel` is the shared shutdown token; loops exit at their `tokio::select!` boundary. (Exact — the `RdsWatcher` template.)
- **C4.** envoy-bin spawn site: `crates/envoy-bin/src/main.rs:180` (`Scheduler::spawn(...)`) beside the 14.2 `OutlierManager::for_bootstrap(&cluster_mgr, token.clone())` at `:194`; the shared `CancellationToken` is `token` (`:91`), drained via `shutdown().await` on the runtime drain path. (Exact — the `RdsWatcher` is constructed here, after the listeners are built.)
- **C5.** The phase-20 RDS load path the reload re-invokes: `crates/envoy-config/src/rds.rs` (`parse_rds_file`); `load_dynamic_resources` in `crates/envoy-config/src/lib.rs`; the per-HCM `rds.*` registration (phase-20 `register_rds_stats` / `HCMStats`); the `RoutesConfigDump` entry in `crates/envoy-admin/src/endpoint.rs`. **Task 1's §6.2 grep re-verifies these exact symbols on the state-3 HEAD** (they are phase-20 code; this PLAN cites them from the phase-20 PLAN/SPEC, not re-grepped line-exact here — a Task-1 SPEC-correction item).
- **C6.** Fixture baseline: `tests/fixtures/0001`…`0033` (next is `0034`); the existing RDS fixture `tests/fixtures/0028-xds-file-based-rds` is the §5.2 idle-watcher regression witness. Harness dynamic-file rendering/mounting (`{{RDS_PATH}}`) in `tests/differential/src/lib.rs` (phase-20 Task 6). (Exact.)

---

## §6.1 split-gate decision (projected SINGLE PHASE — split NOT fired at this PLAN-write; re-evaluate at Task 1)

Projected surface (SPEC §6.1): D1 ~120 (+120 tests) · D2 ~150 (+100) · D3 ~140 (+150) · D4 ~60 (+50) · D5 ~70 (+50) · D6 ~160 (+60) · D7 ~220 · D8 ~200 = **~1200–1600 LoC / 9 implementation tasks + a §6.2 task + a state-4 task**. **Under the §6.1 ~1500-LoC / ~25-task gate — single phase.** Re-evaluate at Task 1: if §6.2 forces `watched_directory` (Task 9) + materially grows D3/D7, and the refined estimate crosses the gate, split at the SPEC §1 seam (`26.1` = Tasks 1–3 + the in-process backstop [foundation; regression-equivalence incl. 0028's idle watcher] / `26.2` = Tasks 4–9 [stat-tick + config_dump + harness + fixture 0034 + close]) with **ADR-0067** — a Task-1 decision.

---

## File structure

- **Modify** `crates/envoy-http1/src/hcm.rs` — `HCMConfig.route_config` field type (`Arc<RouteConfiguration>` → the swappable handle); the construction site (`:209`); the per-request read sites (`:1226`,`:1253`); test constructors (`:1695`+ — many literal `route_config: Arc::new(...)` sites adapt). (Tasks 2, 4, 6.)
- **Modify** `crates/envoy-http2/src/hcm.rs` — the H2 read path (`:468`) + the type-alias re-export consumers + test constructors. (Task 2.)
- **Create** `crates/envoy-http1/src/rds_watcher.rs` (or a PLAN-of-record location decided at Task 2) — the `RdsWatcher` periodic primitive + the reload pipeline. (Tasks 3, 4.)
- **Modify** `crates/envoy-bin/src/main.rs` — construct + spawn the `RdsWatcher` after the listeners (beside `:180-194`); drain on the shutdown path. (Task 3.)
- **Modify** `crates/envoy-config/src/lib.rs` / `rds.rs` — expose the re-parse+re-validate entry point the watcher calls (refactor the phase-20 initial-load path so the reload reuses it). (Task 4.)
- **Modify** `crates/envoy-admin/src/endpoint.rs` — `RoutesConfigDump` reads through the swappable handle + the version/`last_updated` update. (Task 6.)
- **Modify** `tests/differential/src/lib.rs` — the mid-test file-rewrite + settle probe-step. (Task 7.)
- **Create** `tests/fixtures/0034-xds-rds-hot-reload/` + `tests/differential/tests/xds_rds_hot_reload.rs`. (Task 8.)
- **Create** `crates/envoy-bin/tests/xds_rds_hot_reload.rs` — the in-process backstop (`tokio::time`-controlled). (Task 8.)
- **Modify** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — the §2.1 reload-semantics column + the §2.2 hot-reload subsection. (lands at the task that first exercises each.)
- **Conditional create** `crates/envoy-config/fuzz/corpus/parse_bootstrap/config_source_watched_directory.yaml` — ONLY if Task 1 forces the `watched_directory` field. (Task 9.)

---

### Task 1: §6.2 empirical verification on LINUX (run FIRST; fires ADR-0066 if divergent)

**This task MUST run on a Linux host / Linux CI** — the reload trigger is macOS-unobservable (SPEC §5.7 / ADR-0049). It replaces the PLAN-write §6.2 the macOS session could not run.

**Files:** none (verification + DECISIONS.md/PLAN.md update only).

- [ ] **Step 1: Stand up the probe rig (the ADR-0063 methodology).** On Linux, run `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) with an `rds`-configured HCM (`route_config_name: local_route`, `path_config_source.path` → a mounted file routing `/probe` → a host backend cluster) + admin `/stats` + `/config_dump`. Confirm the initial `GET /probe` routes correctly and `http.ingress_http.rds.local_route.{update_attempt,update_success,config_reload}` read their initial values.
- [ ] **Step 2: Resolve P2 (the file-change operation).** Edit the mounted RDS file BOTH ways — (a) in-place truncate-rewrite, (b) write-temp-then-`mv` (atomic-rename) — and observe which triggers Envoy's default (no-`watched_directory`) reload. Record the operation that reliably reloads. **If only atomic-rename reloads (or `watched_directory` is required) → this task adds the `ConfigSource.watched_directory` parse-and-honor field to the plan (Task 9 fuzz seed) and FIRES ADR-0066.**
- [ ] **Step 3: Resolve P1/P4/P5/P6.** After a successful reload: record the settle latency (Task 7 wait bound); the exact `rds.*` counter values (P4 — projected `2/2/2`); the `/config_dump` `RoutesConfigDump` post-reload JSON (P6 — `version_info`/`last_updated`/route shape; confirm 0026/0027 indices unaffected). Then drive bad reloads (malformed YAML / missing `route_config_name` / route → unknown cluster) and record the disposition (P5 — `update_failure` vs `update_rejected`; last-good kept; no dropped traffic).
- [ ] **Step 4: Resolve P3.** Confirm two single-endpoint `http1-echo-server` clusters are distinguishable via `cluster.<name>.upstream_rq_total` (or an echo marker) so the route-change is observable.
- [ ] **Step 5: Lock the shapes.** Replace every `[§6.2-PENDING]` projection in this PLAN (P1–P7) + the §2.1/§2.2 BEHAVIOR_CONTRACT projections with the recorded facts. Fire **ADR-0066** at this commit IFF any of P2(`watched_directory`)/P4/P5/P6 diverged materially (commit title carries `[ADR-0066]`); else record "§6.2 verified, no reconciliation — ADR-0066 unfired" in PROGRESS. Re-evaluate the §6.1 split gate (Task 1 decides ADR-0067).
- [ ] **Step 6: Commit** the PLAN/PROGRESS/BEHAVIOR_CONTRACT lock-ins (+ ADR-0066 if fired) as the Task-1 docs commit; transcript preserved in PROGRESS Task-1 per the ADR-0049 Provenance discipline.

### Task 2: Route-table-handle migration (`Arc<RouteConfiguration>` → a shared swappable handle) — §6.2-INDEPENDENT, may precede Task 1

**Files:** Modify `crates/envoy-http1/src/hcm.rs` (`:122` field, `:209` construct, `:1226`/`:1253` read, test constructors), `crates/envoy-http2/src/hcm.rs` (`:468` + test constructors).

- [ ] **Step 1: Decide the handle type + write the failing read-through-handle test.** Default per §5.1: `route_config: Arc<arc_swap_free_handle>` where the handle = `std::sync::RwLock<Arc<RouteConfiguration>>` (or `tokio::sync::watch::Receiver<Arc<RouteConfiguration>>` if the PLAN-of-record prefers a lock-free read). Write a unit test asserting (a) a request reads the CURRENT `Arc` once at entry, (b) a `store(new_arc)` is visible to the NEXT read, (c) an in-flight reader keeps its snapshot (the §5.4 read-once guarantee). Test goes in `hcm.rs` `#[cfg(test)]`.
- [ ] **Step 2: Run it — expect FAIL** (the field is still a plain `Arc`; no swap API). `cargo test -p envoy-http1 route_table_handle -- --nocapture` → FAIL.
- [ ] **Step 3: Migrate the field + the read sites + the construct site.** Change the field type; add a `current_route_config(&self) -> Arc<RouteConfiguration>` accessor (read-once); migrate `:1226`/`:1253` (and the H2 `:468` path) to call it; seed the handle at `:209`. Enumerate EVERY `route_config:` literal/read in both crates via `grep -rn 'route_config' crates/envoy-http1/src crates/envoy-http2/src` and adapt each (the phase-20 `route_config`→`Option` sweep precedent). Add a `store_route_config(&self, rc: Arc<RouteConfiguration>)` writer (used by Task 4).
- [ ] **Step 4: Run the new test + the full crate tests.** `cargo test -p envoy-http1 -p envoy-http2` → PASS. `cargo clippy -p envoy-http1 -p envoy-http2 --all-targets -- -D warnings` clean.
- [ ] **Step 5: Regression check (the §5.2 witness).** `cargo build -p envoy-config -p envoy-http1 -p envoy-http2` (isolated, per `project_isolated_crate_build_blindspot`) + `cargo test --workspace` green — the migration must be behavior-preserving for every non-reloading request (fixture 0028's path unchanged).
- [ ] **Step 6: Commit** (`feat(http): make HCM route table a swappable handle for hot-reload`) + PROGRESS commit.

### Task 3: The `RdsWatcher` — the 5th periodic-background primitive (skeleton) — §6.2-INDEPENDENT, may precede Task 1

**Files:** Create `crates/envoy-http1/src/rds_watcher.rs` (or PLAN-of-record location); Modify `crates/envoy-bin/src/main.rs` (`:180-194` spawn region); export from the crate `lib.rs`.

- [ ] **Step 1: Write the failing watcher-lifecycle test.** Assert: `RdsWatcher::spawn(targets, cancel)` returns a handle; with a target whose file mtime never changes, NO reload fires; cancelling `cancel` (or `shutdown().await`) terminates the loop. Use `tokio::time` (paused clock) to drive the poll interval deterministically. A "target" = `{ path: PathBuf, route_config_name: String, write_handle: <Task-2 writer>, stats: <Arc<Counter> handles> }`.
- [ ] **Step 2: Run it — expect FAIL** (no `RdsWatcher`). `cargo test -p envoy-http1 rds_watcher -- --nocapture` → FAIL.
- [ ] **Step 3: Implement the watcher skeleton** mirroring `envoy_health::Scheduler` (`scheduler.rs:40`): hold `JoinHandle`s + the `CancellationToken`; one `watch_loop` per target; the loop `tokio::select!`s between `cancel.cancelled()` and a `tokio::time::interval` tick; on tick, `std::fs::metadata(path)?.modified()` vs the last-seen mtime; on change, call the reload pipeline (Task 4 — stub it to a no-op `reload(target)` returning `Ok/Err` THIS task, real in Task 4); `shutdown(self)` awaits the handles.
- [ ] **Step 4: Wire envoy-bin.** Build the target list by walking `all_listeners()` for HCMs with `rds` configured (inert/empty otherwise — §5.2); `RdsWatcher::spawn(targets, token.clone())` after `:180`; drain via `shutdown().await` on the runtime drain path.
- [ ] **Step 5: Run + clippy + workspace.** `cargo test -p envoy-http1`, `cargo test --workspace` (incl. 0028's now-spawned-but-idle watcher path), `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] **Step 6: Commit** + PROGRESS commit.

### Task 4: The reload pipeline (re-parse → re-validate → atomic swap; stat-tick + warm-reject) — §6.2-DEPENDENT (P4/P5), BLOCKED on Task 1

**Files:** `crates/envoy-http1/src/rds_watcher.rs` (the real `reload`); `crates/envoy-config/src/lib.rs`/`rds.rs` (expose a `reparse_and_select_route_config(path, name, &cluster_set) -> Result<RouteConfiguration, ConfigError>` reusing the phase-20 parser+validator).

- [ ] **Step 1: Write the failing reload tests (happy + warm-reject).** Happy: a valid file change → `store_route_config` lands the new table + `update_attempt`/`update_success`/`config_reload` tick **the Task-1-recorded values** (P4). Warm-reject: a malformed file → last-good kept + `update_failure` ticks (P5); a vanished `route_config_name` / unknown-cluster route → last-good kept + `update_rejected` ticks (P5). Drive via `tokio::time` + temp files in-process.
- [ ] **Step 2: Run — expect FAIL** (reload is the Task-3 no-op stub).
- [ ] **Step 3: Implement `reload`.** Re-read file → `reparse_and_select_route_config` (reuse phase-20 `rds.rs` parse + the route→cluster validator against the immutable live cluster set) → on `Ok`: `store_route_config(Arc::new(new))` + tick attempt/success/config_reload + bump the config_dump version (Task 6 hook) ; on `Err`: KEEP the handle + tick attempt + failure/rejected per the error class (the §5.5 warm-reject — **NOT** all-fatal). Map error classes to counters per the Task-1 P5 record.
- [ ] **Step 4: Run + clippy.** PASS; warm-reject paths assert the table is byte-unchanged after a bad reload.
- [ ] **Step 5: Commit** + PROGRESS commit.

### Task 5: Per-HCM `rds.*` counters tick per reload (thread the phase-20 stat handles to the watcher target) — §6.2-INDEPENDENT wiring (values from Task 1)

**Files:** `crates/envoy-http1/src/hcm.rs` (expose the phase-20-registered `rds.*` `Arc<Counter>` handles); `crates/envoy-bin/src/main.rs` (pass them into the Task-3 target build); `rds_watcher.rs` (consume).

- [ ] **Step 1: Write the failing test** — the watcher target carries the SAME `Arc<Counter>` handles the HCM registered at construction (phase-20 `register_rds_stats`), and a reload increments THOSE registry counters (scrape-visible), not a private copy.
- [ ] **Step 2: Run — FAIL.** **Step 3: Thread the handles** from the HCM construction site into the target list build (the 06.x `Arc<Counter>`-shared-handle idiom). **Step 4: Run + clippy.** **Step 5: Commit** + PROGRESS.

### Task 6: `/config_dump` `RoutesConfigDump` reads through the swappable handle + version/`last_updated` on reload — §6.2-DEPENDENT (P6), BLOCKED on Task 1

**Files:** `crates/envoy-admin/src/endpoint.rs` (`ConfigDumpEntry::Routes`).

- [ ] **Step 1: Write the failing test** — after a reload, `/config_dump` `RoutesConfigDump` reflects the NEW route table + the Task-1-recorded version/`last_updated` shape (P6); pre-reload it shows the initial table. **Step 2: Run — FAIL** (renderer reads a startup snapshot). **Step 3: Implement** — render through `current_route_config()` (Task 2) + carry the version/timestamp updated by Task 4; keep emission conditional on `rds` (phase-20 conditionality; 0026/0027 untouched). **Step 4: Run + clippy.** **Step 5: Commit** + PROGRESS.

### Task 7: Harness — mid-test fixture-file rewrite + settle-then-probe — §6.2-DEPENDENT (P1/P2), BLOCKED on Task 1

**Files:** `crates/.../tests/differential/src/lib.rs` (a new probe-step type + the expectations schema extension).

- [ ] **Step 1: Write the failing harness-unit test** — a "reload step" writes per-side-rendered new contents to the SAME mounted path using the **Task-1-confirmed file-change operation** (P2: in-place rewrite, or atomic-rename if `watched_directory`), then settles via bounded wait-for-convergence on a discriminating observable (the routed-to cluster, or `config_reload` advancing — the 12.2 pattern, NOT a fixed sleep, bounded by the Task-1 settle latency). **Step 2: Run — FAIL.** **Step 3: Implement** the reload step + the expectations "reload" + post-reload probe/assert block (generalize the phase-20 `{{RDS_PATH}}` machinery). **Step 4: Run.** **Step 5: Commit** + PROGRESS.

### Task 8: Fixture `0034-xds-rds-hot-reload` + Docker-gated wrapper + in-process backstop — §6.2-DEPENDENT, BLOCKED on Task 1

**Files:** Create `tests/fixtures/0034-xds-rds-hot-reload/` (`envoy.yaml`/`envoy-rust.yaml`/initial+reload+malformed RDS templates/`expectations.yaml`/`README.md`), `tests/differential/tests/xds_rds_hot_reload.rs` (Docker-gated; **Linux-CI-authoritative — README notes macOS-local-unobservability**), `crates/envoy-bin/tests/xds_rds_hot_reload.rs` (the `tokio::time`-controlled backstop: happy reload + the 4 negative paths + in-flight isolation; the deterministic local complement to the Linux-only differential).

- [ ] **Step 1: Author the fixture** (two distinguishable `http1-echo-server` clusters per P3; the §1 three-phase pre→reload→post sequence + the bad-reload probe; assert P4 counter values + P6 config_dump). **Step 2: Author the backstop** (covers the paths the Linux-only fixture can't cleanly drive). **Step 3: Run the backstop locally** (`cargo test -p envoy-bin xds_rds_hot_reload`) — pre-build `tests/helpers/*` first (per `project_flaky_access_log_fixture_0012`). **Step 4: Verify the fixture on Linux CI** (the differential evidence). **Step 5: Commit** + PROGRESS.

### Task 9: Conditional — `ConfigSource.watched_directory` field + fuzz seed — FIRES ONLY IF Task 1 P2 requires it

**Files (conditional):** `crates/envoy-config/src/bootstrap.rs` (`ConfigSource.watched_directory: Option<WatchedDirectory>` parse-and-honor; today rejected by `deny_unknown_fields`); `crates/envoy-config/fuzz/corpus/parse_bootstrap/config_source_watched_directory.yaml` (corpus +1). Only if Task 1 proved Envoy needs `watched_directory` for the reload operation. If unfired, record "Task 9 N/A — in-place rewrite triggered both proxies" in PROGRESS.

### Task 10: State-4 phase-done verification + STATE advance to state-5-next

**Files:** PROGRESS (evidence), STATE.

- [ ] Run the full §7.5 gate: `cargo fmt --all -- --check`, the 4 standalone-crate builds (`-p envoy-config -p envoy-http1 -p envoy-http2 -p envoy-bin`), `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`, the fuzz short-budget run. **The AUTHORITATIVE differential anchor is the Linux CI run** (fixture 0034's reload is macOS-unobservable — ADR-0049 / §5.7); the local Docker differential corroborates the 33 EXISTING fixtures only. Quote per-gate outputs into PROGRESS; advance STATE to state-5-next.

---

## Self-review

- **Spec coverage:** D1→Task 2; D2→Task 3; D3→Task 4; D4→Task 5; D5→Task 6; D6→Task 7; D7→Task 8; D8→Task 8 (backstop+contract); the SPEC §6.2 checklist→Task 1; the conditional `watched_directory`→Task 9; §7.5 gate→Task 10. All SPEC §3 deliverables mapped.
- **§6.2-dependency honesty:** every value Task 1 must supply is marked `[§6.2-PENDING]` and the dependent tasks name Task 1 as the source — no fabricated constants, no bare "TBD." Foundation Tasks 2–3 are §6.2-independent and fully concrete.
- **Type consistency:** the swappable handle (Task 2) exposes `current_route_config()` (read) + `store_route_config()` (write); Task 4 calls `store_route_config`, Task 6 calls `current_route_config` — consistent. The watcher target shape (Task 3) carries `{path, route_config_name, write_handle, stats}` — consumed identically in Tasks 4/5.
- **Ordering:** Task 1 (Linux) gates Tasks 4/6/7/8/9; Tasks 2/3 may precede it. Recorded at the top + per-task.
- **Departure recorded:** the macOS-deferred §6.2 (moved to Task 1) + the deferred ADR-0066 evaluation are flagged in the STATUS banner, the §6.2 projections section, and Task 1 — and will be mirrored in STATE + next-prompt.txt.
