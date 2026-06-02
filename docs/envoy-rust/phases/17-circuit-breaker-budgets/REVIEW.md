# Phase 17 (`17-circuit-breaker-budgets`) — REVIEW

- **Lifecycle state:** state 5 (verified → reviewed). This document is the state-5 output.
- **Skill:** `superpowers:requesting-code-review` (per `BOOTSTRAP_PROMPT.md` §5 state 5 + `SKILL_ROUTING.md`).
- **Review range:** `9774231e5..cb0717ef8` — the Task 1–10 state-3 execution arc + the Task-11
  state-4 verification, atop the state-2 PLAN-write `9774231e5` (the docs-only base; its code tree
  is identical to the pre-phase-17 baseline `ee61fc744`). The CODE commits reviewed: `902a5aa48`
  (T1 `Thresholds` budget fields `max_requests`/`max_retries`/`track_remaining`) → `12817303e` (T2
  `BudgetState` + RAII guards, `crates/envoy-cluster/src/budget.rs`) → `b714b43e8` (T3 `Cluster`
  budget integration + conditional registration + `BudgetAcquisition`) → `03d3b0ba7` (T4 H1
  retry-budget gate) → `ce9136c0b` (T5 H1 request-budget gate + overflow local reply) →
  `a468eac7f` (T6 H2 retry-budget gate mirror) → `a977879e9` (T7 H2 request-budget gate mirror) →
  `b565dee9d` (T8 fixture 0025 + Docker wrapper + the harness lib.rs wiring) → `e1ea23be7` (T9
  in-process backstop, 4 paths incl. the >0-cap concurrency regime) → `fd3d9b147` (T10 fuzz seed +
  BEHAVIOR_CONTRACT rows). The PROGRESS-subsection / STATE-advance commits in the range carry NO
  code. **No in-review fix commits were needed** (contrast with the phase-16 state-5 `995445b52`).
- **Pre-review HEAD:** `cb0717ef8` (== `origin/main` at review start; everything pushed).
- **Method:** 4 read-only code-review subagents, one per concern-cluster, dispatched **SERIALLY**
  (`feedback_serial_subagent_dispatch`), each reading the actual on-disk diff (`git diff` /
  `git show` + `Read` + `Grep`) and re-running the relevant non-Docker test suites
  (`cargo test -p envoy-config` 302/0, `-p envoy-cluster` 85/0, `-p envoy-http1` 99/0,
  `-p envoy-http2` 69/0/1, the in-process backstop 2× stable). The controller independently
  spot-verified the load-bearing claims every cluster verdict rests on (the `upstream_rq_5xx`
  tick-site counts, the L4 gauge-formula sites, the §5.3 overflow-tick sites, the backstop's
  non-gated execution) by direct grep against HEAD before accepting them.
- **Verdict: APPROVED** (zero Critical / **zero Important** / 8 non-gating Minors M17-1…M17-8
  carried). The first phase since 13.1 to clear state-5 review with no in-review fix and no
  Important finding.

---

## 1. The named review focus (STATE.md state-5 charter — the items this session MUST verify)

### 1.1 H1/H2 sibling parity of BOTH gates (Tasks 4–7) — **VERIFIED CORRECT (independently re-diffed)**

The H2-cluster reviewer extracted both files' production gate regions (H1
`crates/envoy-http1/src/hcm.rs:725-936`; H2 `crates/envoy-http2/src/hcm.rs:540-749`) and performed
an independent token-normalized diff — NOT a re-read of the per-task spec reviews' parity claims.
The diff reduces to comment wording, rustfmt line-wrapping, and the four documented
protocol-specific tokens (`run_h2_attempt`/`run_attempt`, `synth_h2_overflow()`/`synth_overflow(close)`,
value-flow-into-`finalize_h2_stream` vs mutable-`outgoing`-into-the-unified-writer, identifier
prefixes). Semantically identical on both sides: the three-arm `match cluster.try_acquire_retry()`
arm ordering (Unlimited → Acquired → Rejected), the counter ticks, the
`retry_guard_slot`/`retry_budget_blocked` declarations, the post-loop
`attempts > 1 && !retry_budget_blocked` split, the single-call-site `try_acquire_request()`
binding, and the guard release points (both at the dispatch-arm close, before the wire write).
The phase-16-review asymmetry bug class did not recur.

### 1.2 The L3 narrow `upstream_rq_5xx`-on-overflow departure — **VERIFIED: does NOT leak to other synth paths**

