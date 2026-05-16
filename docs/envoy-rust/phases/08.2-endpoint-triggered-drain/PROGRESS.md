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

---

## Task 1 (D11 — DrainState foundation)

### Work summary

Landed the shared drain-state primitive at `crates/envoy-listener/src/drain.rs`: `DrainStage` enum (`Live=0`, `HealthcheckFailing=1`, `Draining=2` — `#[repr(u8)]` discriminant load-bearing per parent-08 SPEC §2.3 + 08.2 SPEC §2.2, matches the future `server.state` gauge value at Task 2) + `DrainStage::from_u8` round-trip helper + `DrainState` struct over `AtomicU8 + tokio::sync::Notify` + 5 public methods (`new`, `current`, `fail_healthcheck`, `ok_healthcheck`, `drain`, `drain_signal`) + a `Default` impl that forwards to `new()`. Every mutator uses `compare_exchange` (not unconditional `store`) so the sticky-`Draining` and idempotent self-loop semantics fall out for free; `drain()` calls `notify.notify_waiters()` exactly once on the first successful CAS via two sequenced CAS attempts (`Live → Draining` then `HealthcheckFailing → Draining` only if the first failed) — repeat `drain()` calls fail both CASes silently and do NOT re-notify. `drain_signal()` returns a `Pin<Box<dyn Future<Output = ()> + Send + '_>>` and short-circuits to `std::future::ready(())` when state is already `Draining` (no waiter registration on the already-drained path; idempotent + re-entrant).

Registered the module in `crates/envoy-listener/src/lib.rs` (insertion between the existing crate-root module doc-comment and the first `use std::net::SocketAddr;`) with `pub mod drain;` + `pub use drain::{DrainStage, DrainState};`. Added `pub use envoy_listener::{DrainStage, DrainState};` to `crates/envoy-admin/src/lib.rs` so admin call sites at later tasks read as `use envoy_admin::DrainState;` per parent-08 SPEC §5.1's Cargo-cycle resolution doctrine (placement note + 05.3 / 07.1 / D3 precedent documented in the module doc-comment at the top of `drain.rs`).

NO gauge wiring at this task per PLAN architecture-decision lock-in #3 — the SPEC §6.4 split is foundation-first / stats-second. `DrainState::new()` takes no arguments here; Task 2 widens it to take `&Arc<envoy_stats::StatsRegistry>` for the `server.live` / `server.state` / `listener_manager.total_listeners_active` registration call.

### Tests landed (6)

All 6 colocated in `drain::tests` at the bottom of `crates/envoy-listener/src/drain.rs`:

1. **`new_returns_live`** — fresh `DrainState::new()` reads `DrainStage::Live`.
2. **`drain_flips_to_draining_and_notifies_waiters_once`** (multi-thread tokio) — registers two waiters via `drain_signal()`, calls `drain()` once, asserts both waiters complete within 1s AND a NEW post-drain `drain_signal()` is immediately ready (already-`Draining` early-return path).
3. **`fail_healthcheck_flips_to_healthcheck_failing`** — `Live → HealthcheckFailing` CAS transition.
4. **`ok_healthcheck_restores_to_live`** — `HealthcheckFailing → Live` CAS transition.
5. **`ok_healthcheck_after_drain_is_noop_sticky`** — sticky-drain per parent-08 SPEC §5.6: `ok_healthcheck()` after `drain()` leaves state at `Draining`.
6. **`repeat_drain_calls_are_idempotent`** (multi-thread tokio) — second + third `drain()` calls do not panic and leave state at `Draining`; a waiter registered AFTER any `drain()` call completes immediately.

Red phase (Step 2) verified: pre-implementation `cargo test -p envoy-listener --lib drain::tests` produced 17 errors (`E0432: unresolved imports \`drain::DrainStage\`, \`drain::DrainState\`` at `lib.rs:12` from the module-registration line + 16 × `E0433: cannot find type \`DrainState\` / \`DrainStage\` in this scope` across the six test bodies). Green phase (Step 4) verified: `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out; finished in 0.06s`.

### Per-task deviations from PLAN

