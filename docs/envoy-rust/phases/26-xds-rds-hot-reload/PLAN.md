# Phase 26 (`26-xds-rds-hot-reload`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (the project default per `feedback_execution_style`) to implement this plan task-by-task, SERIALLY (per `feedback_serial_subagent_dispatch` — never dispatch implementers in parallel; they race on shared `main`). Steps use checkbox (`- [ ]`) syntax. TDD per task (`superpowers:test-driven-development` — tests first). Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` **PER TASK** (per `project_state3_arc_skips_clippy`). One code commit + one PROGRESS commit per task.

**Goal:** Make file-based RDS route configurations hot-reloadable — a running HCM whose route table is RDS-supplied picks up an edited RDS file WITHOUT a restart, atomically swapping the live route table, with the per-HCM `http.<stat_prefix>.rds.<route_config_name>.{update_attempt,update_success,config_reload}` counters advancing past their initial-load values and `/config_dump`'s `RoutesConfigDump` reflecting the new version — bilaterally verified against upstream Envoy by fixture `0034-xds-rds-hot-reload` (Linux-CI-authoritative per ADR-0049).

**Architecture:** The route table is already `HCMConfig.route_config: Arc<RouteConfiguration>` (`crates/envoy-http1/src/hcm.rs:122`) and route matching is per-request stateless, so hot-reload = an atomic pointer swap. Migrate that owned `Arc` to a shared swappable handle (dep-free `arc_swap`-free `RwLock<Arc<RouteConfiguration>>` or `tokio::sync::watch`), spawn a 5th periodic-background primitive (an `RdsWatcher` mirroring `envoy-health::Scheduler` — poll-based mtime, `CancellationToken` discipline) that re-reads the file on change, re-parses via the existing `rds.rs`, re-validates route→cluster refs against the immutable live cluster set, and atomically stores the new `Arc`. Reload is **warm-reject** (last-good table kept + `update_failure`/`update_rejected` ticked on a bad reload — the one ADR-0049 startup-all-fatal carve-out). No cluster/listener/pool mutation — only the route table mutates.

**Tech Stack:** Rust (stable, pinned). `serde`/`serde_yaml` (existing). `std::fs` (mtime poll + file read — existing). `tokio` + `tokio_util::sync::CancellationToken` (existing — the watcher task). `envoy-stats` (counters — existing, phase-20 registration reused). **No new crate, no new top-level Cargo dep** (poll-based watch; dep-free swap handle).

---

## ✅ STATUS: §6.2 EMPIRICALLY VERIFIED at state-3 Task 1 (Linux) — ADR-0066 FIRED — Tasks 4/6/7/8/9 UNBLOCKED

**The macOS-deferred §6.2 empirical verification was run on Linux as state-3 Task 1 (2026-06-16) against `envoyproxy/envoy:v1.33.0` and the shapes are now LOCKED.** The DRAFT projections below are replaced by verified facts; **ADR-0066 FIRED** (P2, P5(iii), and P6 diverged materially — see DECISIONS.md ADR-0066 + the §6.2-VERIFIED section below + PROGRESS Task 1). DECISIONS.md ledger head is now **ADR-0066** (count 67). The §6.2-DEPENDENT tasks (Task 4 reload pipeline; Task 6 config_dump; Task 7 harness reload op; Task 8 fixture 0034) are UNBLOCKED. **Task 9 (`watched_directory`) does NOT fire** (no schema change). **The §6.1 split does NOT fire** (single phase confirmed; ADR-0067 stays unfired).

**Cadence note (historical):** §6.2 normally runs at the state-2 PLAN-write; here it was deferred to state-3 Task 1 because the reload trigger is unobservable on macOS Docker Desktop virtiofs (SPEC §0 finding 4 / §5.7 / ADR-0049). The foundation Tasks 2 + 3 (route-table-handle migration; `RdsWatcher` skeleton) landed BEFORE Task 1 (they are §6.2-independent). **Important environment caveat carried forward:** even this Linux host runs Docker Desktop (virtiofs bind-mounts do NOT propagate inotify into the container) — the Task-1 probe got real reloads only via a Docker volume + `docker exec`. Fixture 0034's differential reload therefore remains **Linux-CI-authoritative on a NATIVE-Linux runner** (real bind-mount inotify), NOT locally observable here. Local verification = the in-process backstop (Task 8).