Controller-verified by grep at HEAD: exactly **two** `upstream_rq_5xx().inc()` sites per protocol
file — H1 `hcm.rs:745` (the new request-budget Rejected arm) + `hcm.rs:900` (the phase-16
completing-response gate); H2 `hcm.rs:565` + `hcm.rs:715`. Both reviewers confirmed via
`git diff -w` that the phase-16 gate condition
(`completing_upstream_response && final_response.status / 100 == 5`, the `995445b52` fix) is
byte-untouched on both protocols — the synth-path exclusion that fix established still holds for
every synth path except the one ADR-0047 L3 deliberately carved out. The departure is exactly as
narrow as the ADR states.

### 1.3 The L4 momentary-gauge semantic (`active > 0 AND active >= max`) — **VERIFIED CORRECT at every update site**

The formula is centralized in exactly two private helpers (`budget.rs:184` `update_retry_gauges`,
`:205` `update_request_gauges`), called from all three update sites (acquire-success,
acquire-failure, guard-Drop) — there is no site that can set a gauge to 1 without the same helper
later resetting it to 0. The zero-cap-stays-0 property is provable from the formula (with
`max == 0`, `active` never rises above 0, so `active > 0` is always false) and is pinned by the
unit test `zero_cap_open_gauge_stays_zero` (`budget.rs:416`). The non-zero edge is asserted at the
backstop's iv-a path (mid-flight scrape under a held slot → `rq_open == 1`, then falling edge to
0). Under concurrent acquire/release the gauge is eventually-consistent rather than linearizable
(a thread's `set()` uses the active value that thread observed) — within the L4 contract for a
momentary gauge, asserted only at rest (fixture) and mid-hold (backstop); recorded as M17-1.

### 1.4 The retry-guard lifetime threading (Task 4's `retry_guard_slot`) — **VERIFIED CORRECT**

Traced end-to-end on both protocols (H1 reviewer + H2 reviewer independently): the slot is
declared loop-scoped before the retry loop; the Acquired arm parks the guard AFTER the back-off
sleep, so the slot is held across the back-off AND the next iteration's in-flight `run_attempt`;
each reassignment acquires-then-drops in that order (the active count never dips below truth); the
explicit `drop(retry_guard_slot)` fires at loop exit (H1 `:921`; H2 `:735`). There is no `?` /
early-return inside either retry loop (`run_attempt`/`run_h2_attempt` convert all
connect/reset/overflow failures into owned synth responses internally), so no error path can leak
or early-drop the guard. The backstop's iv-b path proves the hold window under real concurrency:
the winner's guard being held across its ~1.5s retry attempt is what forces the loser's
deterministic rejection — 8/8 stable runs at the arc, 2/2 at this review.

### 1.5 The L9b request-guard span — **VERIFIED CORRECT (acquired once, spans the loop, no early drop)**

`cluster.try_acquire_request()` is called exactly once per dispatch on each protocol (H1 `:730`;
H2 `:555` region), bound and then re-matched (never re-invoked → no overflow double-tick). The
guard is arm-scoped: it spans the entire retry loop including all attempts (the L9b
counts-once-per-lifetime semantic, proven by in-crate test (c): `max_requests: 1` + a retry →
final 200 with `pending_overflow: 0`) and drops at the dispatch-arm close. The release point is a
few synchronous statements before the downstream wire write (header push + filter encode — an
await-free window); this is a hair earlier than Envoy's stream-completion release but is
unobservable by any stat or wire assertion in the sequential regime, and the backstop's
mid-flight/falling-edge assertions bound it under concurrency. Recorded as part of M17-3 (a
comment noting the await-free-window invariant would protect it from future edits).

---

## 2. Cluster verdicts

| # | Concern cluster | Tasks | Verdict | Critical | Important | Minor |
|---|---|---|---|---|---|---|
| 1 | `envoy-config` schema + `envoy-cluster` `BudgetState`/RAII guards/`Cluster` integration | T1, T2, T3 | **CLEAN** | 0 | 0 | 2 (M17-1, M17-2) |
| 2 | H1 retry-budget gate + H1 request-budget gate | T4, T5 | **CLEAN** | 0 | 0 | 2 (M17-3, M17-4) |
| 3 | H2 gate mirrors + H1/H2 sibling parity | T6, T7 | **CLEAN** | 0 | 0 | 2 (M17-5, M17-8) |
| 4 | Fixture 0025 + harness wiring + in-process backstop + fuzz seed + BEHAVIOR_CONTRACT | T8, T9, T10 | **CLEAN** | 0 | 0 | 2 (M17-6, M17-7) |

Cluster-1 highlights: the CAS loop is race-free (bound check + increment fused through one
`compare_exchange`; no TOCTOU; the cap can never be exceeded; counts can never go negative under
RAII); the overflow counters tick exactly once per failed acquire (`budget.rs:114`/`:149` — never
on CAS contention); the shared-Arc idempotent registration of `upstream_rq_pending_overflow`
(pool + BudgetState) is directly tested from both sides; the `Unlimited` path is provably
side-effect-free; the validator zero-cap acceptance matches D2/L1/L2 with full positive/negative
coverage. Cluster-2 highlights: the unified-writer fall-through (the approved Task-5 PLAN
deviation) was verified equivalent to the pool-PendingOverflow arm's accounting (`downstream_rq_5xx`
+ access log fire identically); L7 exclusivity holds including the subtle >0-cap
partial-then-blocked case where the `!retry_budget_blocked` flag is uniquely load-bearing.
Cluster-3 highlights: per-stream budget contention on H2 is correct (guards are task-local; the
only shared state is the cluster's atomics behind the CAS loop). Cluster-4 highlights: the fixture
pair is semantically identical modulo documented deltas; all 22 stat assertions trace 1:1 to the
L1–L12 lock-ins + the ADR-0047 re-anchoring; the harness lib.rs wiring is exact-name-gated and
provably inert for fixtures 0001–0024; the backstop's polling helpers return
last-observed-value-on-timeout (no vacuous passes); the BEHAVIOR_CONTRACT "17 entries" rows match
the code's tick sites and conditionality exactly.

---

## 3. Controller verification notes

Per the phase-16 state-5 method, the controller did not accept cluster verdicts on faith:

1. **Tick-site counts** (the §1.2 isolation claim): re-verified by grep at HEAD — exactly 2 sites
   per protocol file. MATCHES both reviewers.
2. **L4 formula centralization** (§1.3): re-verified by grep — the formula appears in exactly the
   2 helpers + doc comments/tests. MATCHES.
3. **§5.3 overflow single-source-of-truth**: re-verified by grep — `rq_retry_overflow.inc()` and
   `rq_pending_overflow.inc()` each appear at exactly one production site, inside the failed
   acquires. MATCHES.
4. **One reviewer claim CORRECTED:** the H1 reviewer characterized the in-process backstop as
   "Docker-gated / `#[ignore]`-class / won't run in a normal `cargo test`" when arguing the M17-4
   in-crate test gap. Controller-verified: `crates/envoy-bin/tests/upstream_circuit_breaker_budgets.rs`
   carries NO `#[ignore]` and NO Docker dependency — it runs in every `cargo test --workspace` and
   in CI. The M17-4 gap is therefore narrower than the reviewer stated (the >0-cap L7 path IS
   exercised on every CI run via backstop iv-b); the finding is retained as a Minor on
   defense-in-depth grounds only.

---

## 4. Carryforward dispositions + Minor findings (non-gating)

### 4.1 Arc-discovered carryforwards (from PROGRESS Task 11; reviewed + dispositioned)

1. **Fixture 0022 joins the 0011/0012 CI readiness-flake family** (PRE-EXISTING; surfaced by the
   Task-9 push's CI run `26797777933`). A loaded-runner startup race — `envoy-rust never became
   accept-ready within 10s` — on a fixture that configures no budgets; green on the next run with
   identical code. **Disposition: carries unchanged** (the flake family is now 0011 + 0012 + 0022;
   memory `project_flaky_access_log_fixture_0012` already records this).
2. **`http1_echo_backend_drop_terminates_child` harness unit-test flake** (PRE-EXISTING, phase-13
   era; a 200ms drop-to-port-release window). Flaked once locally under full-workspace load during
   the state-4 verification; passed exclusive, in CI, and on re-run. **Disposition: carries as a
   standing harness caution; a future harness-hardening pass could widen the window or poll.**
3. **The pool-liveness family** (PRE-EXISTING, phase-16 carryforwards 1+2: the H1-pool
   `Connection: close` re-pooling gap + the H2-pool send-failure no-invalidate asymmetry M16-5).
   Phase 17 did not engage either; the retry-budget gate marginally REDUCES dead-conn re-acquire
   exposure (a budget-blocked retry surfaces the response immediately instead of re-acquiring).
   **Disposition: carries unchanged to the future pool-hardening + pending-queue phase.**
4. **The H2-upstream-fork retry coverage gap** (PRE-EXISTING, phase-16 carryforward 4). Phase 17's
   H2 in-crate budget tests dispatch to H1 upstream backends (M17-5) — the same coverage shape.
   **Disposition: carries; groups with the future H2-focused phase / stateful-H2-backend item.**
5. **The cyclic retry-script parallel-drive fragility** (PRE-EXISTING, phase-16 carryforward 3).
   Fixture 0025 reuses the same stateful backend; the reviewer confirmed the caution is carried
   verbatim in the 0025 README (`README.md:59-76`). **Disposition: carries as a standing harness
   caution.**
6. **M16-3 (`x-envoy-attempt-count` synth-path emission) — CLOSED.** The phase-17 §6.2 item 11 /
   ADR-0047 L11 empirically verified Envoy's behavior; fixture 0025 asserts the header bilaterally
   on all three probes (1/2/1) including the overflow local reply. **The state-6 rollovers must
   record M16-3 as CLOSED.**

### 4.2 Minor findings (M17-1 … M17-8; none gating; carried with no named owner)

| # | Finding | File | Why non-gating |
|---|---|---|---|
| M17-1 | The momentary `*_open` gauges are eventually-consistent (not linearizable) under concurrent acquire/release — a thread's `set()` uses its own observed active value | `crates/envoy-cluster/src/budget.rs:179-209` | By design within the ADR-0047 L4 contract; the module doc documents it; every assertion site (fixture at-rest 0; backstop mid-hold 1) is insensitive to the transient |
| M17-2 | `from_bootstrap` budget resolution consults only `thresholds.first()` — if the multi-threshold rejection were ever relaxed, thresholds[1..] would be silently ignored | `crates/envoy-cluster/src/cluster.rs` | Unreachable: `validate_circuit_breakers` rejects multiple thresholds; multi-priority is an explicit §4 deferral |
| M17-3 | The H1 `_request_guard` deferred-init shape (an `if let` + later `match` with a `_ => None` arm whose reachability rests on the outer gate) + the guard release at arm close, a few await-free statements before the wire write | `crates/envoy-http1/src/hcm.rs:735-767` (H2 mirror `:555`/`:586`) | Works correctly; clippy-clean; the await-free-window property keeps the early release unobservable — a comment noting that invariant would protect it from future edits |
| M17-4 | No in-crate unit test for the >0-cap partial-then-blocked L7 case (one retry succeeds, a later one is budget-blocked → neither `_success` nor `_limit_exceeded` ticks) — the only case where `!retry_budget_blocked` is uniquely load-bearing | `crates/envoy-http1/src/hcm.rs` + H2 mirror test modules | The backstop's iv-b path exercises it end-to-end in every `cargo test` run and in CI (controller-corrected; see §3.4); the in-crate gap is defense-in-depth only |
| M17-5 | The H2 in-crate budget tests dispatch to H1 upstream backends (no stateful H2 test backend exists) | `crates/envoy-http2/src/hcm.rs` test module | Pre-existing phase-16 coverage shape (the budget gate sits before the protocol fork); grouped with carryforward 4.1.4 |
| M17-6 | Backstop polish: the `u64::MAX` "stat absent" sentinel deserves a comment; the interdependent 1500ms-hold/700ms-scrape literals could be named constants; the single-line escaped cluster-YAML strings hurt readability; `scrape_admin_stats` silently drops non-numeric rows | `crates/envoy-bin/tests/upstream_circuit_breaker_budgets.rs` | Cosmetic; the test is deterministic (8/8 + 2/2 stable runs) and non-vacuous (polling returns last-observed-value, not success-on-timeout) |
| M17-7 | Fixture 0025 doc polish: the README "Reuse" bullet could note `rq_zero` deliberately omits `track_remaining`; the harness lib.rs fixture-name else-if chain is a future consolidation candidate; the `rq_zero` expectations block's omitted `rq_retry_open` assertion is an undocumented asymmetry | `tests/fixtures/0025-.../README.md`, `tests/differential/src/lib.rs` | Documentation/maintainability polish on a fixture that passed bilaterally first-run |
| M17-8 | H2 pushes `"x-envoy-attempt-count"` as a string literal where H1 uses the `X_ENVOY_ATTEMPT_COUNT` const | `crates/envoy-http2/src/hcm.rs` | Pre-existing phase-16 convention (= M16-6 carried); values identical; cosmetic parity nit |

### 4.3 Standing multi-phase Minor inventory (inherited; not engaged by phase 17)

The 14.1 REVIEW M-track items, M-c1 (`tokio-util` `["rt"]`-leanness), M-c2 (`.lock().unwrap()`
poison-hardening), M-c3 (frozen-record "14"s), the §6.9 per-class `upstream_rq_{2,3,4}xx`
extension, the `upstream_cx_total` TCP-proxy carve-out, the phase-15 rollovers (per-endpoint
`cx_open` reconciliation; the `max_pending_requests > 0` pending queue), the phase-16 Minors
M16-1/M16-2/M16-4/M16-7/M16-8 (M16-3 is CLOSED per §4.1.6; M16-5/M16-6 are re-recorded above as
the pool-liveness family / M17-8), and ADR-0028 all carry forward unchanged.

---

## 5. §7.5 phase-done gate re-attestation

The state-4 verification (PROGRESS Task 11) ran gates (a)–(e) ALL GREEN at HEAD `8bef1c109` with
CI anchor `26798019441` (`completed / success`). **This review produced no code changes**, so the
state-4 record stands as the phase-done evidence; the review's own re-verification is the
per-cluster local test re-runs:

| Gate | State-4 evidence (HEAD `8bef1c109`, CI `26798019441`) | Review re-verification (HEAD `cb0717ef8`, local, read-only) |
|---|---|---|
| (a) fixture 0025 green | `ok. 1 passed` bilateral, first-run, zero divergences | Unchanged code; assertion set re-traced 1:1 against the L1–L12 lock-ins + ADR-0047 (cluster 4) |
| (b) 24 pre-existing fixtures green | All 25 green simultaneously (exclusive run + re-passed inside `cargo test --workspace`) | Unchanged code; harness lib.rs wiring re-verified exact-name-gated/inert (cluster 4) |
| (c) h2spec ≥95% | CI-anchored (phase 17 touches no H2 framing) | Unchanged |
| (d) fuzz clean | `Done 200000 runs`, cov 14437, 0 crashes, 28-seed corpus | Seed re-verified parse-clean via the corpus gate (cluster 4) |
| (e) 5 stable gates | build/clippy/fmt/test (~1010/0/2)/deny all clean | `cargo test -p envoy-config` 302/0; `-p envoy-cluster` 85/0; `-p envoy-http1` 99/0; `-p envoy-http2` 69/0/1; backstop 2× stable; clippy clean (clusters 1–4) |
| standalone builds (`project_isolated_crate_build_blindspot`) | 4/4 clean | Unchanged code |
| (f) REVIEW.md approved | — | **THIS document — APPROVED** |

Because this review lands no code, the CI run triggered by this commit's push is docs-only
(vacuous-green expected); the state-4 CI anchor `26798019441` remains the phase's differential
evidence. No §5.2 state-3 re-entry condition exists.

---

## 6. ADR projection

**No new ADR.** The review found no decision-level divergence: the implementation faithfully
realizes ADR-0046 (the scope + the §5.4 cluster-scoped ownership boundary — verified: no budget
state leaked into the pools) and ADR-0047 (the L3 narrow departure + the L4 momentary semantic —
verified at every site). Ledger head stays **ADR-0047** (count 48; next available **ADR-0048**,
never consumed by the split that did not fire). **ADR-0028** (H1-listener × H2-cluster dispatch
deferral) remains OPEN — phase 17 does not engage it.

---

## 7. Verdict + next state

**APPROVED.** Zero Critical; zero Important; 8 non-gating Minors (M17-1…M17-8) + 6 carryforward
dispositions (including the M16-3 CLOSURE) are recorded above with no named owner. No in-review
fix was needed — the per-task two-stage review discipline of the state-3 arc (16 findings fixed
in-task pre-push) left nothing gating for state 5.

Per `BOOTSTRAP_PROMPT.md` §5 state 6 + §5.1 (one state per session), the **next session performs
the state-6 deterministic close-out**: flip ROADMAP row `17` `in-progress → done` (a non-split
top-level phase flips its own row alone), advance STATE.md to "awaiting next planning", append the
`### Phase-17 rollovers` Notes subsection (recording M16-3 as CLOSED per §4.1.6 + the M17-1…M17-8
inventory + the carryforward dispositions), and land the §5.3-format final phase commit
(`phase 17: circuit-breaker budgets … [ADR-0046, ADR-0047]` with the `Differential surface:` +
`Conformance:` trailer lines). **After that close-out, the Upstream-robustness family is complete
in minimum-viable form.**