**Zero structural deviations.** The `Box::pin` shape for `drain_signal()` (vs. the unsafe pin-projection sketch that appeared earlier in PLAN.md) is what PLAN.md lines 623-634 explicitly directed the implementer to use — this is a PLAN-followed-the-explicit-`Box::pin`-direction, not a deviation. The `#![forbid(unsafe_code)]` crate-root attribute (`crates/envoy-listener/src/lib.rs:1`) makes that the only viable shape regardless. The `std::task::{Context, Poll}` imports that appeared in PLAN's unsafe pin-projection sketch were correspondingly NOT included here (they would be unused with the `Box::pin` shape) — this is consistent with PLAN's authored direction and is documented here for completeness.

**One incidental fmt adjustment**: the initial append of `pub use envoy_listener::{DrainStage, DrainState};` to `crates/envoy-admin/src/lib.rs` placed the re-export at the bottom (per the task text directive "append after that line"), but `cargo fmt` requires the `pub use` block to be alphabetically ordered — moved the re-export from line 19 to line 17 (between `pub use endpoint::AdminEndpoint;` and `pub use error::AdminError;`) so the block sorts as `AdminConfig`, `AdminEndpoint`, `envoy_listener::{…}`, `AdminError`, `handler::{…}`. Functionally identical; recorded here so the deviation is documented.

### LoC delta

| File | Production lines | Test lines |
|---|---|---|
| `crates/envoy-listener/src/drain.rs` (new) | 202 (module doc-comment + `DrainStage` enum + `DrainState` struct + 5 methods + `Default` impl) | 106 (6 unit tests in `mod tests`) |
| `crates/envoy-listener/src/lib.rs` | +3 (blank line + `pub mod drain;` + `pub use drain::{DrainStage, DrainState};`) | 0 |
| `crates/envoy-admin/src/lib.rs` | +1 (`pub use envoy_listener::{DrainStage, DrainState};`) | 0 |
| **Totals** | **+206** | **+106** |

Total delta: **+312 LoC** (drain.rs 308 lines + 4 re-export lines). Within the ~50-LoC envelope projected by PLAN.md §Task 1 for the production-side surface (the gap is in the docstring-heavy module header + per-method doc-comments; raw code is ~80 lines).

Workspace `envoy-listener` unit-test bucket grew from 12 → 18 tests (delta +6, exactly the 6 new tests in `drain::tests`).

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff).

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean compile across all 8 workspace crates, zero warnings).

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 8 crates compiled).

**Gate 4 — `cargo test --workspace`:** PASS (exit 0; every per-bucket `test result:` line reads `ok. N passed; 0 failed`; total workspace test count delta is exactly +6 in the `envoy-listener` lib bucket which grew from 12 → 18 tests — the new `drain::tests::{new_returns_live, fail_healthcheck_flips_to_healthcheck_failing, ok_healthcheck_restores_to_live, ok_healthcheck_after_drain_is_noop_sticky, drain_flips_to_draining_and_notifies_waiters_once, repeat_drain_calls_are_idempotent}`).

**Gate 5 — `cargo deny check`:** PASS (exit 0). Verbatim output:

