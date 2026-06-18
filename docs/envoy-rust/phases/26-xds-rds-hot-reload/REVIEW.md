# Phase 26 (`26-xds-rds-hot-reload`) — REVIEW

> State-5 code review (`superpowers:requesting-code-review`). Two-stage per project
> discipline: **(1) spec-compliance** THEN **(2) code-quality**, each a fresh
> `superpowers:code-reviewer` subagent given crafted context (not session history).
> Review range: `c8b2ffc` (state-2 PLAN-write) … `73712df` (Task-10 STATE advance) —
> the full phase-26 production + test diff (~2990 insertions).
> Differential evidence: the AUTHORITATIVE native-Linux CI run **`27708943522`**
> (all 34 fixtures incl. `0034` RDS hot-reload + h2spec + deny + fuzz, GREEN).

## Verdict: **APPROVED**

- **Stage 1 (spec-compliance): APPROVE** — 0 Critical / 0 Important / 3 Minor.
- **Stage 2 (code-quality): APPROVE-WITH-MINORS** — 0 Critical / 0 Important / 5 Minor.
- **Combined: 0 Critical / 0 Important / 8 Minor (non-blocking).** No issue requires a
  fix before close-out; the 8 Minors are recorded below as **phase-27 carry-forwards**
  (the established pattern — REVIEW Minors weighed at the next phase's planning).

Per `BOOTSTRAP_PROMPT.md` §5.2, an approved REVIEW with no Critical/Important does NOT
re-enter state-3. Phase 26 proceeds to state-6 close-out.

## Scope reviewed (all 10 PLAN tasks; Task 9 N/A)

The swappable route-table handle (`RwLock<Arc<RouteConfiguration>>` + read-once
`current_route_config()` / `store_route_config()` + the owned-snapshot `ResolvedRoute`
read path, H1+H2); the `RdsWatcher` poll-based mtime primitive; the reload pipeline
(reparse-outside-the-lock → single-move swap; warm-reject taxonomy); the `rds.*` counter
threading; the `/config_dump` read-through-handle; the `Http1RdsReload` differential
driver + fixture `0034` + the in-process backstop; and the Task-10 gate fixes (fmt;
fixture-0034 admin-port convention; testcontainers `CmdWaitFor::exit_code(0)` reload-exec).

## Strengths (both stages)

- **All 9 implemented deliverables map cleanly to the diff and are real, non-stubbed**
  (no §6.3 anti-pattern). The Task-3 no-op reload stub was genuinely replaced by the
  Task-4 pipeline.
- **The concurrency core is correct.** `current_route_config()` snapshots the inner `Arc`
  (unambiguous `Arc::clone` via guard auto-deref) and drops the read guard at
  end-of-statement — a true read-once snapshot; `store_route_config()` is a single
  `*guard = rc` move; the reload does all IO/parse/validate OUTSIDE the lock and takes the
  write lock only for the swap; no lock is held across `.await`. Corroborated by the
  `route_table_handle_swap_is_read_once` unit test AND the end-to-end in-flight-isolation
  backstop. Poison handling (`unwrap_or_else(|p| p.into_inner())`) is defensible and
  documented (a single Arc move cannot tear).
- **`ResolvedRoute`** owns the snapshot and stores vh/route indices computed against that
  exact snapshot → `route()` is always in-bounds and lifetime-safe; both call sites
  correct.
- **The error classifier is provably exhaustive** — `reparse_and_select_route_config`
  constructs only the four `Rds*`/`UnknownCluster` variants, so the four explicit arms
  cover every reachable case and the `unreachable!()` final arm is genuinely unreachable
  (with a correct maintainer note).
- **Counter taxonomy is exactly per ADR-0066 P4/P5** (`1/1/0/0/1`→`2/2/0/0/2`; the
  warm-reject buckets {IO/parse→`update_failure`+keep; name-absent &
  unknown-cluster→`update_rejected`+keep}), asserted by both unit tests and the backstop.
- **Both intended divergences are honestly documented per doctrine D-3.3** — the
  unknown-cluster warm-reject (grounded in the request path's `.expect()` at `hcm.rs:821`)
  across the doc comment + ADR-0066 + PLAN + BEHAVIOR_CONTRACT; and the Task-4 carry-forward
  (re-validate cluster-existence only, NOT the ADR-0028 H1×H2 gate) without overclaiming.
- **Wiring is correct** — the one inner `Arc<HCMConfig>` is shared by the watch target, the
  H2 wrapper's `.inner`, and the admin `live_route_configs`, so all observe the single
  swappable cell; `rds_counter_base` is the single name source (no drift); the watcher
  drains on both clean and error-exit paths.
- **Task 9 correctly N/A** — no `watched_directory` schema field added.
- **The Task-10 fixes are correct and well-documented** — the `CmdWaitFor::exit_code(0)`
  blocking-exec fix (accurate root-cause comment on the `ExitCode: null` race), the
  admin-port-0 convention fix, the fmt fix.

## Findings — Minor (8; non-blocking → phase-27 carry-forwards)

- **M26-1 [substantive] — H1 request path snapshots the route table TWICE.** `resolve_route`
  (`crates/envoy-http1/src/hcm.rs:691`) takes one `current_route_config()` snapshot for
  per-route filter-config threading; `build_response` (`:766` → `:1346`) takes an
  INDEPENDENT snapshot for the routing decision. A reload landing between the two reads
  would apply OLD-table per-route filter config with NEW-table routing — a narrow gap vs
  the SPEC §5.4 "load the current route-table Arc ONCE at request entry" wording. Impact is
  bounded (one ~1s-poll window; benign — no panic; routing itself stays self-consistent;
  untested because the in-flight backstop exercises only the routing path). **NOT an
  Envoy-equivalence divergence** (the differential is green) — an internal-invariant
  wording-vs-impl nuance. Resolve when next touched by EITHER threading one snapshot from
  `serve_connection` through `build_response`, OR narrowing the §5.4 wording in
  BEHAVIOR_CONTRACT §2.2 to "the routing decision is read-once" (D-3.3: fix impl or update
  the contract — recorded here so it is not silent). The H2 path already resolves once.
- **M26-2 [substantive] — `wait_for_reload_convergence` can spuriously "converge" if the
  discriminator has neither `expected_status` nor `expected_body`** (`tests/differential/src/lib.rs:1257-1268`).
  Both fields are `Option`; both-`None` makes `status_ok && body_ok` true on the first poll,
  returning `Ok` without confirming the new table is live. Fixture 0034 sets both (safe
  today), but the schema permits a non-discriminating discriminator. Suggested: `bail!` in
  the `Http1RdsReload` arm if the discriminator has neither field, or document the invariant.
  CI-path robustness hardening.
- **M26-3 — mtime-only change detection** (`crates/envoy-http1/src/rds_watcher.rs:177-182`):
  two reloads within one (possibly coarse, ≥1s) mtime tick would miss the second; the
  watcher relies solely on mtime inequality, not size/content. Never bites the
  single-reload test contract, and the ~1s poll makes sub-second double-edits unlikely
  operationally. Worth a one-line caveat on `read_mtime` (or a secondary file-length
  compare) since this is the project's first live-mutation primitive.
- **M26-4 — fixture 0034 uses `direct_response` bodies (`rds-v1`→`rds-v2`) rather than the
  SPEC §1 two-cluster routing-flip.** A deliberate, thoroughly-documented simplification
  (the harness spawns one backend + the driver converges on status/body, so two clusters
  can't be distinguished in a differential response; the cluster/counter/config_dump proofs
  live in the backstop per SPEC §6.3/D8). A reasonable deviation, honestly recorded — not a
  defect.
- **M26-5 — stale Task-3-era comments** on `RdsWatcher::spawn` (`rds_watcher.rs:114-120`)
  and the `main.rs` spawn site still say "skeleton registers no counters / `reload` is a
  no-op stub" after Tasks 4/5 landed the real pipeline + counters. Cosmetic; refresh.
- **M26-6 — duplicated atomic-rename helper across test crates** (`atomic_rename_over` in
  `tests/differential/src/lib.rs:1127` vs `atomic_rename_rds` in
  `crates/envoy-bin/tests/xds_rds_hot_reload.rs:399`), with a subtle divergence
  (`.push(".reload-tmp")` preserving `.yaml` + cleanup vs `with_extension("reload-tmp")`
  replacing it + no cleanup). Consistent with the already-tracked deferred
  shared-test-support-crate item; noted so the divergence is intentional, not silent drift.
- **M26-7 — redundant accessor call** in the config_dump miss path
  (`crates/envoy-admin/src/endpoint.rs` calls `handler.live_route_configs()` twice). Cheap
  slice borrows; cosmetic (bind once if touched).
- **M26-8 — `RouteSnapshot::as_ref`** is an inherent method shadowing the conventional
  `AsRef::as_ref` name (`crates/envoy-admin/src/endpoint.rs`); a name like `route_config()`
  would be marginally clearer. Trivial.

## Disposition

**APPROVED — proceed to state-6 close-out.** None of the 8 Minors block the phase; M26-1
and M26-2 are the only substantive items and both are bounded, untested-because-narrow, and
NOT Envoy-equivalence divergences (the differential gate is green). They are carried to
phase-27 planning per the established Minor-carry-forward pattern. The phase-done §7.5 gate
(a)–(e) is GREEN at CI `27708943522`; this approved `REVIEW.md` satisfies (f).