**State-3 ordering (Task 1 DONE):** ~~Task 1 (Linux §6.2)~~ ✅ → ~~Task 2 + Task 3 (foundation)~~ ✅ → **Tasks 4 → 5 → 6 → 7 → 8 → (9 N/A) → 10** (§6.2-locked, the next unstarted is Task 4).

---

## §6.2 VERIFIED (state-3 Task 1, Linux, 2026-06-16 — ADR-0066) — the locked facts the downstream tasks consume

The SPEC §6.2 7-item checklist, RESOLVED (full transcript: PROGRESS Task 1; decision record: ADR-0066):

- **P1 (reload happens + readiness) — MATCHES.** Editing the RDS file under a running Envoy re-routes live traffic with no restart; **settle latency ~50 ms** (6–60 ms observed → the Task-7 harness wait bound is a bounded wait-for-convergence on a discriminating observable, NOT a fixed sleep); the listener stays up (zero drops under concurrent load).
- **P2 (file-change operation) — DIVERGES → drives Task 7 + drops Task 9.** Envoy's default file-watch reloads on **ATOMIC-RENAME ONLY** (write-temp-then-`mv`); **in-place truncate-rewrite is NEVER detected** (confirmed 3×), and `watched_directory` does NOT rescue in-place. **`watched_directory` is NOT required → Task 9 does NOT fire (no config-schema change).** The Task-7 harness MUST rewrite the fixture file via atomic-rename so BOTH proxies reload. envoy-rust's poll-based mtime watcher detects atomic-rename (fresh inode mtime) → the landed Task-3 skeleton needs no change.
- **P3 (distinguishable backends) — MATCHES.** `cluster.backend_a.upstream_rq_total` vs `cluster.backend_b.upstream_rq_total` cleanly discriminates the routed-to cluster; one STATIC endpoint per cluster suffices.
- **P4 (reload counter values) — MATCHES.** `update_attempt/update_success/update_failure/update_rejected/config_reload` = `1/1/0/0/1` (boot) → `2/2/0/0/2` (one reload) → `3/3/0/0/3` (two). `update_attempt`+`update_success`+`config_reload` each +1 per successful reload. Fixture 0034 asserts `2/2/…/2` after its one reload.
- **P5 (bad-reload disposition — locks §5.5 + Task 4) — PARTIALLY DIVERGES.** {malformed YAML / IO / parse → `update_failure` +1, last-good KEPT} — MATCHES; {`route_config_name` absent → `update_rejected` +1, last-good KEPT} — MATCHES; {route → UNKNOWN cluster → **Envoy ACCEPTS** (`update_success`+`config_reload` +1, serves 503/`no_cluster`, last-good NOT kept)} — **DIVERGES**. **envoy-rust does NOT mirror the unknown-cluster acceptance**: it re-validates route→cluster refs against the immutable live cluster set and **warm-rejects** (`update_rejected` +1, last-good KEPT) — a recorded deliberate divergence (ADR-0066), because the request path `.expect()`s cluster existence (`hcm.rs:818`) and matching Envoy would need a request-time 503 synth path out of scope; unobservable in fixture 0034 (backstop-only).
- **P6 (config_dump on reload) — MINOR DIVERGENCE (already-correct in envoy-rust).** `dynamic_route_configs[]` keys are `[@type, route_config, last_updated]` — **NO `version_info`** for file-RDS (already the phase-20 `RoutesConfigDump` shape — endpoint.rs); `last_updated` changes; `route_config` reflects the NEW table. Task 6 only needs the renderer to read through the swappable handle. 0026/0027 indices unaffected.
- **P7 (in-flight isolation) — MATCHES (resolved).** A request begun pre-reload completes under the OLD table (verified on Envoy with a 5 s in-flight request); only NEW requests see the swap (§5.4 read-once). Backstop asserts the same for envoy-rust.

---

## PLAN-time SPEC corrections (code anchors verified on macOS at HEAD `1785a0d42`, read-only — these are §6.2-INDEPENDENT and locked NOW)