```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

The 5 `license-not-encountered` warnings are pre-existing across the project (the `deny.toml` allow-list is broader than the workspace's transitive dependency tree); the verdict line `advisories ok, bans ok, licenses ok, sources ok` is the gate-pass signal. Identical posture to 08.1 / 07.x / 06.x precedent. Quoted verbatim per the 07.1-REVIEW doctrine (no "assumed no-op" handwave).

### Differential surface delta

**None.** Task 1 lands the `DrainState` foundation only — no admin endpoint surface change, no listener-serve signature change, no gauge registration, no fixture. All 14 Docker-gated fixtures (0001-0014) remain GREEN by construction (zero changed wire-protocol behavior); the 08.1 state-4 anchor CI run `25964680619` HEAD `03e6435` remains the authoritative bridge-CI evidence. The new differential fixture (0015) lands at Task 8; the state-4 CI re-run lands at Task 11.

---

## Task 1 fixup (review-driven; lands on top of `c1c9604`)

### Fixup driver

Code-quality review of Task 1 commit `c1c9604` (the D11 DrainState foundation) flagged 1 Critical + 3 Important findings against `crates/envoy-listener/src/drain.rs`. This fixup commit closes all 4. The 8 Minor findings from that review (`DrainStage::from_u8` pub→pub(crate); `panic!` → `unreachable!`; `Err(0)` sentinel readability; `Default` impl Task-2-anticipation widening; `Hash` derive on `DrainStage`; etc.) are deferred per the project's close-opportunistically doctrine — none gate Task 2 and they accumulate naturally into a future minor-polish pass.

### What changed

**1 Critical (TOCTOU race in `drain_signal()`) closed.** Pre-fix `drain_signal()` sequenced `state.load(Acquire) → (race window) → self.notify.notified()`. A concurrent `drain()` firing in the window — CAS state to `Draining`, then `notify_waiters()` — would bump the notify counter; the subsequently-constructed `Notified` would snapshot the post-bump counter at construction time; on first poll, `poll_notified`'s counter-comparison would equal-out (snapshot == current) and fall through to register a waiter that never unparks (sticky-drain idempotency means no second `notify_waiters()` ever fires). Tokio's `Notify::notified()` docs make construction (not first poll) the race-free anchor: "The `Notified` future is guaranteed to receive wakeups from `notify_waiters()` as soon as it has been created." Fix inverts the order: construct `notified` FIRST (anchoring the snapshot before any racer can bump the counter), then load state. If already `Draining`, discard the unpolled `Notified` (Drop on an unpolled `Notified` is safe — no registration has occurred) and return `std::future::ready(())`. Otherwise return `Box::pin(notified)`.

**Why this matters now:** Task 6 (D12 listener observation, scheduled ~5 commits from now) calls `drain_signal()` from `Listener::serve`'s accept-loop `tokio::select!` — the structurally worst-case exposure point because the listener re-enters the call on every loop iteration concurrent with the admin thread that fires `drain()`. The race must be closed before Task 6 lands.

**3 Important closed:**

- **Important #1** — Test 2 (`drain_flips_to_draining_and_notifies_waiters_once`) rewritten to use a 3-party `tokio::sync::Barrier` instead of a 50ms sleep. The sleep was flake-prone on busy CI (past phases have shown 100-200ms scheduling jitter on macOS runners) and didn't actively verify registration — it just gave a budget. The 3-party Barrier (the two waiters + the main task) explicitly anchors each waiter's `drain_signal()` construction at the barrier rendezvous before the main task fires `drain()`.
- **Important #2** — NEW Test 7 (`drain_signal_is_race_free_with_concurrent_drain`) — regression test for the Critical race. Spawned task constructs the signal future BEFORE the 2-party Barrier rendezvous; main task fires `drain()` AFTER the rendezvous. Without the Critical fix this test deterministically hangs; with the fix it completes in <1s. Comment header explicitly references the fix shape so a future-reader sees the regression coverage chain.
- **Important #3** — NEW Test 8 (`drain_from_healthcheck_failing_notifies_waiters_once`) — exercises the second-CAS branch of `drain()` end-to-end (Live→Draining CAS fails because state is HealthcheckFailing; HealthcheckFailing→Draining CAS succeeds; `notify_waiters()` still fires exactly once). Previously only Test 2 covered drain-from-Live; the second-CAS branch had no end-to-end coverage.
- **Important #4** — NEW Test 9 (`fail_healthcheck_after_drain_is_noop_sticky`) — symmetric to Test 5's `ok_healthcheck()` post-drain assertion. Verifies `fail_healthcheck()` post-drain leaves state at `Draining` (`compare_exchange` for Live→HealthcheckFailing silently fails on Draining current value).

### Diff summary

| File | Lines touched |
|---|---|
| `crates/envoy-listener/src/drain.rs` — `drain_signal()` body | replaced lines 187-194 (8 lines) with the race-free shape (20 lines incl. extended comment block — the comment block expansion is required per the fix's source-address-stability directive) |
| `crates/envoy-listener/src/drain.rs` — Test 2 body | replaced internals (kept test name); +20 lines for the Barrier construction + waiter spawn rewrite |
| `crates/envoy-listener/src/drain.rs` — Tests 1, 3, 4, 5, 6 comment headers | renumbered "Test N of 6" → "Test N of 9" (5 single-line edits) |
| `crates/envoy-listener/src/drain.rs` — Test 7 (NEW) | +44 lines including doc-comment |
| `crates/envoy-listener/src/drain.rs` — Test 8 (NEW) | +28 lines |
| `crates/envoy-listener/src/drain.rs` — Test 9 (NEW) | +14 lines |

Test count delta: `drain::tests` 6 → 9 (+3). `envoy-listener` lib bucket: 18 → 21 (+3). Workspace test count delta from the pre-Task-1 baseline now stands at **+9** (Task 1 commit landed +6; this fixup adds +3).

No changes to `lib.rs` (envoy-listener or envoy-admin). No new top-level Cargo deps — `tokio::sync::Barrier` rides on the existing `tokio` `sync` feature already declared at `crates/envoy-listener/Cargo.toml:16` + `:21`. Crate root still carries `#![forbid(unsafe_code)]`.

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff).

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean compile across all 8 workspace crates, zero warnings).

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 8 crates compiled).

