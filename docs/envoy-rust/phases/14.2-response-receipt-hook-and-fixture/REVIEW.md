# Phase 14.2 (`14.2-response-receipt-hook-and-fixture`) — REVIEW

- **Lifecycle state:** state 5 (verified → reviewed). This document is the state-5 output.
- **Skill:** `superpowers:requesting-code-review` (per `BOOTSTRAP_PROMPT.md` §5 state 5 + `SKILL_ROUTING.md`).
- **Review range:** `c0816d77..64f4f576` — the 13 state-3 task/fixup commits + the state-4
  verification commit, atop the state-2 PLAN-write `c0816d77` (excluded as the base). The chain:
  `34cb7bf5` (T1 M4 serialization) → `260fd440` (T2 D4-H1 + A-M2) → `085de46b` (T3 D4-H2) →
  `05139c89` (PROGRESS T1–4) → `2b97b744` (T4 D7 sweeper) → `bd35f92c` (T5 envoy-bin wiring) →
  `0c7708bb` (T6 keep-alive driver ext) → `ff02056d`+`014c8b43`+`8d06d6fb` (T7 fixture 0022) →
  `4e3cc2e0`+`1d6b3a05`+`4cd25158`+`1adab6fd` (T8 backstop + panic_threshold bugfix) →
  `91e8dfad` (T9 D9 docs + M8) → `64f4f576` (T10 state-4 verification + STATE advance).
- **Pre-review HEAD:** `64f4f576` (== `origin/main`; CI run `26692551259` `completed/success`).
- **Method:** 6 read-only code-review subagents, one per concern-cluster (parallel, small batches,
  per `feedback_serial_subagent_dispatch` — read-only reviewers may parallelize), each reading the
  actual on-disk diff (`git diff` + `Read` + `Grep`) and returning per-cluster verdicts; the
  controller synthesized this REVIEW, performed an independent out-of-band verification step
  (isolated-crate build) that surfaced one finding the diff-readers missed, and resolved all
  Critical/Important findings in-review (all were mechanically fixable).
- **Verdict: APPROVED** (Important findings resolved in-review + re-verified; Minor follow-ups
  carried). Mirrors the 14.1 state-5 `Approved with M-track follow-ups` shape (`e0ba8d01`).

---

## 1. The two load-bearing review items (the named owners this session MUST verify)

### 1.1 M4 discharge — per-endpoint serialization lock — **VERIFIED CORRECT**

The 14.1 `EndpointEjection` used `Relaxed` atomics on a `&self` receiver, race-free ONLY under
single-writer-per-endpoint. At 14.2 the writers became genuinely concurrent (one per in-flight
request via the D4 hook + the D7 sweeper). Task 1 discharged this with a per-endpoint
`ejected_at: std::sync::Mutex<Option<Instant>>`.

The reviewer exhaustively grepped every production mutation of the ejection atomics and confirmed
the **single-serialized-writer-per-endpoint premise holds**:

- The only three production mutation entry points are `Cluster::record_response`
  (`cluster.rs:292`, under the guard acquired at `:291`), `EndpointEjection::eject`
  (`cluster.rs:313`, under the held guard), and `OutlierEjectionSweeper::sweep_once`'s
  `try_un_eject` (`outlier.rs:78`, under the guard acquired at `:72`). `ClusterHandle::record_response`
  (`cluster.rs:352`) is pure delegation into the guarded path.