- **C1.** `HCMConfig.route_config: Arc<RouteConfiguration>` at `crates/envoy-http1/src/hcm.rs:122`; constructed at `:209` (`Arc::new(clone_route_config(cfg.route_config.as_ref().expect(...)))`); `clone_route_config` helper at `:224`. (SPEC §0 finding 1 anchor exact.)
- **C2.** The per-request READ sites (the swap-consumer migration surface): `crates/envoy-http1/src/hcm.rs:1226` + `:1253` (the vhost/route match path); the H2 mirror reaches the route table via `crates/envoy-http2/src/hcm.rs:468` `apply_route_config`. `HCMConfig` lives in `envoy-http1` and is re-exported as a type alias by `envoy-http2` (SPEC §0 finding 1 / hcm.rs:115-118 doc-comment), so BOTH crates read the same field — the migration is a single field-type change rippling through both. (Exact — Task 2 enumerates every site via the SPEC-correction grep.)
- **C3.** The periodic-background-task primitive: `envoy_health::Scheduler::spawn(bootstrap: &Bootstrap, cluster_mgr: Arc<ClusterManager>, registry: Arc<StatsRegistry>, cancel: CancellationToken) -> Result<Self, HealthError>` at `crates/envoy-health/src/scheduler.rs:40`; holds `JoinHandle`s; `cancel` is the shared shutdown token; loops exit at their `tokio::select!` boundary. (Exact — the `RdsWatcher` template.)
- **C4.** envoy-bin spawn site: `crates/envoy-bin/src/main.rs:180` (`Scheduler::spawn(...)`) beside the 14.2 `OutlierManager::for_bootstrap(&cluster_mgr, token.clone())` at `:194`; the shared `CancellationToken` is `token` (`:91`), drained via `shutdown().await` on the runtime drain path. (Exact — the `RdsWatcher` is constructed here, after the listeners are built.)
- **C5.** The phase-20 RDS load path the reload re-invokes: `crates/envoy-config/src/rds.rs` (`parse_rds_file`); `load_dynamic_resources` in `crates/envoy-config/src/lib.rs`; the per-HCM `rds.*` registration (phase-20 `register_rds_stats` / `HCMStats`); the `RoutesConfigDump` entry in `crates/envoy-admin/src/endpoint.rs`. **Task 1's §6.2 grep re-verifies these exact symbols on the state-3 HEAD** (they are phase-20 code; this PLAN cites them from the phase-20 PLAN/SPEC, not re-grepped line-exact here — a Task-1 SPEC-correction item).
- **C6.** Fixture baseline: `tests/fixtures/0001`…`0033` (next is `0034`); the existing RDS fixture `tests/fixtures/0028-xds-file-based-rds` is the §5.2 idle-watcher regression witness. Harness dynamic-file rendering/mounting (`{{RDS_PATH}}`) in `tests/differential/src/lib.rs` (phase-20 Task 6). (Exact.)

---

## §6.1 split-gate decision — RE-EVALUATED at Task 1: SINGLE PHASE CONFIRMED (ADR-0067 stays UNFIRED)

Projected surface (SPEC §6.1): D1 ~120 (+120 tests) · D2 ~150 (+100) · D3 ~140 (+150) · D4 ~60 (+50) · D5 ~70 (+50) · D6 ~160 (+60) · D7 ~220 · D8 ~200 = **~1200–1600 LoC / 9 implementation tasks + a §6.2 task + a state-4 task**. **Re-evaluated at Task 1 (§6.2 verified): the refined surface is at or UNDER the projection — Task 9 (`watched_directory`) does NOT fire (no schema change), Task 4's unknown-cluster handling is the already-planned re-validate-and-reject (no extra request-time code), and Task 6 shrinks (`version_info` already absent — read-through-handle only). Below the ~1500-LoC / ~25-task gate → SINGLE PHASE CONFIRMED; ADR-0067 stays UNFIRED.**

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

### Task 1: §6.2 empirical verification on LINUX (run FIRST; fires ADR-0066 if divergent) — ✅ DONE (2026-06-16; ADR-0066 FIRED)