**Gate 4 — `cargo test --workspace`:** PASS (exit 0; every per-bucket `test result:` line reads `ok. N passed; 0 failed`; the `envoy-listener` lib bucket grew from 18 → 21 tests — the 3 new `drain::tests::{drain_signal_is_race_free_with_concurrent_drain, drain_from_healthcheck_failing_notifies_waiters_once, fail_healthcheck_after_drain_is_noop_sticky}`). Focused re-run on the drain module: `cargo test -p envoy-listener --lib drain::tests -- --nocapture` reads `test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out; finished in 0.00s`.

**Gate 5 — `cargo deny check`:** PASS (exit 0). Verbatim output:

```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

The 5 `license-not-encountered` warnings are pre-existing across the project (the `deny.toml` allow-list is broader than the workspace's transitive dependency tree); the verdict line `advisories ok, bans ok, licenses ok, sources ok` is the gate-pass signal. Quoted verbatim per the 07.1-REVIEW doctrine (no "assumed no-op" handwave).

### Differential surface delta

**None.** This fixup is logic + test changes inside `drain.rs` only — no admin endpoint surface change, no listener-serve signature change, no gauge registration, no fixture. All 14 Docker-gated fixtures (0001-0014) remain GREEN by construction (zero changed wire-protocol behavior); the 08.1 state-4 anchor CI run `25964680619` HEAD `03e6435` remains the authoritative bridge-CI evidence until the 08.2 state-4 anchor at Task 11.

---

## Task 2 (D14 — gauge registrations + `DrainState` wiring)

### Work summary

Widens `DrainState::new` to take `&Arc<envoy_stats::StatsRegistry>`; registers `server.live` + `server.state` gauges via the registry at construction; stores both as `Arc<envoy_stats::Gauge>` fields on `DrainState`; updates the gauges inline at every state-transition CAS-success site in `fail_healthcheck` / `ok_healthcheck` / `drain` (one source of truth — gauges are NEVER polled). Additionally registers the shared third gauge `listener_manager.total_listeners_active` inside `Listener::bind` (idempotent same-name re-registration across multiple `bind` calls returns the same `Arc<Gauge>` per Task 5's contract); stores the `Arc<Gauge>` as a 6th field on `Listener`; exposes a `pub(crate) fn listener_manager_active()` accessor for Task 6's RAII hoist (Task 6 lands the actual inc/dec wiring at `serve` entry/exit; Task 2 only registers + stores).

Per 08.2 SPEC §2.2: `server.live` emits `1` ONLY when `current() == Live`; HealthcheckFailing AND Draining BOTH emit `0`. Gauge writes inside `drain()` happen BEFORE `notify.notify_waiters()` so a just-woken waiter that reads the registry sees the post-drain values (load-bearing ordering).

### Tests landed (7 new tests by name)

**`drain::tests::*` (5 new — total 9 → 14):**

1. `new_registers_server_live_gauge` — fresh `DrainState::new(&registry)` registers the `server.live` key in the snapshot.
2. `new_registers_server_state_gauge` — fresh `DrainState::new(&registry)` registers the `server.state` key.
3. `new_initial_gauge_values_are_live` — initial values `server.live = 1`, `server.state = 0` (Live discriminant) per 08.2 SPEC §2.2.
4. `drain_updates_server_live_to_zero_and_server_state_to_two` — `drain()` flips both gauges at the CAS-success site (Live → Draining; gauges read 0 / 2).
5. `fail_healthcheck_sets_server_live_to_zero_and_server_state_to_one` — `fail_healthcheck()` flips both gauges (Live → HealthcheckFailing; gauges read 0 / 1). **Corrects the PLAN's first-draft test name `_keeps_server_live_at_one`** — the SPEC §2.2 invariant is "`server.live = 1` ONLY when `Live`", so the HealthcheckFailing case MUST emit `0`, not keep at `1` (the PLAN.md self-correction at line 989 spells this out).

**`tests::*` (2 new — total 11 → 13):**

6. `bind_registers_listener_manager_total_active_gauge` — `Listener::bind` registers `listener_manager.total_listeners_active` against the shared registry.
7. `bind_listener_manager_gauge_is_idempotent_shared` — two `Listener::bind` calls against the same registry produce exactly one shared gauge entry in the snapshot (per Task 5's idempotent same-name re-registration contract).

### Per-task deviations from PLAN

- **PLAN sketch `let snapshot: BTreeMap<_, _> = registry.snapshot().collect();` rewritten** to `let snapshot: BTreeMap<_, _> = registry.snapshot().into_iter().collect();` in all 5 new drain tests + both 2 new lib tests. `StatsRegistry::snapshot()` returns `Vec<(String, StatHandle)>`, not an iterator; the PLAN sketch as-written would not compile. Mirrors the existing `crates/envoy-stats/src/registry.rs:188` test pattern.
- **PLAN's `NoopHandler` test stub replaced with the existing `NullHandler`** at `crates/envoy-listener/src/lib.rs:470` (the no-op connection handler already in scope from Task 1's bind-side tests). The PLAN named the stub `NoopHandler`; the actual codebase has the equivalent under the name `NullHandler`.
- **PLAN's first-draft test `fail_healthcheck_keeps_server_live_at_one` REPLACED with the SPEC-correct `fail_healthcheck_sets_server_live_to_zero_and_server_state_to_one`** asserting `g.value() == 0` for `server.live` in the HealthcheckFailing case. This is the PLAN's own self-correction at PLAN.md line 989 — the SPEC §2.2 invariant is "`server.live = 1` ONLY when `current() == Live`", which HealthcheckFailing does NOT satisfy.
- **`Default for DrainState` impl removed** (not just stubbed) — the new `DrainState::new(&Arc<StatsRegistry>)` signature requires a registry argument, which `Default::default()` cannot synthesize. Replaced the impl block with a tombstone comment pointing at the Task 2 cause. This closes Minor finding #4 from the Task-1 code-quality review as a forced-removal rather than an opportunistic cleanup.
- **`Listener::listener_manager_active` field + accessor decorated with `#[allow(dead_code)]`** at Task 2 because both are registered + stored but not yet consumed (Task 6 (D12) hoists the gauge into the RAII guard at `serve` entry/exit). The allows are scoped to the two declarations and carry inline comments referencing Task 6's removal of the allow. Without this, `cargo clippy -D warnings` flags `field never read` + `method never used`.
- **The 9 existing Task-1 tests had their `DrainState::new()` calls swept** to `DrainState::new(&mk_registry())` via a new module-local `fn mk_registry() -> Arc<envoy_stats::StatsRegistry>` helper at the top of `drain::tests` (parallels the `lib.rs` test helper of the same name — same semantics).