- `eject` / `try_un_eject` (`ejection.rs:198-232`) touch only the atomics + stats and **never**
  reference `ejected_at` — so the externally-held guard cannot self-deadlock (lock-in #5 honored).
- The timestamp is stamped (`cluster.rs:317`) / cleared (`outlier.rs:81`) by the *caller* under the
  already-held guard.
- No `std::sync::Mutex` guard is held across an `.await`: `sweep_once` is a sync `fn`; the only
  `.await`s (`tick.tick()` / `cancel.cancelled()`) are outside the guard.
- The read side stays **lock-free**: `is_ejected()` (`ejection.rs:125-127`) is a single `Relaxed`
  `AtomicBool` load; `pick()` never takes the mutex.

The M5/M6 fold-ins assert real behavior, not tautologies: the strengthened tie test exercises live
detector selection (5xx wins → `enforced_consecutive_5xx==1 && enforced_consecutive_gateway_failure==0`),
the `max_ejection_percent==0` edge test exercises the `cap_count==0 ⇒ overflow` path.

**Cluster verdict: CLEAN.** The M4 carryforward is correctly discharged.

### 1.2 Task-8 `panic_threshold` root-cause bugfix (`1adab6fd`) — **VERIFIED CORRECT + GUARDED**

The in-process backstop surfaced a genuine 12.1-landed product bug: `from_bootstrap` parsed
`common_lb_config.healthy_panic_threshold` only inside the `if cfg.health_checks.first()` branch,
defaulting outlier-detection-only clusters to `panic_threshold = 50.0`, so a freshly-ejected sole
endpoint (0% eligible < 50%) was re-admitted by panic-routing and `pick()` never returned `None`.

The reviewer confirmed:

- **The fix is correct.** The `panic_threshold` parse is hoisted to `cluster.rs:719-724`, run
  unconditionally before either health-check or OD state is built; the parse chain
  (`healthy_panic_threshold → value`, `unwrap_or(50.0)`) is **byte-identical** to the deleted
  in-branch parse — only its scope changed.
- **Differential-inert for the 12.x active-HC clusters.** Because the hoisted expression is
  character-for-character the old in-branch one, active-HC clusters resolve to exactly the same
  `panic_threshold` as pre-fix; there is no double-parse and no precedence change. The 21
  pre-existing fixtures are unaffected.
- **`value: 0` honored.** `pick()` uses strictly-below (`eligible_percent < panic_threshold`), so
  `0.0 < 0.0` is false → panic disabled → a 0-eligible cluster falls through to `None`. Matches
  Envoy's "0 disables panic" semantics. Units are consistent (both sides 0–100; validator rejects
  out-of-range / NaN at parse time).
- **The regression test genuinely guards the fix.**
  `from_bootstrap_honors_panic_threshold_zero_without_health_checks` (`cluster.rs:2438-2480`) drives
  the REAL `envoy_config::parse_bootstrap` → `from_bootstrap` path (not a hand-built `Cluster`
  literal), ejects the sole endpoint via `record_response(ep, 500)`, and asserts
  `pick_endpoint().is_none()` — the exact pre-fix failure mode.

**Cluster verdict: MINOR-ONLY** (one stale field doc-comment, fixed in-review — see §4).

---

## 2. Per-cluster verdicts

| Cluster | Surface | Verdict |
|---|---|---|
| A — M4 serialization | `ejection.rs`, `cluster.rs::record_response`, `outlier.rs::sweep_once` | **CLEAN** |
| B — `panic_threshold` bugfix | `cluster.rs::from_bootstrap` + regression test | **MINOR-ONLY** |
| C — D4 hooks (H1+H2) + A-M2 | `envoy-http1/src/hcm.rs`, `envoy-http2/src/hcm.rs`, `pool.rs:322` | **CLEAN** |
| D — D7 sweeper + manager + wiring | `outlier.rs`, `lib.rs`, `cluster.rs` (OD fields), `main.rs`, `Cargo.toml` | **MINOR-ONLY** |
| E — fixture 0022 + driver + backstop | `tests/fixtures/0022-*`, `tests/differential/src/lib.rs`, backstop | **MINOR-ONLY** |
| F — docs / M8 / PROGRESS / STATE | `BEHAVIOR_CONTRACT.md`, `14.1 SPEC`, `PROGRESS.md`, `STATE.md` | **HAS-IMPORTANT** (resolved in-review) |

Highlights from the CLEAN/MINOR clusters:

- **C (D4 hooks):** the H1 single-site call (`hcm.rs:692`) funnels all four endpoint-attributed
  arms (proxied / send-fail-502 / connect-fail-502 / pool-overflow-503) through one
  `record_response(endpoint, outgoing.status)` after the `upstream_rq_*` increments and before the
  downstream write; the no-healthy `else` arm is correctly excluded. The H2 two sites
  (`hcm.rs:437` success, `hcm.rs:406` connect-fail-502) are mutually exclusive via the `Err`-arm
  `return` — **exact-once coverage, no gap, no double-count**. A-M2 comment fix verified accurate.
