# Phase 14.1 (`14.1-endpoint-ejection-and-lb-integration`) — PROGRESS

> Per-task narrative log. Appended at every task commit per the 06.2 / 06.3 / 07.x / 08.x /
> 09 / 10 / 11 / 12.1 / 12.2 / 13.1 / 13.2 cadence. State-2 PLAN-write lands this skeleton +
> the Task 1 preamble; state-3 dispatch appends `### Task N — <name>` subsections in
> execution order.

---

## State-2 commit context

This commit (the state-2 standalone PLAN-write commit) lands:

- **CREATE** `docs/envoy-rust/phases/14.1-endpoint-ejection-and-lb-integration/PLAN.md` (the
  state-2 PLAN.md per `BOOTSTRAP_PROMPT.md` §5 state 2; 7 tasks; 25 architecture lock-ins;
  12 PLAN-write SPEC corrections; full `- [ ]` checkbox TDD steps with complete code per task).
- **CREATE** `docs/envoy-rust/phases/14.1-endpoint-ejection-and-lb-integration/PROGRESS.md`
  (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flip row `14.1` `status: planned` →
  `status: in-progress`. No other row touched (parent row `14` stays `in-progress`; `14.2`
  stays `planned`).
- **MODIFY** `docs/envoy-rust/STATE.md` — Active phase status; Next expected skill; Last
  commit; Last updated; new `### Phase-14.1 state-2 PLAN-write` subsection in Notes.

**Predecessor commit:** `0a4d225` — `phase 14: state-2 SPLIT — sub-phase SPECs 14.1 + 14.2 +
ADR-0040 (split) + ADR-0041 (§6.2 empirical revision) [ADR-0040, ADR-0041]` (the parent-14
state-2 SPLIT commit; immediate prologue; HEAD == origin/main at this PLAN-write's prologue;
CI run `26424002812` settled `success`).

**SPEC commit base:** `0a4d225`. **This state-2 commit makes NO inline SPEC.md edits** — the
§6.2 empirical verification was ratified at the parent-14 state-2 SPLIT (`0a4d225`) by
ADR-0041; the 14.1 PLAN-writer baked the locked facts into the PLAN lock-ins without
re-running Docker.

**ROADMAP status before this commit:** row `14.1` `planned` (added at the parent-14 state-2
SPLIT). **ROADMAP status after this commit:** row `14.1` `in-progress`. Parent row `14`
remains `in-progress` (the closing-sub-phase invariant; parent flips at 14.2 close).

**STATE.md "Active phase" status before:** `phase 14.1 lifecycle state 1-complete / state-2-next (SPEC.md exists; PLAN.md does NOT)`.
**STATE.md "Active phase" status after:** `phase 14.1 lifecycle state 2-complete / state-3-next (PLAN.md + PROGRESS.md skeleton + Task 1 preamble landed; first task implementation pending)`.

**DECISIONS.md status before AND after:** **ADR-0041** (project-cumulative count 42). **No
ADR lands at this state-2 commit** (SPEC §7 + PLAN lock-in #2 — 14.1 introduces no new
crate, no foundations grant, no wire-level contract revision; the `EndpointEjection`
`Relaxed`-ordering choice is covered by the existing `cluster.rs` `pick()` precedent + the
12.1 `EndpointHealth` precedent). Next available number stays **ADR-0042**.

**BEHAVIOR_CONTRACT.md status before AND after:** Unchanged at this commit. The 7 new
`Stat-name mapping` rows (under a new `**14.1 entries (outlier detection):**` block) land
at **Task 5** per the 06.x → 13.2 cadence (contract extensions land at the task where the
surface is first wired, NOT at PLAN-write time).

**ENVOY_TARGET.md + rust-toolchain.toml:** Unchanged (D-3.7 / D-3.9).

---

## PLAN scope summary

- **7 tasks** per PLAN §File-Structure + tasks 1-7. Subagent-driven execution at state 3 per
  PLAN lock-in #23 + `feedback_execution_style`.
- **~900-1100 LoC projected** (production ~430, tests ~520, doc / corpus ~60) — comfortably
  under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task gate. The parent-14 split
  (ADR-0040) already absorbed the over-gate scope into 14.1 + 14.2; 14.1 does NOT nest-split
  (lock-in #1; standalone-PLAN posture per `feedback_pick_recommendation`).
- **ZERO ADR landings** (lock-in #2; SPEC §7).
- **NO new differential fixture** — regression-equivalence via the 21 existing Docker-gated
  fixtures (`0001`-`0021`) staying green simultaneously proves the machinery is inert when
  `outlier_detection` is unconfigured (the 05.1 / 07.1 / 12.1 foundation-slice pattern;
  §5.3).
- **D8.2 fuzz seed lands at 14.1** (per SPEC §6.1 recommendation + lock-in #4 — the seed
  exercises the new envoy-config surface 14.1 introduces; corpus 21 → 22 success seeds).

---

## Task 1 preamble

### §6.2 empirical-verification findings — locked at the parent-14 SPLIT (`0a4d225`) by ADR-0041; NOT re-run at this PLAN-write

Per the 14.1 SPEC §2.1 + §6.2 + STATE.md `### Phase-14 state-2 split decision` + ADR-0041,
the HEAVY 9-item §6.2 verification against `envoyproxy/envoy:v1.33.0` was performed at the
parent-14 state-2 SPLIT commit `0a4d225` (Docker bridge networking + the project's
`tests/helpers/health-aware-http1-backend/` Rust helper + admin `/stats?filter=outlier_detection`
+ `/clusters` + curl/Python data-plane drivers). **7 of 9 items MATCHED the parent SPEC's
projection; 2 of 9 items DIVERGED MATERIALLY (ratified by ADR-0041).** The findings binding
14.1 are LOCKED FACTS (PLAN lock-in #3); the 14.1 PLAN-writer baked them into the PLAN
without re-running Docker:

1. **Config defaults (MATCHED):** `consecutive_5xx=5, consecutive_gateway_failure=5,
   interval=10s, base_ejection_time=30s, max_ejection_percent=10`. D1 (Task 1) bakes the
   doc-comment defaults exactly; D2 (Task 2) reuses `parse_duration`. The deferred-field
   list rejected by `deny_unknown_fields` is enumerated in PLAN lock-in #6 (note:
   `interval_jitter` was correctly excluded from the rejected-fields comment — it is NOT a
   v3 field name).
2. **Stat namespace (DIVERGED MATERIALLY — ADR-0041 item-2):** Envoy emits 21 names;
   envoy-rust emits the **7-name minimum-viable subset** (PLAN lock-in #19 — 1 gauge + 6
   counters; per ADR-0041 §6.2 item-2 the per-detector counters split into `_detected_/_
   enforced_` pairs and the cluster-level `ejections_overflow` lives on
   `OutlierDetectionState`). D6 (Task 5) wires exactly these 7 names.
3. **Initial state (MATCHED):** endpoints implicitly never-ejected at boot; NO warmup
   window. D3 (Task 3) `EndpointEjection::new` initializes `ejected: false` with both
   consecutive counters at 0; the `ejections_active` gauge stays 0 across construction
   (lock-in #12).
4. **`max_ejection_percent` cap (MATCHED):** cap formula `floor(host_count *
   max_ejection_percent / 100)`; overflow re-fires per detection-tick (NOT once-per-host).
   D3 + Task 4 lock the cluster-level cap check inside `Cluster::record_response`
   (lock-in #15 + #19 — `ejections_overflow` lives on `OutlierDetectionState`).
5. **Ejection-time + counter reset (MATCHED but DEFERS to 14.2):** un-eject at sweep time
   resets BOTH per-detector counters; 2xx/3xx/4xx responses also reset both. 14.1 wires
   the reset semantics in `EndpointEjection::record_response` + `try_un_eject` (lock-in
   #18); the sweeper-driven un-eject site defers to 14.2 D7.
6. **Fixture observable (DEFERS to 14.2 D8.1):** request 1-3 backend 500 + body
   `server error\n`; request 4 synth-503 + body `no healthy upstream` (19 bytes per
   ADR-0037); `x-envoy-upstream-service-time` PRESENT on backend 5xx, ABSENT on synth-503.
   14.1 lands no fixture.
7. **Composition with active HC (MATCHED):** AND-composition via independent health-flag
   bits at the `pick()` candidate-build site. D5 (Task 4) implements the AND-composition
   slow-path arm; the fast-path arm preserves 12.1's both-filters-None invariant verbatim
   (lock-in #10 + #13).
8. **H1 vs H2 sibling parity (DEFERS to 14.2 D4):** identical stat namespace + ejection-
   on-5xx semantics. 14.1 ships no protocol-specific code; the H1+H2 router-arm wiring
   is uniform at 14.2 D4.
9. **Connect-failure classification + synth-status bypass (DIVERGED MATERIALLY — ADR-0041
   item-9):** Envoy classifies connect-failure as BOTH `consecutive_5xx` AND
   `consecutive_gateway_failure` for the picked endpoint. PLAN lock-in #17 simplifies
   the implementation: the classifier is **purely status-driven** (502/503/504 → both
   detectors automatically; no `source` arg needed). The `pick() -> None` synth-503
   bypass decision lives at the 14.2 D4 call-site (does NOT call `record_response` when
   there's no endpoint to attribute). 14.1 unit tests verify the status-driven classifier
   in Task 3.

### PLAN-write SPEC corrections (read against HEAD `0a4d225`)

The 12 corrections in PLAN §1 (verified against HEAD): (1) config `Cluster` derives
`Debug, Serialize, Deserialize, PartialEq` (NO `Clone`; HAS `Serialize`) — the new
`OutlierDetection` struct matches; (2) `parse_duration` is at `bootstrap.rs:2401`, NOT
`:2289` as the 14.1 SPEC sketched (drift accumulated through phase additions); (3) runtime
`Cluster` struct is at `cluster.rs:43-87` (10 existing fields including 12.1 `endpoint_health`
+ `panic_threshold` at lines 77-87), NOT lines 60-76; 14.1 appends `outlier_detection`
after `panic_threshold`; (4) `Cluster::pick()` at `cluster.rs:166` is private — the 12.1
fast-path at `:172-178` is preserved verbatim; (5) `ConfigError` lives in `lib.rs:43-463`;
new 14.1 variants slot at the end after `InvalidMaxConnections`; (6) `validate_outlier_detection`
appends to the per-cluster loop after `validate_circuit_breakers` at `:1727`; (7)
`StatsRegistry::register_counter` / `register_gauge` return `Arc<...>`; idempotent
same-kind re-registration; (8) `ClusterError::StatsRegistration` is the error-mapping
pattern reused; (9) `EndpointHealth` shape verified at `health.rs:32-90` — `EndpointEjection`
mirrors with more stat handles per lock-in #20; (10) `Cluster::record_response` endpoint
lookup uses `self.endpoints.iter().position(...)` (linear scan over small Vec; defense-in-depth
silent on unknown endpoint per lock-in #15); (11) the 4 runtime `Cluster {}` literal sites
audited at HEAD (`cluster.rs:573, :627, :914, :1511`); (12) the 2 by-hand
`envoy_config::Cluster {}` literal sites audited at HEAD (`cluster.rs:825, :860` — only
test scaffolding; `typed_extension_protocol_options` references in
`crates/envoy-http2/src/hcm.rs` + `crates/envoy-bin/tests/http2_router_upstream.rs` are
inside YAML strings, NOT Rust literals — no compile-fix needed; mirrors the 12.1 PROGRESS
Task 1 refinement).

### Carryforward disposition

14.1 engages **no** carryforward (lock-in #24). The 13.2 A-M2 stale-comment carryforward
at `crates/envoy-http1/src/pool.rs:322` is **not touched at 14.1** (foundation slice — no
envoy-http1 touch); the A-M2 close opportunity defers to **14.2 D4** which DOES touch
`envoy-http1::router`. The 06.3 REVIEW I2 was FULLY CLOSED at parent-13 `96630f9`. The
inherited multi-phase Minor carryforward inventory (parent-14 SPEC §6.9) carries forward
UNCHANGED — 14.1 closes no named carryforward.

### Subagent-driven execution at state 3

Per `feedback_execution_style` auto-memory + the 06.x → 13.2 cadence, each task below is
dispatched to a fresh subagent with two-stage review (spec-compliance THEN code-quality);
TDD per task per `superpowers:test-driven-development`; one commit per task per the 06.x
→ 13.2 one-commit-per-task cadence. The state-2 PLAN-write (THIS commit) is the
controller's authoring pass — NOT a subagent dispatch.

---

<!-- state-3 task subsections append below this line -->

## Phase-14.1 state-3 execution arc (Tasks 1-7)

_To be appended in execution order. Each task entry mirrors the 12.1 / 13.1 PROGRESS
narrative: one paragraph capturing the commit SHA, the key implementation decision, any
review-surfaced fold-ins, and the `cargo fmt --all -- --check` / `cargo clippy` confirmation._