### LoC delta

| File | Insertions | Deletions |
|---|---|---|
| `crates/envoy-listener/src/drain.rs` | +169 | -19 |
| `crates/envoy-listener/src/lib.rs` | +86 | -3 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | +6 | 0 |
| **Total source + doc:** | **+261** | **-22** |

Test-count delta: `envoy-listener` lib bucket grew **21 → 28** (+7 = 5 new `drain::tests::*` + 2 new `tests::*`); workspace total grew by the same +7 (no other crate touched).

No new top-level Cargo deps — `envoy_stats` was already a `[dependencies]` entry on `envoy-listener` (used since 06.1 for `cx_total` + 06.3 for `cx_active` / `cx_accept_failed`). Crate root still carries `#![forbid(unsafe_code)]`; zero new unsafe blocks.

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff after one `cargo fmt --all` apply during implementation — a single line in the new bind test's `let snapshot` line wrapped vs. rustfmt's preferred single-line shape).

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean compile across all 8 workspace crates, zero warnings). The two `#[allow(dead_code)]` annotations on `Listener::listener_manager_active` field + accessor are required at Task 2 (the gauge is registered + stored but not consumed until Task 6's RAII hoist).

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 8 crates + 2 helper bin crates compiled).

**Gate 4 — `cargo test --workspace`:** PASS (exit 0; every per-bucket `test result:` line reads `ok. N passed; 0 failed`; the `envoy-listener` lib bucket grew from 21 → 28 tests). Focused re-run: `cargo test -p envoy-listener --lib drain::tests` reads `14 passed; 0 failed`.

