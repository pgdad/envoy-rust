# Phase 27 (`27-xds-eds-hot-reload`) — REVIEW

> **Lifecycle state 5** (`BOOTSTRAP_PROMPT.md` §5 — verified, not reviewed →
> `superpowers:requesting-code-review` → REVIEW.md). This review covers the phase-27
> state-3 implementation arc (Tasks 2–8 code = deliverables D1–D8) + the Task-9 state-4
> verification gate. **Verdict: APPROVED.**
>
> **Review model:** each of the 8 state-3 tasks was ALREADY individually two-stage-reviewed
> (spec-compliance THEN code-quality) by a fresh `superpowers:code-reviewer` subagent during
> execution. This state-5 review is therefore the **holistic phase review** — a single fresh
> `superpowers:code-reviewer` subagent given crafted context (not session history), tasked
> with the six cross-cutting system-level seams that per-task reviews cannot fully see, plus
> triage of the per-task Minors already logged in PROGRESS.
>
> **Review range:** `f43d1a6` (state-2 PLAN-write base) … `acac6d4` (Task-8 BEHAVIOR_CONTRACT)
> — the full phase-27 production + test diff (+3682 / −179, 23 files). The state-4 STATE-advance
> commit `334c093` is docs-only and out of code scope.
> **Differential evidence:** the AUTHORITATIVE native-Linux CI run **`27818702552`** @ `acac6d4`
> (both jobs GREEN: fixture `0035` EDS hot-reload `xds_eds_hot_reload_fixture ... ok`, all 35
> Docker-gated fixtures `0001`–`0035`, h2spec ≥95%, `parse_bootstrap` + `jwt_parse` fuzz;
> local fmt / clippy `--all-targets --all-features` / builds / `deny check` clean; 1182
> non-Docker tests pass). The differential is native-Linux-CI-authoritative (this dev host is
> Docker Desktop / virtiofs; `differential::admin_config_dump_server_info` fails locally — a
> documented host artifact, NOT a regression — per memories `host-docker-desktop-virtiofs-no-inotify`
> + `envoy-rust-state4-ci-first-execution`).

## Verdict: **APPROVED**

**APPROVED — 0 Critical / 0 Important / 3 Minor (non-blocking).** All six cross-cutting
focus areas verified PASS under independent reviewer evidence; all per-task Minors already
logged during state-3 were re-triaged and CONFIRMED non-blocking. No reviewer rated any
finding above Minor. This is the **twelfth consecutive clean state-5** (after 17, 18, 19,
20, 21, 22, 23, 24, 25.1, 25.2, 26).