- **D (sweeper):** cancellation discipline is verbatim-faithful to the three sibling primitives
  (`select!` with `cancel.cancelled()` first, `MissedTickBehavior::Skip`, `>=1ms` clamp,
  cancel-then-join); drains cleanly on BOTH the clean-exit and error-exit paths in `main.rs:481`
  (awaited before `first_err` is returned); spawns exactly one sweeper per OD-configured cluster
  and **zero** for non-OD clusters (inertness gate (b)). Sweeper tests use real wall-clock time
  (justified deviation from the PLAN's `start_paused` sketch — `std::time::Instant` does not track
  tokio's paused timer) with generous polling budgets — not flaky-by-construction.
- **E (fixture):** `envoy.yaml` ↔ `envoy-rust.yaml` differ only in documented dimensions (bind
  addr, node.id, `generate_request_id`); `expectations.yaml` matches SPEC §6.2 item-6 exactly
  (4-request `500,500,500,503`, byte counts 13/19, header presence/absence, the 5 exact ejection
  counters); the keep-alive driver extension is additive + backward-compatible (all 3 new fields
  `#[serde(default)]`; status-reader delegates with unchanged framing); the in-process backstop
  exercises both convergence directions with poll-until-converged (not bare sleeps) + asserts the
  5-standard-header presence on the synth-503.

---

## 3. Findings ledger

### Critical
None.

### Important (both resolved in-review — see §4)

- **I-1 — `envoy-cluster` is not self-contained: missing `tokio` `time` feature (build defect).**
  `crates/envoy-cluster/Cargo.toml:16` declared `tokio = { features = ["net", "rt", "macros"] }`,
  but `outlier.rs` (library code added at Task 4) uses `tokio::time::interval` +
  `MissedTickBehavior`, which require the `time` feature. The feature was present only in the
  **dev-dependencies** (`Cargo.toml:21`). `cargo build --workspace` and CI were green because Cargo
  **feature unification** pulls `time` in from sibling crates — but `cargo build -p envoy-cluster`
  in isolation **failed** with `error[E0433]: cannot find time in tokio` at `outlier.rs:44-45`.
  This is a latent fragility (the crate depends on a sibling to supply a feature its own lib code
  needs) and a real breakage of a legitimate invocation. Not Critical: the shipped workspace
  artifact builds, and the §7.5 gate (`cargo build --workspace --all-targets`) by definition uses
  unification. Surfaced by the controller's independent isolated-crate build, NOT by the diff
  readers (who correctly read the unified build as green). **Fixed:** added `"time"` to the regular
  `tokio` feature list.

- **I-2 — M8 reconciliation was half-closed + PROGRESS made a false on-disk claim.** The Task-9
  commit `91e8dfad` edited only `BEHAVIOR_CONTRACT.md`; the 14.1 SPEC (`§2.1` line 50, `§5.5`
  line 182) still said "14", while `PROGRESS.md:346-347` claimed the "14"→"13" correction landed in
  "BOTH" files (and cited the wrong section, §2.2 — the count actually lives in §2.1/§5.5). This
  left the count MORE inconsistent than before and is exactly the on-disk-state-accuracy defect
  (D-3.4) the M8 carryforward existed to close. The reviewer independently re-tallied the deferred
  set and confirmed **13 is the correct count** (5 `_detected_`/`_enforced_` detector pairs = 10 +
  3 legacy aliases = 13; the "14" arose from double-counting `ejections_consecutive_5xx`, a legacy
  alias of `ejections_enforced_consecutive_5xx`). **Fixed:** edited the 14.1 SPEC §2.1 + §5.5 to
  "13" (and corrected the adjacent false claim that fixture 0022 uses `allowlist_envoy_only` — the
  keep-alive driver has no such key); corrected the PROGRESS section reference + completed the
  honest narrative. Also surfaced + fixed a surviving hallucinated SHA `9a228d44` →
  `8d06d6fb` in `PROGRESS.md:247` (the Task-7 fixup-2 commit).

### Minor (resolved in-review)

- **M-r1 — stale `panic_threshold` field doc-comment** (`cluster.rs:119-120`): said "Read by
  `pick()` only when `endpoint_health` is `Some`" — false after the I-1-adjacent bugfix (now read
  whenever any eligibility filter is configured). **Fixed.**
- **M-r2 — self-contradicting fixture README** (`0022-*/README.md`): the
  "`allowlist_envoy_only` provenance" section claimed the deferred names are "listed under
  `allowlist_envoy_only` in `expectations.yaml`" — but that key was deliberately dropped (it does
  not exist on `Driver::Http1KeepAlive`). **Fixed** (rewritten to match `expectations.yaml`'s own
  correct prose + the BEHAVIOR_CONTRACT catalogue).

### Minor (carried forward — non-gating, no named owner)

- **M-c1 — `tokio-util` omits the `["rt"]` feature** the sibling crates pin
  (`envoy-cluster/Cargo.toml:17`). Benign: only `CancellationToken` is used, which does not need
  `rt`; builds + tests pass. Leaner, not broken. A future-reader-consistency nit only.