**Gate 5 — `cargo deny check`:** PASS (exit 0). Verbatim output:

```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

The 5 `license-not-encountered` warnings are pre-existing (`deny.toml` allow-list broader than the transitive tree); the verdict line `advisories ok, bans ok, licenses ok, sources ok` is the gate-pass signal. Quoted verbatim per 07.1-REVIEW doctrine.

### Differential surface delta

**None at the differential / fixture surface.** Gauges register but no fixture asserts their values yet — fixture 0015 at Task 8 (the new differential fixture for `/drain_listeners`) is the first to assert `server.state = 2` post-drain. All 14 Docker-gated fixtures (0001-0014) remain GREEN by construction (zero changed wire-protocol behavior); the 08.1 state-4 anchor CI run `25964680619` HEAD `03e6435` remains the authoritative bridge-CI evidence until the 08.2 state-4 anchor at Task 11.

The `BEHAVIOR_CONTRACT.md` "Stat-name mapping" section gains 3 new rows under a new `**08.2 entries (drain machinery):**` subheading inserted between the existing `**06.3 entries:**` table and the `**06.1 Prometheus exposition shape divergence (06.1 fixture 0011):**` callout — mirrors the per-phase subheading cadence the 06.1 → 06.3 entries already established.

---

## Task 3 (D9 + D10 — three POST admin endpoints)

### Work summary

Adds 3 new variants to `AdminEndpoint` (`DrainListeners`, `HealthcheckFail`, `HealthcheckOk`) each declaring `allowed_method() = "POST"`; extends `AdminEndpoint::from_path` with their 3 path arms (`/drain_listeners`, `/healthcheck/fail`, `/healthcheck/ok`); extends `AdminEndpoint::render` with 3 new `unreachable!()` arms (the registry-only render path carries no `DrainState`); extends `AdminEndpoint::render_with` with 3 new `todo!()` arms — gated until Task 4 (D13b) widens `AdminHandler::new` from 6-arg to 7-arg and exposes the `handler.drain()` accessor. Lands 3 new render fns at module scope (`render_drain_listeners` / `render_healthcheck_fail` / `render_healthcheck_ok`) + a shared `empty_200_ok()` helper returning `Response { status: 200, reason: Some("OK"), headers: [("content-length", "0")], body: Bytes::new() }`. Each render fn takes `drain: &envoy_listener::DrainState` and invokes the corresponding `DrainState` method before producing the 200 OK empty body.

The 9 colocated unit tests at `crates/envoy-admin/src/endpoint.rs::drain_admin_tests` exercise the render fns directly via `render_drain_listeners(&drain)` etc. — they construct a fresh `DrainState::new(&Arc::new(StatsRegistry::new()))` in-test, invoke the render fn, and assert (response shape AND post-call `drain.current()`). This bilateral verification deliberately bypasses the still-`todo!()`-gated `render_with` dispatch arm, so the side effect + response shape are fully verified at Task 3 even though the end-to-end dispatch surface lights up only at Task 4.

Appends 3 new rows to `docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s "Admin endpoint body shapes" table (one per POST endpoint) and patches the existing `/server_info` row note to acknowledge the D5e value-source rebind: replaces the prior parenthetical `(08.1 emits the constant "LIVE"; 08.2 extends to LIVE / DRAINING)` with the explicit `Live | HealthcheckFailing → "LIVE"`, `Draining → "DRAINING"` mapping prose per the SPEC text in PLAN.md Task 3 Step 3.

### Tests landed (9)

All 9 colocated in a new `endpoint::drain_admin_tests` module appended at the bottom of `crates/envoy-admin/src/endpoint.rs` (mirrors the per-phase / per-task subsidiary `#[cfg(test)] mod _tests` pattern already established by `config_dump_tests` / `server_info_tests` / `clusters_tests` / `listeners_tests`):