Per `BOOTSTRAP_PROMPT.md` §5.2 the re-enter-state-3 trigger is a Critical or Important
finding; there are none. The phase lands APPROVED with M-track follow-ups (the established
pattern — REVIEW Minors weighed at the next phase's planning). The state-6 deterministic
close-out (commit `phase 27: file-based EDS endpoint hot-reload [ADR-0067, ADR-0068]`; flip
ROADMAP row `27` `in-progress → done`; STATE → AWAITING NEXT PLANNING; ADR-0035 narrative
relocation; push) is the NEXT session. This approved `REVIEW.md` satisfies §7.5 gate (f);
(a)–(e) are GREEN at CI `27818702552`.

## Scope reviewed (all of D1–D8)

The D1 swappable endpoint handle (`Cluster.endpoints: RwLock<Arc<Vec<SocketAddr>>>` +
read-once `current_endpoints()` / `store_endpoints()` + the `pick()` LB read path); the D2
domain-free `XdsFileWatcher` generalized out of the phase-26 `RdsWatcher` (+ the `RdsWatcher`
reseat); the D3+D4 EDS reload pipeline (`eds_reload.rs`: reparse/validate-outside-the-lock →
single-move swap; the V4 bad-reload taxonomy; `build_eds_watch_targets`; the 5 `eds.*`
counters) + the envoy-bin watcher wiring; the D5 `/config_dump` `EndpointsConfigDump`
read-through-handle; the D6 `Http1EdsReload` differential driver + second distinguishable
backend; the D7 fixture `0035` + the in-process backstop; the D8 BEHAVIOR_CONTRACT §2.1/§2.2
EDS hot-reload extension.

## Cross-cutting focus areas — all PASS

1. **D1 endpoint-handle correctness — PASS.** `Cluster::pick()` (`crates/envoy-cluster/src/cluster.rs:322-373`)
   snapshots the `Arc<Vec>` ONCE at entry (`:327`) and returns `None` on empty *before* any
   `% total` (`:328-333`), so a SHRINKING endpoint set (2→1) can neither index out of bounds
   nor divide by zero on either the fast path (`:338`) or the eligible-index path (`:372`).
   Witnessed by both the in-crate `endpoint_handle_shrinking_set_keeps_cursor_in_bounds`
   unit test and the `cursor_bounds_on_shrinking_endpoint_set` backstop. Poison recovery
   (`.unwrap_or_else(|p| p.into_inner())`) is consistent across BOTH read (`:390-396`) and
   write (`:407-419`) sites — a reader never inherits a writer-side panic. The hot-path
   RwLock read is the SPEC §5.1-accepted tradeoff (arc-swap documented as the profiling
   fallback), NOT an unflagged regression.
2. **D2 XdsFileWatcher generalization — PASS.** `xds_watch.rs` is genuinely domain-free (a
   boxed `FnMut` closure carries all domain knowledge); the `RdsWatcher` reload pipeline
   (`crates/envoy-http1/src/rds_watcher.rs:178-227`) is logically byte-equivalent after being
   reseated on the generic core (RDS regression witness = phase-26 fixture `0034`, green at
   CI). NO crate cycle: `envoy-cluster` gained no `envoy-http1` dependency; `Cargo.lock` shows
   only dev-deps `filetime` + `tempfile` added to `envoy-cluster`.
3. **V4 bad-reload taxonomy fidelity — PASS.** `eds_reload::reload()`
   (`crates/envoy-cluster/src/eds_reload.rs:97-176`) does all IO/parse/select/validate as
   pure work OUTSIDE any lock; the only success/apply-empty lock touch is the single
   `store_endpoints(Arc::new(candidate))` at `:173` — no TOCTOU between validate and swap, no
   write-lock across IO. The five dispatch arms increment exactly the right counter
   (IO/parse → `update_failure`+keep; wrong-name/bad-IP → `update_rejected`+keep;
   empty-envelope → `update_empty`+keep; apply-empty → `update_success` + subsequent 503), and
   the apply-empty 503 body is the correct 19-byte `"no healthy upstream"`. Counter tuples
   cross-verified against both unit tests and the backstop.
4. **Encapsulation boundary (C-1/C-2) — PASS.** Writes only via in-crate `eds_reload`; reads
   via `pub ClusterHandle::current_endpoints()`; `ClusterHandle::store_endpoints` is
   `#[doc(hidden)] pub` (test-only reach); `into_inner` is `pub(crate)`. No wider write
   surface leaked into the public API.
5. **EDS + HC/OD no-watcher safety — PASS (closed by construction).** `build_eds_watch_targets`
   (`eds_reload.rs:194-236`) emits a target ONLY when `endpoint_health.is_none() &&
   outlier_detection.is_none()` (`:200-202`), and `store_endpoints` has no caller outside the
   EDS pipeline + tests — so a swap can never desync an HC/OD index-aligned array.
6. **config_dump byte-parity — PASS.** `EndpointsConfigDump` (`crates/envoy-admin/src/endpoint.rs`)
   reconstructs `LocalityLbEndpoints` from the live endpoint set through the existing phase-21
   serializer, so fixture `0029`'s idle (no-reload) witness stays green (CI `27818702552`).

## Findings — Minor (3; non-blocking → phase-28 carry-forwards)

- **M27-1 — `Cluster::store_endpoints` (`crates/envoy-cluster/src/cluster.rs:407`) is `pub`,
  not `pub(crate)` as the PLAN projected.** It is effectively unreachable cross-crate (no
  public API hands out an `&Cluster`/`Arc<Cluster>`; `into_inner` is `pub(crate)`), so this is
  not an actual write-surface leak. Tightening to `pub(crate)` would make the encapsulation
  match the PLAN-of-record and remove the latent reachability if a future `pub` `Cluster`
  accessor is ever added. Pure hardening; resolve when next touched.
- **M27-2 — no `debug_assert!` coupling the `pick()` slow-path array indices to the snapshot
  length (`cluster.rs:344-355`).** The slow path indexes `health[i]`/`ejection[i]` for
  `i in 0..eps.len()`, relying on the plainness filter (M5 above) to guarantee alignment. This
  holds today by construction; a `debug_assert_eq!(eps.len(), health.len())` (when `Some`)
  would turn a future regression (someone wiring a watcher onto an HC/OD cluster) into a loud
  test failure rather than a production index-panic. Defense-in-depth.
- **M27-3 — in-flight-isolation backstop uses a 400ms wall-clock sleep**
  (`crates/envoy-bin/tests/xds_eds_hot_reload.rs:761`) to ensure the request picks the old
  endpoint pre-reload. It is well-cushioned by a 2s slow-backend delay (very low flake risk),
  but it is the sole non-bounded timing assumption in the backstop (all other waits are
  bounded poll loops). Acceptable as-is; note for the deferred test-support hardening.

## Triage of per-task Minors already logged during state-3 (all CONFIRMED non-blocking)

- **Task 5 (config_dump) — absent-cluster silent fallback / `.contains()` presence checks /
  comment.** CONFIRM non-blocking. The fallback is reached only on `ClusterManager::empty()`
  test paths; in production every EDS cluster has a live handle, and the diff now comments the
  fallback. The phase-26 RouteSnapshot warns-on-missing because RDS can legitimately reference
  a not-yet-loaded route; the EDS case cannot, so silent fallback is defensible here.
- **Task 7 (tests) — bad-reload/counter/config_dump proofs in the backstop not the differential
  fixture; 7th copy of `reserve_port`/`wait_ready`/atomic-rename; cursor-bounds test shares
  bodies.** CONFIRM non-blocking — a faithful phase-26 fixture-`0034` precedent-match given the
  single-reload schema + differential-response limits, NOT a coverage gap. **Observation:** the
  reviewer reproduced the documented port-TOCTOU flake ONCE under full parallel `--test` load
  (`cursor_bounds_on_shrinking_endpoint_set` failed, then passed 8/8 on isolated re-runs + 2
  full re-runs) — the known `host-docker-desktop-virtiofs-no-inotify` / port-bind-then-drop
  class, native-CI-authoritative, NOT a behavior bug. It reinforces (does not block) the
  deferred shared-test-support-crate extraction (a `SO_REUSEADDR`-or-retry port helper would
  remove it).
- **Task 6 (harness) — RDS+EDS dispatch arm ~70-line skeleton duplication.** CONFIRM
  non-blocking; the EDS arm is in fact *stronger* than the RDS arm — it folds the M26-2
  spurious-convergence guard (`eds_reload_discriminator_is_load_bearing`,
  `tests/differential/src/lib.rs`) that the RDS arm omitted. A shared
  `run_file_reload_differential` helper on a THIRD reload driver remains the right trigger.

## Recommendations

- Land M27-1 (tighten `store_endpoints` to `pub(crate)`) and M27-2 (slow-path
  `debug_assert_eq!` length-coupling) opportunistically in a future EDS-touching phase —
  neither blocks merge.
- When the THIRD file-reload driver (CDS/LDS/SDS hot-reload) lands, that is the trigger to
  (a) extract the shared `reserve_port`/`wait_ready`/atomic-rename test-support crate (also
  centralizing a flake-resistant port helper — see Task-7 observation + M27-3), and (b) factor
  the RDS/EDS reload-dispatch skeleton (M26-track). Both deferrals are now justified but
  accruing.

## Disposition

**APPROVED — proceed to state-6 close-out.** The phase is system-level correct across all six
cross-cutting seams — read-once LB discipline with safe cursor-bounds and empty-set handling,
leak-free reload lock discipline, no crate cycle, byte-equivalent RDS refactor, exact V4
taxonomy, structurally-closed HC/OD index-aligned-array desync, and preserved config_dump
parity. The only findings are three Minor hardening nits (M27-1..3) plus a confirmed
pre-existing port-TOCTOU test flake (passes on re-run; CI-authoritative), none of which gate
the merge. The §7.5 gate (a)–(e) is GREEN at CI `27818702552`; this approved `REVIEW.md`
satisfies (f). The Minors carry to phase-28 planning per the established Minor-carry-forward
pattern.
</content>
</invoke>