**✅ DONE.** Ran on Linux against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`). Findings locked in the "§6.2 VERIFIED" section above; reconciliation in **ADR-0066** (P2 atomic-rename-only + Task-9-N/A; P5(iii) unknown-cluster recorded divergence; P6 `version_info`-absent already-correct; §6.1 single-phase confirmed). Full probe transcript: PROGRESS Task 1. All 6 steps below completed.

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

### Task 4: The reload pipeline (re-parse → re-validate → atomic swap; stat-tick + warm-reject) — §6.2-LOCKED (P4/P5), UNBLOCKED (Task 1 done) — NEXT TASK

**Files:** `crates/envoy-http1/src/rds_watcher.rs` (the real `reload`); `crates/envoy-config/src/lib.rs`/`rds.rs` (expose a `reparse_and_select_route_config(path, name, &cluster_set) -> Result<RouteConfiguration, ConfigError>` reusing the phase-20 parser+validator).

**§6.2-LOCKED bad-reload taxonomy (ADR-0066 P5 — map error class → counter):** {file unreadable / IO error / malformed YAML / parse error → `update_attempt`+`update_failure`, KEEP last-good}; {`route_config_name` not present in the reloaded envelope → `update_attempt`+`update_rejected`, KEEP last-good}; {a reloaded route references an UNKNOWN cluster → `update_attempt`+`update_rejected`, KEEP last-good — the **recorded deliberate divergence** from Envoy's accept-and-503, because the request path `.expect()`s cluster existence at `crates/envoy-http1/src/hcm.rs:818`; envoy-rust re-validates route→cluster refs against the immutable live cluster set and warm-rejects}. Happy path (P4): `update_attempt`+`update_success`+`config_reload` each +1.

**CARRY-FORWARD review notes folded in here (from the Task-2 code-quality review, non-blocking):** **(a)** add a one-line note at `hcm.rs` `store_route_config` that the poison-recovery (`unwrap_or_else(|p| p.into_inner())`) is safe ONLY while the write critical section is a single `*guard = rc` Arc move — so this reload pipeline MUST do the reparse/revalidate **OUTSIDE the write lock**, then a single `store_route_config` (do NOT widen the write critical section; load-bearing for warm-reject correctness). **(b)** tidy the stale `resolve_route` inline comment ("we yield BOTH …") — a leftover from the pre-`ResolvedRoute` two-value design; it now yields one owned `ResolvedRoute`.

- [ ] **Step 1: Write the failing reload tests (happy + warm-reject).** Happy: a valid file change → `store_route_config` lands the new table + `update_attempt`/`update_success`/`config_reload` each +1 (P4 = `2/2/…/2` after one reload). Warm-reject: a malformed file → last-good kept + `update_failure` +1; a vanished `route_config_name` → last-good kept + `update_rejected` +1; an unknown-cluster route → last-good kept + `update_rejected` +1 (the recorded divergence). Drive via `tokio::time` + temp files in-process; rewrite the temp file via **atomic-rename** to mirror the harness (though in-process the mtime poll catches either).
- [ ] **Step 2: Run — expect FAIL** (reload is the Task-3 no-op stub).
- [ ] **Step 3: Implement `reload`.** Re-read file → `reparse_and_select_route_config` (reuse phase-20 `rds.rs` parse + the route→cluster validator against the immutable live cluster set) **OUTSIDE the write lock** (carry-forward (a)) → on `Ok`: a single `store_route_config(Arc::new(new))` + tick attempt/success/config_reload (config_dump version is handled by Task 6 reading through the handle — no separate bump needed) ; on `Err`: KEEP the handle + tick attempt + failure/rejected per the §6.2-LOCKED taxonomy above (the §5.5 warm-reject — **NOT** all-fatal). Apply carry-forwards (a) + (b).
- [ ] **Step 4: Run + clippy.** PASS; warm-reject paths assert the table is byte-unchanged after a bad reload.
- [ ] **Step 5: Commit** + PROGRESS commit.

### Task 5: Per-HCM `rds.*` counters tick per reload (thread the phase-20 stat handles to the watcher target) — ✅ FOLDED INTO Task 4 (the reload pipeline cannot tick counters without the handles)

**Files:** `crates/envoy-http1/src/hcm.rs` (expose the phase-20-registered `rds.*` `Arc<Counter>` handles); `crates/envoy-bin/src/main.rs` (pass them into the Task-3 target build); `rds_watcher.rs` (consume).

- [ ] **Step 1: Write the failing test** — the watcher target carries the SAME `Arc<Counter>` handles the HCM registered at construction (phase-20 `register_rds_stats`), and a reload increments THOSE registry counters (scrape-visible), not a private copy.
- [ ] **Step 2: Run — FAIL.** **Step 3: Thread the handles** from the HCM construction site into the target list build (the 06.x `Arc<Counter>`-shared-handle idiom). **Step 4: Run + clippy.** **Step 5: Commit** + PROGRESS.

### Task 6: `/config_dump` `RoutesConfigDump` reads through the swappable handle — §6.2-LOCKED (P6), UNBLOCKED (Task 1 done) — NARROWED

**Files:** `crates/envoy-admin/src/endpoint.rs` (`ConfigDumpEntry::Routes`).

**§6.2-LOCKED (P6 / ADR-0066):** NO `version_info` for file-RDS (already the phase-20 `RoutesConfigDump` shape — endpoint.rs already omits it); `last_updated` is already render-time `now()` (already changes per dump). **So Task 6 shrinks to ONE thing: render `route_config` THROUGH the swappable handle (`current_route_config()`) so a post-reload `/config_dump` reflects the NEW table** — no version field, no separate timestamp plumbing.

- [ ] **Step 1: Write the failing test** — after a reload (drive via Task-4 `store_route_config`), `/config_dump` `RoutesConfigDump.route_config` reflects the NEW route table; pre-reload it shows the initial table. **Step 2: Run — FAIL** (renderer reads a startup snapshot). **Step 3: Implement** — render through `current_route_config()` (Task 2); keep `last_updated` as-is (render-now); NO `version_info` (unchanged); keep emission conditional on `rds` (phase-20 conditionality; 0026/0027 untouched). **Step 4: Run + clippy.** **Step 5: Commit** + PROGRESS.

### Task 7: Harness — mid-test fixture-file rewrite (ATOMIC-RENAME) + settle-then-probe — §6.2-LOCKED (P1/P2), UNBLOCKED (Task 1 done)

**Files:** `crates/.../tests/differential/src/lib.rs` (a new probe-step type + the expectations schema extension).

**§6.2-LOCKED (P2 / ADR-0066):** the reload step MUST rewrite the mounted RDS file via **ATOMIC-RENAME** (render new per-side contents to a temp file ON THE SAME MOUNT, then `std::fs::rename`/`mv` over the watched path) — the ONLY operation that triggers BOTH proxies (Envoy's default file-watch ignores in-place truncate-rewrite). Settle via bounded wait-for-convergence (the 12.2 pattern), bounded by the ~50 ms Task-1 settle latency (with generous slack). **NOTE:** under Docker Desktop virtiofs (this host) the Envoy-side reload is NOT observable locally — fixture 0034's differential evidence is **native-Linux-CI-authoritative** (§5.7 / ADR-0066); the harness step is unit-tested in isolation locally, the full differential runs on CI.

- [ ] **Step 1: Write the failing harness-unit test** — a "reload step" writes per-side-rendered new contents to a temp file on the mount then atomic-renames over the SAME mounted path, then settles via bounded wait-for-convergence on a discriminating observable (the routed-to cluster, or `config_reload` advancing — NOT a fixed sleep). **Step 2: Run — FAIL.** **Step 3: Implement** the atomic-rename reload step + the expectations "reload" + post-reload probe/assert block (generalize the phase-20 `{{RDS_PATH}}` machinery). **Step 4: Run.** **Step 5: Commit** + PROGRESS.

### Task 8: Fixture `0034-xds-rds-hot-reload` + Docker-gated wrapper + in-process backstop — §6.2-LOCKED, UNBLOCKED (Task 1 done; fixture differential is NATIVE-Linux-CI-authoritative, backstop is the local complement)

**Files:** Create `tests/fixtures/0034-xds-rds-hot-reload/` (`envoy.yaml`/`envoy-rust.yaml`/initial+reload+malformed RDS templates/`expectations.yaml`/`README.md`), `tests/differential/tests/xds_rds_hot_reload.rs` (Docker-gated; **Linux-CI-authoritative — README notes macOS-local-unobservability**), `crates/envoy-bin/tests/xds_rds_hot_reload.rs` (the `tokio::time`-controlled backstop: happy reload + the 4 negative paths + in-flight isolation; the deterministic local complement to the Linux-only differential).

- [ ] **Step 1: Author the fixture** (two distinguishable `http1-echo-server` clusters per P3; the §1 three-phase pre→reload→post sequence + the bad-reload probe; assert P4 counter values + P6 config_dump). **Step 2: Author the backstop** (covers the paths the Linux-only fixture can't cleanly drive). **Step 3: Run the backstop locally** (`cargo test -p envoy-bin xds_rds_hot_reload`) — pre-build `tests/helpers/*` first (per `project_flaky_access_log_fixture_0012`). **Step 4: Verify the fixture on Linux CI** (the differential evidence). **Step 5: Commit** + PROGRESS.

### Task 9: ~~Conditional — `ConfigSource.watched_directory` field + fuzz seed~~ — ❌ DOES NOT FIRE (Task 1 P2 / ADR-0066)

**N/A — no work.** Task 1 P2 proved Envoy reloads on ATOMIC-RENAME with NO `watched_directory` needed (and `watched_directory` does not even rescue the in-place case). The Task-7 harness uses atomic-rename; envoy-rust's mtime poll detects it. **NO config-schema change, NO new fuzz seed.** Recorded "Task 9 N/A" in PROGRESS + ADR-0066. Skip to Task 10.

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