- **M-c2 — lock-poisoning via `.lock().unwrap()`** in the M4 critical sections. Non-impactful: the
  guarded regions hold only infallible atomic ops + `Instant::now()`, none of which panic.
  `unwrap_or_else(|e| e.into_inner())` would harden it but is gold-plating at this scope.
- **M-c3 — residual "14" deferred-count references in FROZEN / ratified records:** `DECISIONS.md`
  ADR-0041 (`:868`, `:875` — **append-only, invariant 4: must NOT edit landed ADRs**), the
  `STATE.md` historical §6.2 enumeration block (`:1189`), the closed `14.1 PLAN.md`
  (`:2272`, `:2308`), and the active-phase **ratified** `14.2 SPEC.md` (`:136`). Per the §6.3
  corrections-in-PROGRESS cadence (ratified SPECs are NOT edited; corrections live in PROGRESS/this
  REVIEW) and the append-only ADR/history doctrine, these are left as preserved-as-was records. The
  **canonical contract** (`BEHAVIOR_CONTRACT.md`), the **M8-named target** (`14.1 SPEC §2.1/§5.5`),
  and the **live fixture README** are now authoritative and reconciled to 13. The 14.2 SPEC §136
  "14" + its `allowlist_envoy_only` claim are corrected here in REVIEW (not in the ratified SPEC).
- **M1 / M2 / M3 / M7 / M9** (14.1 REVIEW) + the inherited multi-phase Minor inventory + the
  §6.9 per-class `upstream_rq_{2,3,4}xx` extension (observability, deferred) + **ADR-0028**
  (H1-listener × H2-cluster dispatch deferral; owner = a follow-up foundations-pivot phase) — all
  carry forward unchanged; none engaged by 14.2's surface.

---

## 4. In-review fixups (landed in this state-5 commit)