1. **`drain_listeners_path_dispatches_on_post`** — `dispatch("POST", "/drain_listeners")` resolves to `Dispatch::Endpoint(AdminEndpoint::DrainListeners)`.
2. **`drain_listeners_405_on_get`** — `dispatch("GET", "/drain_listeners")` resolves to `Dispatch::MethodNotAllowed { allow: "POST" }`.
3. **`healthcheck_fail_path_dispatches_on_post`** — same shape for `/healthcheck/fail`.
4. **`healthcheck_ok_path_dispatches_on_post`** — same shape for `/healthcheck/ok`.
5. **`drain_listeners_render_returns_200_empty_body_and_invokes_drain`** — `render_drain_listeners(&drain)` returns status 200 / reason `Some("OK")` / empty body AND leaves `drain.current() == DrainStage::Draining`.
6. **`healthcheck_fail_render_returns_200_empty_body_and_flips_state`** — same shape; post-call state `HealthcheckFailing`.
7. **`healthcheck_ok_render_returns_200_empty_body_and_restores_live`** — starts from a pre-failed drain (calls `fail_healthcheck()` first); `render_healthcheck_ok(&drain)` restores `Live`.
8. **`healthcheck_ok_after_drain_is_noop_via_render_fn`** — sticky-drain regression at the render-fn boundary: pre-drained state stays `Draining` after `render_healthcheck_ok`.
9. **`each_drain_endpoint_declares_post_allowed_method`** — compile-time tautology for the three new arms in `allowed_method()`.

Red phase (Step 2) verified pre-implementation: `cargo test -p envoy-admin --lib drain_admin_tests` produced 7 `E0599` errors (3 × `no variant or associated item named DrainListeners`; 2 × `... HealthcheckFail`; 2 × `... HealthcheckOk`) + matching `E0432: unresolved imports super::render_drain_listeners, super::render_healthcheck_fail, super::render_healthcheck_ok`. Green phase (Step 4) verified post-implementation: `test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s`.

### Per-task deviations from PLAN

1. **Architecture-decision deviation #1 (anticipated by the SPEC + PLAN): `render_with` arms gated with `todo!()` until Task 4.** PLAN.md Task 3 NOTE block (lines 1068-1092) explicitly authorizes this split — the alternative ("widen `AdminHandler::new` to 7-arg at Task 3") would conflate Task 3's enum + render-fn surface with Task 4's `envoy-bin` wiring. The PLAN recommendation is to land the render fns at Task 3 + leave `render_with` arms gated; this PROGRESS records the cleanly-followed path. The `todo!()` strings explicitly reference Task 4 (D13b) per the SPEC sketch.

2. **`#[allow(dead_code)]` on the 3 render fns + the `empty_200_ok()` helper.** Required to keep `cargo clippy -D warnings` green: the render fns are reachable only from the colocated unit tests at Task 3 (the production-side call sites in `render_with` are `todo!()`-gated). Each allow is scoped per-decl + carries an inline doc-comment referencing Task 4's removal of the allow. Mirrors the Task 2 precedent for `Listener::listener_manager_active` field + accessor decoration (`PROGRESS.md:304`).

