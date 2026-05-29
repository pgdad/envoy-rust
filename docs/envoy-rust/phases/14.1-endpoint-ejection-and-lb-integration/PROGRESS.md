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

### Task 1 — D1 envoy-config OutlierDetection schema + config-`Cluster`-literal compile-fix — `0170d1e`

Landed the `OutlierDetection` config struct (5 fields — `consecutive_5xx`,
`consecutive_gateway_failure`, `interval`, `base_ejection_time`, `max_ejection_percent`; all
`Option<_>` with `#[serde(default)]`; derives `Debug, Serialize, Deserialize, PartialEq` —
NO `Clone` per lock-in #5/correction #1; `#[serde(deny_unknown_fields)]`) adjacent to the
`CircuitBreakers` group in `crates/envoy-config/src/bootstrap.rs`, plus the
`outlier_detection: Option<OutlierDetection>` field appended to `Cluster` after
`circuit_breakers` (default `None` — the §5.3 inert-when-unconfigured invariant). Re-exported
`OutlierDetection` from `lib.rs`. Doc comment enumerates the parent-§4 deferred fields rejected
by `deny_unknown_fields` and correctly OMITS `interval_jitter` (lock-in #6). 3 parse tests
(positive minimum-viable / default-None / `deny_unknown_fields` rejection via
`success_rate_minimum_hosts`). Mechanical compile-fix added `outlier_detection: None` to the 2
by-hand `envoy_config::Cluster {}` test literals in `crates/envoy-cluster/src/cluster.rs`
(`from_bootstrap_rejects_empty_cluster` + `from_bootstrap_rejects_duplicate_cluster_name`). TDD:
tests written first, confirmed red (E0609 "no field `outlier_detection`"), then green.
`cargo test -p envoy-config --lib` 3/3 new green; `cargo test -p envoy-cluster --lib` compile-fix
tests green; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean;
`cargo build --workspace --all-targets` clean. Two-stage review: Stage-1 spec-compliance ✅ (all
5 fields/derives/tests/compile-fixes verified, no over-build, only the 3 files touched); Stage-2
code-quality APPROVE with one **Minor** cosmetic doc-grammar nit (the `Cluster.outlier_detection`
field doc "`None` (the §5.3 …)" lacks a verb vs the sibling `circuit_breakers` "`None` means
defaults") — deferred to state-5 REVIEW.md per the one-commit-per-task / Minor-deferral cadence;
no fold-in.

### Task 2 — D2 `validate_outlier_detection` + 3 `ConfigError` variants — `10980b1`

Added 3 `ConfigError` variants to `lib.rs` after `InvalidMaxConnections`:
`InvalidOutlierDetectionThreshold { cluster, field: &'static str }` (consecutive_5xx /
consecutive_gateway_failure must be ≥ 1 when present), `InvalidOutlierDetectionTiming { cluster,
field }` (interval + base_ejection_time parse via `parse_duration`; rejects parse-failure AND
zero-duration AND sub-second decimals), `InvalidMaxEjectionPercent { cluster, value: u32 }` (range
`[0,100]`; 0 and 100 both accepted). Added `validate_outlier_detection(cluster)` sub-validator in
`bootstrap.rs` adjacent to `validate_circuit_breakers`, wired into `validate()`'s per-cluster loop
immediately after `validate_circuit_breakers(cluster)?;`. Empty `outlier_detection: {}` is ACCEPTED
(falls through all `if let Some` arms to `Ok(())` — no separate error variant per lock-in #7). The
duration loop reuses the verbatim `validate_health_checks` idiom (`match parse_duration(raw) {
Ok(d) if !d.is_zero() => {}, _ => Err(...) }`). Let-chains compiled as-is (already used in
`validate_circuit_breakers`). 10 validator tests + `build_od_yaml` helper cover every positive +
negative path incl boundaries (0/100/101, empty block, `0.5s` sub-second). TDD: 10 tests red
(variants not in scope), then green. `cargo test -p envoy-config --lib` 285/285 green; clippy clean
(no `dead_code` on the 3 variants — all reachable via the wired call site). Two-stage review:
Stage-1 spec-compliance ✅ (3 variants + validator + call-site placement + 10 tests all verified;
no over-build; only 2 files touched); Stage-2 code-quality APPROVE with 3 **Minor** cosmetic
findings — (a) `v < 1` vs sibling `validate_circuit_breakers`'s `== 0` style split; (b) the
`_accepts_minimum_viable_full_block` test hand-indents a multi-line `\n`-joined body string
(brittle vs the clean single-line `r#"..#"` in the other 9); (c) `InvalidOutlierDetectionTiming`
doc's second sentence omits restating the zero-duration case (covered in the first sentence) — all
deferred to state-5 REVIEW.md; no fold-in.

### Task 3 — D3 `EndpointEjection` state machine (new `ejection.rs`) — `e506b9c`

Created `crates/envoy-cluster/src/ejection.rs` (sibling to the 12.1 `health.rs`; NO new crate) with
the per-endpoint outlier-detection state machine: `EndpointEjectionStats` (6 shared `Arc` stat
handles — 1 `Gauge` + 5 `Counter`; `#[derive(Clone, Debug)]`), `DetectorType` enum
(`Consecutive5xx | ConsecutiveGatewayFailure`), `EjectionDecision { crossed_5xx, crossed_gateway_failure }`
+ `any()`, and `EndpointEjection` (`ejected: AtomicBool`, two `AtomicU32` consecutive counters, two
`u32` thresholds, `stats`). All atomics `Ordering::Relaxed` (single-writer-per-endpoint at 14.2;
12.1 `EndpointHealth` + `pick()` cursor precedent). `new` starts never-ejected (§6.2 item-3).
`record_response(status)`: already-ejected → no mutation; 5xx (`status/100==5`) ticks consecutive_5xx
+ (502/503/504) also ticks consecutive_gateway_failure, each with a `threshold>0` defensive guard;
inc `ejections_detected_*` on crossing; non-5xx resets BOTH counters (§6.2 item-5). `eject(detector)`
swap-edges the gauge + `ejections_enforced_total` + matching `_enforced_<detector>` (idempotent);
`try_un_eject()` swap-edges down, resets both counters, returns whether it actually un-ejected.
Re-exported the 4 types from `lib.rs`. 13 unit tests assert concrete counter/gauge values across init,
classifier-per-status-class, threshold-crossings, reset, eject idempotency, un-eject convergence,
threshold-0. TDD red→green; `cargo test -p envoy-cluster --lib` 49/49 (13 ejection + no regressions);
clippy clean. **One PLAN-code-vs-clippy reconciliation (accepted, behavior-identical):** the PLAN's
verbatim `matches!(status, 502 | 503 | 504)` triggered `clippy::manual_range_patterns` under
`-D warnings`; changed to `matches!(status, 502..=504)` (same 502/503/504 set). Two-stage review:
Stage-1 spec-compliance ✅ (all 4 types/derives/fields, all 6 methods incl already-ejected
short-circuit + detected-vs-enforced separation + Relaxed everywhere, 13 named tests, lib.rs wiring;
no over-build); Stage-2 code-quality APPROVE with 2 **Minor** polish items — (a) the `mk` test helper
hand-clones all 6 `Arc`s into a second `EndpointEjectionStats` literal where `stats.clone()` would do
(`EndpointEjectionStats` already derives `Clone`); (b) optional one-line inline single-writer-contract
note on `EndpointEjection` to match `health.rs`'s heavier inline doc — both deferred to state-5
REVIEW.md; no fold-in.

### Task 4 — D5 `Cluster::pick()` AND-composition + `Cluster::record_response` — `09bc34a`

Integrated outlier detection into the runtime `Cluster` (`cluster.rs`). Added private
`OutlierDetectionState` (`pub(crate)`: `endpoints: Vec<Arc<EndpointEjection>>`,
`max_ejection_percent: u32`, `ejections_overflow: Arc<Counter>`); appended
`outlier_detection: Option<OutlierDetectionState>` to `Cluster` after `panic_threshold`. Rewrote
`pick()`: fast-path guard extended to `endpoint_health.is_none() && outlier_detection.is_none()`
→ byte-for-byte phase-02 round-robin (regression-equivalence gate (b)); slow-path `is_eligible(i)`
closure AND-composes `healthy && not_ejected` (each `None` filter vacuously true), preserves the
12.1 strictly-below panic comparison + the `eligible_idx.is_empty() → None` arm (the 12.2 synth-503
path fires unchanged). Added `Cluster::record_response(endpoint, status)` (NO production caller at
14.1): no-op when OD unconfigured (§5.3) or endpoint unknown (defense-in-depth via `position`);
delegates to `EndpointEjection::record_response`; on `decision.any()` computes
`cap_count = (total * max_ejection_percent) / 100` (integer floor), counts active ejections, and on
`active_count >= cap_count` increments `ejections_overflow` per detection-tick (ADR-0041 §6.2
item-2) WITHOUT ejecting; else ejects with the 5xx-wins-ties detector (lock-in #15). Added the
`ClusterHandle::record_response` delegate. Updated all 4 in-crate runtime `Cluster {}` literals to
`outlier_detection: None` (production `from_bootstrap` stays `None` — Task 5 wires the configured-OD
arm); the 2 `envoy_config::Cluster` literals already carry the field from Task 1 (left alone).
**Reconciliation:** real `Cluster` matched the PLAN's 11-field set (no reorder); the real 12.1
`pick()` used a `healthy_count`/`healthy_percent`/`healthy_idx` slow path — extended to the
AND-composition so health-only clusters are semantically identical (ejection branch vacuously true);
the existing 12.1 health-only pick tests pass unchanged. 10 new tests + the
`mk_handle_with_health_and_ejection` helper (carries `#[allow(clippy::too_many_arguments)]` — 8-arg
test-only signature, idiomatic suppression). TDD red→green; `cargo test -p envoy-cluster --lib`
59/59 (49 prior + 10 new); `cargo build --workspace --all-targets` clean; clippy clean. Two-stage
review: Stage-1 spec-compliance ✅ (struct/field/pick-fast+slow-path/record_response cap-logic/
delegate/4-literals/10-tests all verified; no over-build; no production caller; from_bootstrap still
`None`; integer-floor cap with `>=`; 12.1 semantics preserved; only cluster.rs touched); Stage-2
code-quality APPROVE with 3 **Minor** findings — (a) `pick()` iterates the eligible set twice
(`.count()` then `.collect()`) — defensible for small clusters + matches the 12.1 pattern; (b) the
`cluster_record_response_picks_5xx_detector_on_ties` test only asserts `is_ejected()` (not the
detector choice) and carries a vestigial `let _ = stats_active;` — by-design per the PLAN's
no-deep-stats-seam choice; (c) index-aligned-Vec coupling between `OutlierDetectionState.endpoints`
+ `Cluster.endpoints` + `endpoint_health` enforced by construction not types — matches the existing
12.1 health pattern + documented alignment contract — all deferred to state-5 REVIEW.md; no fold-in.

### Task 5 — D6 `from_bootstrap` stats wiring + BEHAVIOR_CONTRACT rows — `63188e4`

Wired the configured-OD arm in `from_bootstrap` (`cluster.rs`): when `cfg.outlier_detection.is_some()`,
register the 7-name minimum-viable stat subset against the shared `StatsRegistry` — `ejections_active`
(Gauge) + `ejections_enforced_total` / `ejections_detected_consecutive_5xx` /
`ejections_enforced_consecutive_5xx` / `ejections_detected_consecutive_gateway_failure` /
`ejections_enforced_consecutive_gateway_failure` (6 Counters in `EndpointEjectionStats`, shared
across endpoints) + the cluster-level `ejections_overflow` (Counter on `OutlierDetectionState`).
Local `mk_counter`/`mk_gauge` closures DRY the 7 registrations + the
`ClusterError::StatsRegistration { cluster, message }` error mapping. Envoy v3 defaults baked via
`unwrap_or`: thresholds 5/5, `max_ejection_percent` 10 (§6.2 item-1; `interval`/`base_ejection_time`
validator-checked but consumed only at 14.2 sweeper). Build one `EndpointEjection` per endpoint
sharing `stats.clone()`; OD state built from `endpoints.iter()` before the move into the `Cluster`
literal (sound borrow ordering); the production literal now uses the computed `outlier_detection`
variable (`Some` when configured, `None` otherwise). Unconfigured clusters register ZERO
outlier-detection stats (the 21-fixture regression-equivalence gate (b)). Appended a 7-row
`**14.1 entries (outlier detection):**` block + the 14-deferred-names `allowlist_envoy_only`
paragraph to BEHAVIOR_CONTRACT.md (mirrors the 12.1/13.x layout). 4 `#[tokio::test]`s: all-7-present
when configured / none when unconfigured / `ejections_active` reads 0 at construct (§6.2 item-3) /
`max_ejection_percent` defaults to 10 for `outlier_detection: {}`. **One API adaptation (sound):**
test-3 reads the gauge via the idempotent `registry.register_gauge(...).value()` (returns `i64`)
rather than the PLAN's `.parse::<i64>()`, because `StatsRegistry::snapshot()` returns
`Vec<(String, StatHandle-enum)>` not strings — the test asserts snapshot PRESENCE first (defeating
the idempotent-register masking risk) then reads 0. TDD red→green; `cargo test -p envoy-cluster --lib`
63/63 (59 prior + 4 new); clippy clean. Two-stage review: Stage-1 spec-compliance ✅ (all 7
registrations with correct kinds — `ejections_active` Gauge, rest Counters — defaults 5/5/10,
StatsRegistration mapping, computed-not-`None` literal, zero-stats unconfigured path, 4 tests, 7
contract rows + paragraph; no over-build; only 2 files); Stage-2 code-quality APPROVE with 2 **Minor**
optional-polish notes — (a) the new `mk_counter`/`mk_gauge` closures introduce a localized
two-idiom inconsistency with the 5 pre-existing inline per-cluster stat registrations (the closure
approach is the better one; justified by the 7-name OD subset); (b) the `unwrap_or(5/5/10)` Envoy
defaults are single-use magic numbers (adequately documented inline with §6.2 item-1 citations;
named consts would add indirection now but become worthwhile if 14.2 needs the same defaults) — both
deferred to state-5 REVIEW.md; no fold-in.

### Task 6 — D8.2 `parse_bootstrap` fuzz seed `cluster_outlier_detection.yaml` (corpus 21→22) — `5bbb37c`

Added the `parse_bootstrap` fuzz corpus seed exercising the 14.1 outlier-detection schema (all 5
fields at Envoy v3 defaults: consecutive_5xx 5 / consecutive_gateway_failure 5 / interval 10s /
base_ejection_time 30s / max_ejection_percent 10) in ONE commit with the `.gitignore` allow-list
line + the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array entry (the 09/10/11/12.1/13.x
three-files-one-commit lesson). Corpus 21 → 22 success seeds. The two files use DIFFERENT prefix
conventions, both verified against adjacent entries and mirrored exactly: `.gitignore` uses
`!corpus/parse_bootstrap/...`; the SUCCESS array uses `fuzz/corpus/parse_bootstrap/...`. The seed is
git-TRACKED (not ignored), validation-clean (placed in the SUCCESS not reject array). `cargo test -p
envoy-config --lib` 285/285 (incl `fuzz_corpus_seeds_parse_or_reject_cleanly`); clippy clean. Exactly
3 files committed; PROGRESS.md left unstaged. Two-stage review: Stage-1 spec-compliance ✅ (seed YAML
5 fields + valid bootstrap structure, both prefix conventions correct per their own file, tracked,
3-file commit hygiene, no over-build); Stage-2 code-quality APPROVE with 2 **Minor** cosmetic
non-issues — (a) top-level `static_resources:`-before-`admin:` key order differs from
`cluster_circuit_breakers.yaml` (matches `cluster_health_check.yaml`; YAML key order parse-irrelevant);
(b) the `// 14.1 D8.2` trailing comment is the only annotated entry in the SUCCESS array (defensible
provenance tag) — both deferred to state-5 REVIEW.md; no fold-in.

### Task 7 — state-4 phase-done verification + STATE advance — THIS commit

Docs-only (PROGRESS + STATE). The §7.5 (a)–(e) gate was run fresh locally per
`superpowers:verification-before-completion` (evidence below); STATE advanced to state-5-next.
The 6 task commits under verification: Task 1 `0170d1e`, Task 2 `10980b1`, Task 3 `e506b9c`,
Task 4 `09bc34a`, Task 5 `63188e4`, Task 6 `5bbb37c`.

**§7.5 gate evidence (fresh local run at HEAD `5bbb37c` + this docs commit):**
- **(e) 5 stable-toolchain gates — all clean:**
  - `cargo build --workspace --all-targets` → `Finished \`dev\` profile … in 2m 11s` (exit 0).
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` → `Finished`, no warnings.
  - `cargo fmt --all -- --check` → clean (exit 0, no diff).
  - `cargo test --workspace` → **922 passed; 0 failed; 2 ignored** across 76 test binaries (exit 0).
    The 12.1/13.x baseline + the 14.1 new tests (envoy-config 285 + envoy-cluster 63). **Flake
    note:** the FIRST `cargo test --workspace` run surfaced one failure —
    `crates/envoy-bin/tests/upstream_h2_connection_pooling.rs:296` "backend ready:
    ConnectionRefused" — the 13.2 in-process H2-pool backstop whose test spawns a NESTED
    `cargo run --manifest-path …/http2-echo-server` and waits 30s for backend readiness. Under
    the parallel `cargo test --workspace` load the nested cargo contends for cargo's
    build/package-cache lock and exhausted the 30s budget. Confirmed environmental, NOT a 14.1
    regression: (i) 14.1 touched zero envoy-bin / helper / test code and the `pick()` fast-path
    is byte-for-byte unchanged for non-OD clusters (this test configures no `outlier_detection`);
    (ii) the http2-echo-server helper binds in ~1s standalone; (iii) re-run in isolation
    `cargo test -p envoy-bin --test upstream_h2_connection_pooling` → **1 passed in 2.05s**;
    (iv) the full `cargo test --workspace` RE-RUN (everything pre-built) → **922 passed; 0
    failed** with the flake not recurring. This is the documented nested-cargo-under-cargo-test
    contention flake class (kin to the 13.2 C-M3 local-flake note); CI does not see it.
  - `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (the two
    `license-not-encountered` warnings for `Unicode-DFS-2016` / `Zlib` are pre-existing
    unmatched-allowance warnings, not failures).
- **(a)/(b) differential regression-equivalence — GREEN:** `cargo test -p differential --
  --include-ignored` → exit 0, **0 failed, 0 panics**; 110 lib unit tests + all **21
  Docker-gated fixture binaries `... ok`** vs `envoyproxy/envoy:v1.33.0` (Docker 28.0.4; image
  digest `sha256:56da5afd…0c2`): `tcp_proxy`, `tls_downstream`, `tls_sni`, `tls_upstream`,
  `http1_direct_response`, `http1_router_upstream`, `http2_direct_response`,
  `http2_router_upstream`, `admin_ready`, `admin_config_dump_server_info`,
  `admin_drain_listeners`, `admin_stats_prometheus`, `access_log_file_sink`,
  `http_filter_header_mutation`, `http_filter_fault`, `http_filter_local_rate_limit`,
  `http_filter_rbac`, `upstream_active_health_check`,
  `upstream_connection_pooling_and_per_class_counters`, `upstream_h2_connection_pooling`, `echo`.
  This is the load-bearing 14.1 proof: the 21 fixtures stay green simultaneously with the
  outlier-detection machinery present-but-inert (no fixture configures `outlier_detection`, so
  every `pick()` takes the both-filters-None fast path = byte-for-byte phase-02 round-robin).
  Notably both documented-flake-prone binaries (`access_log_file_sink`,
  `upstream_h2_connection_pooling`) ran green this pass.
- **(c) h2spec ≥95% — held vacuously:** 14.1 touched zero H2-framing code (only envoy-config,
  envoy-cluster, docs, fuzz corpus); the parent-05 baseline 99.31% is unaffected. No local re-run.
- **(d) parse_bootstrap fuzz on the 22-seed corpus — clean:** `cargo +nightly fuzz run
  parse_bootstrap --fuzz-dir crates/envoy-config/fuzz -- -runs=200000` (cargo-fuzz 0.13.1) →
  `Done 200000 runs in 19 second(s)`, 0 crashes, exit 0.

**Carryforward:** 14.1 engaged NO carryforward (lock-in #24). The 13.2 A-M2 stale-comment is
unmoved (14.1 does NOT touch envoy-http1; A-M2 close opportunity defers to 14.2 D4). The inherited
multi-phase Minor carryforward inventory carries forward UNCHANGED. The state-3 two-stage review
surfaced ONLY Minor findings across Tasks 1-6 (no Important/Critical, no fold-ins) — all collected
for the state-5 REVIEW.md disposition. NO ADR landed across the entire 14.1 state-3 arc (lock-in
#2 held; DECISIONS.md ledger head stays ADR-0041). One PLAN-code-vs-clippy reconciliation at Task 3
(`502 | 503 | 504` → `502..=504` for `manual_range_patterns`; behavior-identical).

**Next:** state 5 — `superpowers:requesting-code-review` over the range `0170d1e..5bbb37c`
(the 6 task commits) → `REVIEW.md`.