All Critical/Important findings were mechanically fixable doc/manifest-hygiene corrections with no
behavior change and no TDD surface (the manifest fix adds a feature the workspace already linked via
unification; the rest are documentation). Per `feedback_serial_subagent_dispatch` ("prefer tiny
mechanical edits directly") + `feedback_pick_recommendation`, they were resolved within this session
rather than deferred to a fresh state-3 cycle, and each was re-verified.

| File | Change | Finding |
|---|---|---|
| `crates/envoy-cluster/Cargo.toml:16` | add `"time"` to the regular `tokio` features | I-1 |
| `crates/envoy-cluster/src/cluster.rs:119-122` | correct the stale `panic_threshold` field doc-comment | M-r1 |
| `docs/.../14.1-.../SPEC.md:50,182` | "14"→"13" + drop the false `allowlist_envoy_only`-usage claim | I-2 |
| `docs/.../14.2-.../PROGRESS.md:247,346-349` | hallucinated SHA `9a228d44`→`8d06d6fb`; §2.2→§2.1/§5.5; honest M8 narrative | I-2 |
| `tests/fixtures/0022-*/README.md` | rewrite the `allowlist_envoy_only` provenance section to match reality | M-r2 |

**Verification of the in-review fixups (evidence, not assertion):**

- `cargo build -p envoy-cluster` → `Finished` (was `error[E0433]` before the fix).
- `cargo build -p envoy-cluster --all-targets` → `Finished`.
- `cargo build --workspace` → `Finished` (unification path unregressed).
- `cargo test -p envoy-cluster` → `70 passed; 0 failed`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → `Finished`, no warnings.
- `cargo fmt --all -- --check` → clean.
- `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (the "Zlib unmatched
  license allowance" is a pre-existing benign allow-list note, unchanged — the manifest edit added
  no dependency; `Cargo.lock` is unchanged).

The differential surface is **unaffected**: the `time` feature was already linked into the
envoy-bin binary via workspace unification when the state-4 fixture-0022 differential run produced
its green result, so the binary's runtime behavior is byte-identical. No Docker re-run required.

---

## 5. §7.5 phase-done gate re-attestation (against CI run `26692551259`, HEAD `64f4f576`, + the in-review re-verification)

- **(a) New fixture green:** fixture `0022-upstream-outlier-detection-consecutive-5xx` GREEN
  bilaterally vs `envoyproxy/envoy:v1.33.0` (state-4: `1 passed, 4.10s`; both proxies agree on the
  4-request `500,500,500,503` sequence + byte-exact bodies + `x-envoy-upstream-service-time`
  presence/absence + the 5 outlier_detection counters). **Confirmed.**
- **(b) Pre-existing fixtures green:** the 21 prior fixtures are inert-unaffected (the OD machinery
  short-circuits when unconfigured; the `panic_threshold` hoist is byte-identical for active-HC
  clusters). A 22-fixture `--include-ignored` run is the CI confirmation. **Confirmed.**
- **(c) h2spec ≥95%:** held vacuously — no H2 framing/codec touched (the H2 hook fires at the HCM
  post-dispatch logic site only). **Confirmed.**
- **(d) `parse_bootstrap` fuzz:** corpus unchanged at 22 seeds. **Confirmed.**
- **(e) Five stable-toolchain gates:** build / clippy / fmt / `cargo test --workspace` /
  `cargo deny check` clean — re-run in-review after the manifest edit (see §4). The one documented
  environmental flake (`upstream_h2_connection_pooling`, a 13.2 backstop that spawns its backend via
  compile-on-demand `cargo run` with a 30s budget; passes isolated `1 passed, 2.05s`; 14.2 touched
  zero h2-pool path) remains the only `cargo test --workspace` non-pass and is confirmed
  environmental. **Confirmed (CI builds helpers up front → green).**
- **(f) REVIEW.md approved:** THIS document. **Satisfied.**

---

## 6. Carryforward dispositions confirmed

- **M4** (14.1 REVIEW) — **DISCHARGED + VERIFIED** (this REVIEW is the named owner; §1.1).
- **M5 / M6** (14.1 REVIEW) — **CLOSED** at Task 1 (tie-test strengthening + `EndpointEjectionStats`
  exposure + drop vestigial binding; `max_ejection_percent==0` edge test + cap-site comment) —
  verified real, not tautological.
- **A-M2** (13.2 REVIEW) — **CLOSED** at Task 2 (`pool.rs:322` comment → `parking_lot::Mutex`) —
  verified accurate.
- **M8** (14.1 REVIEW) — **CLOSED** (fully reconciled to 13 in the canonical contract + the
  M8-named 14.1 SPEC §2.1/§5.5 + the live fixture README; the half-close was completed in-review —
  I-2). Frozen-record "14"s left per append-only/ratified doctrine (M-c3).
- **M1 / M2 / M3 / M7 / M9** + inherited multi-phase Minors + §6.9 extension + **ADR-0028** —
  carried forward unchanged (M-c3); no named owner; not engaged by 14.2.

---

## 7. ADR projection

**No new ADR.** A code review is docs-only and projects no ADR (SPEC §7; PLAN lock-in #2). The M4
serialization mirrors the 13.x pool `parking_lot::Mutex` write-serialization; the sweeper mirrors
the 12.2/13.x external-sibling-registry pattern; the `panic_threshold` hoist is a straightforward
bug correction; the in-review `tokio` `time`-feature addition is manifest hygiene. DECISIONS.md
ledger head stays **ADR-0041** (next available ADR-0042); `DECISIONS.md` is unmodified in the review
range. **ADR-0028 remains OPEN** (owner = a follow-up foundations-pivot phase, NOT 14.2).

---

## 8. Verdict + next state

**APPROVED.** All production-code logic is sound — both load-bearing items (the M4 single-writer
serialization and the `panic_threshold` outlier-detection-only bugfix) are verified correct, the D4
hooks cover every endpoint-attributed arm exactly once, the D7 sweeper matches the established
cancellation discipline and drains cleanly, and fixture 0022 asserts the bilateral acceptance signal
exactly. The two Important findings (the `envoy-cluster` `time`-feature build defect and the
half-closed M8 reconciliation + false PROGRESS claim) were both mechanically fixable
doc/manifest-hygiene corrections with no behavior change; they were resolved in-review and
re-verified (§4). The remaining Minors are non-gating follow-ups with no named owner.

**Next state — state 6 (CLOSING-sub-phase close-out), a LATER session** (per `BOOTSTRAP_PROMPT.md`
§5.1 one-state-per-session; the 14.1 precedent stops after REVIEW.md lands). The state-6 session
flips ROADMAP rows `14.2` AND parent `14` `in-progress → done` SIMULTANEOUSLY in one commit
(closing-sub-phase invariant; commit title carries `[parent 14 done]`, NO `[ADR-NNNN]` bracket —
SPEC §9 + PLAN Task 11). The ROADMAP is NOT flipped in this state-5 commit. After 14.2 closes,
parent-14 outlier detection is COMPLETE and the project advances to the next
Upstream-robustness-family phase per ROADMAP §9.