3. **fmt-driven single-line signatures on the 3 render fns.** Initial implementation wrote `pub(crate) fn render_drain_listeners(\n    drain: &envoy_listener::DrainState,\n) -> envoy_http1::Response {` per the SPEC snippet; `cargo fmt --all` collapsed each to a single-line `pub(crate) fn render_drain_listeners(drain: &envoy_listener::DrainState) -> envoy_http1::Response {` (the body fits rustfmt's max width). Functionally identical; recorded per the Task 1 precedent of documenting incidental fmt adjustments.

4. **Module name `drain_admin_tests` (not `tests`).** The SPEC snippet at PLAN.md Step 1 reads "Append (the 9 new tests cover dispatch + render + side-effect)" without naming a target module. Appending the 9 tests to the existing `mod tests` would have crowded the 06.1-era unit-test module; per the Task 6/7/8/9 precedent already in this file (every 08.1 task per-endpoint test cohort lives in its own subsidiary `#[cfg(test)] mod <task>_tests`), the new tests live in a dedicated module. Module-scope `use super::{...}` imports the 3 render fns + `AdminEndpoint` + `Dispatch`.

5. **BEHAVIOR_CONTRACT.md `/server_info` row patch: verbatim-replaceable.** The on-disk text at line 143 matched the SPEC's pre-patch snippet verbatim (modulo trailing whitespace) — the replacement landed in a single `Edit` invocation with zero adaptation. (Confirmed prior to the patch by direct read.)

### LoC delta

| File | Insertions | Deletions |
|---|---|---|
| `crates/envoy-admin/src/endpoint.rs` | +223 | 0 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | +4 | -1 |
| **Total source + doc:** | **+227** | **-1** |

Test-count delta: `envoy-admin` lib bucket grew **58 → 67** (+9, exactly the 9 new tests in `drain_admin_tests`); workspace total grew by the same +9 (no other crate touched).

No new top-level Cargo deps. `envoy_listener` / `envoy_stats` / `bytes` / `envoy_http1` were all already on `envoy-admin`'s `[dependencies]` list since the 08.1 Task 6 / 7 / 8 / 9 cohort. Crate root still carries `#![forbid(unsafe_code)]`; zero new unsafe blocks.

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff after one `cargo fmt --all` apply during implementation — the multi-line `pub(crate) fn render_*` signatures collapsed to single-line shape per rustfmt's preferred form for fitting widths).

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean compile across all 8 workspace crates, zero warnings). The 4 `#[allow(dead_code)]` annotations on the 3 render fns + `empty_200_ok()` helper are required at Task 3 (the `render_with` dispatch arms are `todo!()`-gated until Task 4 lands `handler.drain()`).

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 8 crates + 2 helper bin crates compiled).

**Gate 4 — `cargo test --workspace`:** PASS (exit 0; every per-bucket `test result:` line reads `ok. N passed; 0 failed`; the `envoy-admin` lib bucket grew from 58 → 67 tests — the 9 new `endpoint::drain_admin_tests::{drain_listeners_path_dispatches_on_post, drain_listeners_405_on_get, healthcheck_fail_path_dispatches_on_post, healthcheck_ok_path_dispatches_on_post, drain_listeners_render_returns_200_empty_body_and_invokes_drain, healthcheck_fail_render_returns_200_empty_body_and_flips_state, healthcheck_ok_render_returns_200_empty_body_and_restores_live, healthcheck_ok_after_drain_is_noop_via_render_fn, each_drain_endpoint_declares_post_allowed_method}`). Focused re-run: `cargo test -p envoy-admin --lib drain_admin_tests` reads `9 passed; 0 failed; 0 ignored; 0 measured; 58 filtered out`. (Note: a first workspace-test run flaked on `differential::backend::tests::tcp_proxy_backend_{spawns_and_echoes,drop_terminates_child}` with port-collision-style `Connection refused`; both tests passed on isolated re-run AND on a subsequent full-workspace re-run with 94/94 differential pass — the failures are a pre-existing port-allocation flake in the parallel-test execution environment, unrelated to Task 3's changes which only touch `crates/envoy-admin` and a docs file. The clean 94/94 workspace re-run is the authoritative gate signal.)

**Gate 5 — `cargo deny check`:** PASS (exit 0). Verbatim output:

```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

The 5 `license-not-encountered` warnings are pre-existing (`deny.toml` allow-list broader than the transitive tree); the verdict line `advisories ok, bans ok, licenses ok, sources ok` is the gate-pass signal. Quoted verbatim per 07.1-REVIEW doctrine.

### Differential surface delta

**None at the differential / fixture surface.** Task 3 introduces the 3 POST endpoint variants + render fns but the `render_with` dispatch is still `todo!()`-gated (Task 4 lands `handler.drain()`), so no live HTTP wire surface changes yet. Fixture 0015 at Task 8 is the first differential surface to exercise the endpoints end-to-end. All 14 Docker-gated fixtures (0001-0014) remain GREEN by construction (zero changed wire-protocol behavior on already-shipped endpoints); the 08.1 state-4 anchor CI run `25964680619` HEAD `03e6435` remains the authoritative bridge-CI evidence until the 08.2 state-4 anchor at Task 11.

The `BEHAVIOR_CONTRACT.md` "Admin endpoint body shapes" table gains 3 new rows (one per POST endpoint) + the existing `/server_info` row note patched to acknowledge the D5e value-source rebind that Task 5 lands. Per the PLAN architecture-decision lock-in #24 (body-shape rows land at Task 3); the Admin-action effect equivalence subsection lands at Task 8.
