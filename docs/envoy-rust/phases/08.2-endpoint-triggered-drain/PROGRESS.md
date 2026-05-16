# Phase 08.2 (`08.2-endpoint-triggered-drain`) — PROGRESS

> Per-task narrative log; appended at every task commit. Created at the state-2 standalone-PLAN commit (this commit) with the Task 1 preamble below; Tasks 1-10 land at state-3; Task 11 is the state-4-reached / state-5-next commit. Mirrors the 08.1 (`7dbd984`) / 07.2 (`c7dea4c`) / 06.3 (`3a964cc`) / 06.2 (`dc00750`) PROGRESS-skeleton-lands-at-state-2 cadence.

---

## Task 1 preamble (lands alongside PLAN.md + this PROGRESS.md skeleton at the state-2 standalone-PLAN commit)

### State-2 commit context

- **Predecessor:** `3ed6af0` — `phase 08.1: admin endpoint surface (config_dump, server_info, clusters, listeners) + 06.1 carryforward closures (I2, M1, M4)` (08.1 state-6 close-out commit; immediate predecessor / former HEAD of `origin/main`).
- **08.2 SPEC:** committed at `56dee82` (parent-08 state-2 split commit) — 328 lines, 10 sections.
- **08.2 state-1 brainstorm: SKIPPED** per 08.2 SPEC §8 explicit authorization (the parent-08 state-2 split commit `56dee82` already brainstormed the 08.2 slice as a carve-out of the parent-08 state-1 SPEC at `0202e38`; sibling-skip-state-1 pattern mirrors parent-04 / parent-05 / parent-06 / parent-07 inter-sub-phase precedent).
- **ROADMAP:** row `08.2` flips `planned` → `in-progress` at THIS commit per `BOOTSTRAP_PROMPT.md` §4.1 invariant 3. Parent row `08` stays `in-progress` (closing-sub-phase invariant — flips done only at 08.2's state-6 commit). Sibling row `08.1` stays `done`.
- **STATE.md:** advances from `08.2 state 2 (SPEC.md only)` to `08.2 state 3 (SPEC + PLAN exist; implementation incomplete)`; next-skill `superpowers:writing-plans` → `superpowers:subagent-driven-development` per the user's standing preference (auto-memory `feedback_execution_style`).
- **DECISIONS.md:** UNCHANGED. Ledger head stays **ADR-0032** (parent-08 state-2 split decision; landed at `56dee82`). No ADR at a state-2 PLAN-write commit per the established no-foundations-grants posture. ADR-0033 stays reserved-available for execution-time landings if reality forces it (per 08.2 SPEC §7 + parent-08 SPEC §5.3 — recommended posture is no new ADRs).
- **BEHAVIOR_CONTRACT.md:** UNCHANGED at this commit. The 3 new "Admin endpoint body shapes" rows (POST endpoints) + 3 new "Stat-name mapping" rows (drain gauges) + new "Admin-action effect equivalence" subsection + the `/server_info` row note patch all land at execution-time task commits per the 06.x / 07.x / 08.1 doctrine (Task 2 lands the stat-name rows; Task 3 lands the body-shape rows + /server_info row patch; Task 8 lands the Admin-action effect equivalence subsection).

### PLAN scope summary

- **11 tasks; ~1375 LoC projected** (production ~515; tests ~600; fixture/doc ~260). Comfortably under the §6.1 ~25-task gate; ~+45% over the SPEC §6.1 ~920 upper LoC projection but concentrated in test + fixture material — production-side LoC ~515 matches the SPEC's implied production target.
- **No nested split** per parent-08 SPEC §6.1 alternative (vi) + the standardized accept-drift-on-test-heavy-projections posture across 08.1 / 07.x / 06.x.
- **Recommended execution order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11.** Tasks 7 + 9 are fully independent of Tasks 1-6 and may dispatch concurrently. Tasks 3 + 5 + 6 are mutually parallelizable after Task 4 lands.

### PLAN-write SPEC corrections (6)

Recorded fully in `PLAN.md` "PLAN-write SPEC corrections" section above. Summary:

1. `AdminHandler::new` current shape is 6-arg (not 5-arg as the SPEC §3 D13b text implies); 08.2's D13b widens 6-arg → 7-arg.
2. `AdminEndpoint::allowed_method` per-variant declaration shape already in place from 08.1 D4 — 08.2 additively declares `"POST"` on the 3 new variants.
3. `AdminEndpoint::render_with(&handler)` dispatch path already in place from 08.1 D6 — 08.2 additively extends the match arms.
4. `Listener::serve` current signature is 1-arg `(shutdown)`; 08.2's D12 widens 1-arg → 2-arg with `drain: Arc<DrainState>`.
5. `Listener::bind` already takes the registry; 08.2's D14 reuses the same registry to idempotently register the shared `listener_manager.total_listeners_active` gauge alongside the existing per-listener gauges.
6. The admin path + the echo path both use `TcpListener::bind` + a custom serve fn (not `Listener::bind`); they are naturally excluded from the new `listener_manager.total_listeners_active` gauge AND from drain observation per parent-08 SPEC §5.5.

### Architecture-decision lock-ins (30)

Recorded fully in `PLAN.md` "Architecture decisions locked at PLAN-write time" table (30 rows). Summary of the load-bearing picks:

- **#1-#7** — D11 ordering (Task 1 preamble); DrainState placement at `envoy-listener::drain` with `envoy-admin` re-export; DrainState::new(&Arc<StatsRegistry>) shape with gauge fields; compare_exchange + notify-once-on-CAS-success drain semantics; drain_signal() returns Box<dyn Future + Send> with already-Draining early-return; DrainStage `#[repr(u8)]` discriminant load-bearing for server.state gauge value; the 6 DrainState unit tests.
- **#8-#11** — D9/D10 in one task; D5e + D-ready in one task; /server_info state-binding swap shape; /ready drain-aware body strings ("LIVE\n" / "Service Unavailable\n" / "DRAINING\n").
- **#12-#17** — listener_manager.total_listeners_active gauge scope (data-plane only; echo + admin paths excluded); idempotent re-registration in Listener::bind; RAII guard pattern at Listener::serve entry/exit; Listener::serve 2-arg widening shape; the listener drain unit test; D13b Arc<DrainState> wiring in envoy-bin.
- **#18-#20** — D16 field order (pre_admin_actions BEFORE pre_requests in YAML; dispatch temporal sequence is pre_requests → pre_admin_actions → scrapes → post_admin_assertions per the Task-8-surfaced correction); AdminAction + AdminAssertion enum shapes (within_ms u64, NOT humantime); DataPlaneConnectionRefused assertion semantics (ECONNREFUSED OR immediate-EOF either accepted).
- **#21-#23** — 08.1 REVIEW M2 + M4 closures folded into Task 7; 08.1 REVIEW process-note pick — option (b) trivial-echo-filter workaround documented for future admin-only backstops + Task 10 itself uses HCM + direct_response shape (Task 10 architecture deviation #1).
- **#24** — BEHAVIOR_CONTRACT row landing cadence per task (stat-name rows at Task 2; body-shape rows at Task 3; Admin-action effect equivalence subsection at Task 8).
- **#25-#28** — pre-state-4 fmt discipline; state-4 evidence-discipline; Cargo.lock cadence; no new ADRs.
- **#29-#30** — PROGRESS.md cadence; `#![forbid(unsafe_code)]` carries forward unchanged (no new crate roots).

### Carryforward dispositions

| ID | Severity | Item | Disposition at 08.2 |
|---|---|---|---|
| **08.1 REVIEW M2** | Minor | `BodyRule::JsonShape::value_may_differ_keys` field-level doc-comment | **CLOSED at Task 7 (D16).** One-line doc-comment added co-located with the `Driver::AdminScrape` extension. The chain ends. |
| **08.1 REVIEW M4** | Minor | `walk_pointer` opaque error on dotted path with empty segment | **CLOSED at Task 7 (D16).** 3-line guard at function head produces structured error naming the offending path. The chain ends. |
| **08.1 REVIEW M1** | Minor | Fixture 0014 README doc-drift | **CLOSED at 08.1 state-6 close-out commit `3ed6af0`** per REVIEW.md §4 disposition. The chain ended before 08.2 began. Do NOT re-engage. |
| **08.1 REVIEW M3** | Minor | Forward-looking `Arc<BTreeMap<...>>` wrap on `command_line_options` | **Carry forward indefinitely.** Activates only if a future deployment widens the CLI surface beyond `-c <config>`. Not 08.2's owner. |
| **08.1 REVIEW §4 process note** | Process | `filter_chains: []` schema-vs-runtime inconsistency | **Option (b) picked + recorded in lock-in #23.** Trivial-echo-filter workaround documented for FUTURE admin-only backstops; Task 10 itself uses HCM + direct_response shape per Task 10 architecture deviation #1 (Task 10's backstop needs a real HCM data-plane listener, so the workaround is not directly applicable). Option (a) (parse-time validator extension) deferred to a future hardening phase. |
| **07.2 REVIEW M1 / M2 / M3** | Minor | `HttpFilterInstance::build` position plumbing / Overwrite O(n²) YAGNI / fixture-0013 expected_body coupling | **Carry forward unchanged** per 07.2's named-owner dispositions. 08.2 does not engage. |
| **06.3 REVIEW I2** | Important | Synthetic 5xx backend + 4-class pre_requests deferred from 06.3 | **Carry forward unchanged.** 08.2 introduces no synthetic-backend infrastructure; upstream-robustness family is still the natural close site. |
| **06.2 REVIEW M1 / M2 / M4 / M5** | Minor | Various 06.2 access-log polish items | **Not engaged by 08.2; carry forward unchanged.** |
| **06.1 REVIEW M2 / M3 / M5 / M6** | Minor | Various 06.1 stats/admin polish items | **Carry forward indefinitely.** Not 08.2's owner. |
| **05.3 REVIEW I2** | Important | Typed-error chain dissolution at H2 dispatch site | **Carry forward unchanged.** 08.2 does not re-engage the H2 dispatch site. |
| **05.2 REVIEW I1 / I2 / I3** | Important | CI h2spec SHA / Http2Error variant rename / MalformedH2HeaderBlock split | **Not engaged by 08.2; carry forward unchanged.** |
| **04.1 REVIEW M5 / M9** | Minor | Cargo.lock cadence ratification ADR | **Carry forward unchanged.** 08.2 introduces zero new top-level Cargo deps; cadence pick stays unforced. |
| **02.2 REVIEW M1** | Minor | `*EchoBackend::Drop` polling loop blocks on `std::thread::sleep` | **Standing carryforward.** Fixture 0015 reuses the existing data-plane backend helpers unchanged. |

**No new carryforwards generated by the 08.2 state-2 PLAN-write that gate any future phase.**

### PLAN-write deviations from SPEC

Beyond the 6 SPEC corrections above, the PLAN materialized **2 deviations** worth recording:

1. **PLAN architecture-decision lock-in #18 + Task 8 PLAN-write correction:** the SPEC §3 D16 enumerates the field order as `pre_admin_actions, pre_requests, ..., post_admin_assertions` but does NOT prescribe the temporal dispatch sequence. PLAN lock-in #18 settles the temporal sequence at `pre_requests → pre_admin_actions → scrapes → post_admin_assertions` (NOT the inverse) because fixture 0015's natural shape is "verify pre-drain baseline → drain → verify post-drain state → wire-level assertion." The Task 7 dispatch fn body honors this temporal order regardless of YAML field order.

2. **PLAN Task 10 architecture deviation #1:** the SPEC §3 D17.4b implies the backstop uses the trivial-echo-filter workaround (per the 08.1 process-note carry); the PLAN settles on HCM + direct_response shape for Task 10's backstop instead, because Task 10's backstop NEEDS a real data-plane listener to verify the drain rejection (echo-filter listeners don't bind a real HTTP-speaking data plane). The trivial-echo-filter workaround is reserved for future admin-only backstops where no data-plane listener is needed. Updated lock-in #23 in this PROGRESS preamble to reflect the scope split.

### Differential surface at state-2 (this commit)

None. State-2 is docs-only (PLAN.md + PROGRESS.md skeleton + ROADMAP row flip + STATE.md advance + Task 1 preamble). All 14 Docker-gated fixtures (0001-0014) remain GREEN simultaneously per the 08.1 state-4 anchor CI run `25964680619` HEAD `03e6435`; no CI re-run is gated for a docs-only commit per the established docs-only convention. The state-4 CI re-run lands at Task 11 (which becomes the 08.2 state-4 evidence anchor; ~10 task commits from now).

### State-3 entry routing (next session)

Per `BOOTSTRAP_PROMPT.md` §5 state 3 + STATE.md's advance at this commit: next session enters **08.2 lifecycle state 3** with next-skill `superpowers:subagent-driven-development` scoped to this PLAN.md. The session-1 dispatch executes Task 1 (D11 DrainState foundation) per the recommended order.

---

*End of Task 1 preamble. Task 1 narrative appends below at the first state-3 commit (the D11 DrainState foundation task). Subsequent tasks (2-11) each append their per-task narrative + the 5-gate test-bucket attestation including verbatim `cargo deny check` output per the 07.1-REVIEW doctrine reminder.*
