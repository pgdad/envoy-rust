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

## Task 4 (D13b — `AdminHandler::new` 7-arg widen + envoy-bin DrainState wiring)

### Work summary

Widens `AdminHandler::new` from 6-arg to 7-arg (adds the trailing `drain: Arc<DrainState>` parameter per 08.2 PLAN architecture-decision lock-in #13 — additive, no reordering of existing positional args). Adds a `drain: Arc<DrainState>` field on the `AdminHandler` struct + a `pub(crate) fn drain(&self) -> &Arc<DrainState>` accessor mirroring the existing `bootstrap()` / `registry()` / `cluster_manager()` / `command_line_options()` accessor shape. Extends the `ConnectionHandler::handle` impl's `let cloned = Arc::new(AdminHandler { ... })` site to clone the new `drain` `Arc` (mirrors the pattern Task 5 / 06.1 established for the other handle-arc fields).

Replaces the 3 `todo!()`-gated arms in `endpoint::AdminEndpoint::render_with` (introduced at Task 3 as the architecture-decision deviation #1 placeholders) with the real `render_drain_listeners(handler.drain())` / `render_healthcheck_fail(handler.drain())` / `render_healthcheck_ok(handler.drain())` calls. Removes the `#[allow(dead_code)]` annotations from the 3 render fns + the shared `empty_200_ok()` helper introduced at Task 3 — the production-side dispatch path now reaches all four declarations, so the per-decl allows are no longer needed (and would themselves now trigger `clippy::useless_attribute` under `-D warnings`).

Threads the new `drain` handle through `envoy-bin::main`: constructs a single process-wide `Arc<DrainState>` ONCE at startup (alongside the existing `Arc<StatsRegistry>` at `crates/envoy-bin/src/main.rs:101-102`, before the data-plane listener-walk so both the admin and the future Task-6 data-plane `Listener::serve` call sites observe the same handle) and passes `Arc::clone(&drain)` as the 7th argument to `envoy_admin::AdminHandler::new` at the admin-handler-construction site (`crates/envoy-bin/src/main.rs:358-365`). The data-plane `Listener::serve` call sites at the tcp_proxy + HCM arms continue to invoke the still-1-arg `Listener::serve(shutdown)` signature unchanged at Task 4 — Task 6 (D12) widens `Listener::serve` to 2-arg (`shutdown, drain`) and updates envoy-bin's two `set.spawn(async move { listener.serve(...) })` sites accordingly.

Updates every existing `AdminHandler::new(...)` test call site (7 sites in the `handler.rs::tests` module plus the `handler_with_bootstrap` helper at `endpoint.rs::config_dump_tests` plus the constructor-coverage test at `handler.rs::admin_handler_new_6arg_tests`) to pass `Arc::new(envoy_listener::DrainState::new(&registry))` as the new 7th arg. Where call sites previously moved a bare `registry: Arc<StatsRegistry>` into `new`, the move-by-value was switched to `Arc::clone(&registry)` so the same `registry` can also feed `DrainState::new(&registry)` on the next line (3 sites required this adjustment; the other 4 already cloned).

### Tests landed (2)

Both colocated in the existing `handler::tests` module at `crates/envoy-admin/src/handler.rs` (the 06.1-era in-process integration-style test cohort), positioned immediately before the `handler_response_carries_admin_headers` test:

1. **`admin_handler_new_takes_drain_state_as_seventh_arg`** — Constructs a fresh `Arc<DrainState>` + `AdminHandler::new(…, Arc::clone(&drain))`, then asserts `Arc::ptr_eq(handler.drain(), &drain)`. Verifies (a) the constructor accepts the 7th positional arg and (b) the new `drain()` accessor returns the same `Arc` (pointer equality — the constructor stores the passed-in handle without cloning the underlying `DrainState`).
2. **`drain_listeners_endpoint_invokes_drain_via_render_with`** — Constructs a handler with a fresh `DrainState`, asserts pre-state is `DrainStage::Live`, dispatches `AdminEndpoint::DrainListeners.render_with(&handler)`, then asserts (a) `resp.status == 200` and (b) post-state is `DrainStage::Draining`. The end-to-end equivalent of Task 3's `render_drain_listeners(&drain)` direct-call test, but routed through the dispatch surface that Task 4 wires by removing the `todo!()` placeholder.

Red phase (Step 2) verified pre-implementation: `cargo test -p envoy-admin --lib` produced `error[E0061]: this function takes 6 arguments but 7 arguments were supplied` on the 2 new tests AND `error[E0599]: no method named drain found for struct AdminHandler` on the `handler.drain()` call in test 1 (3 errors total — both tests show the E0061 + test 1 also shows the E0599; the 2nd test's `render_with` call indirectly triggers the same `drain()` method lookup through the dispatch). Green phase (Step 4) verified post-implementation: `test result: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` (67 → 69, +2 exactly the new tests).

### Per-task deviations from PLAN

1. **Test-helper names diverge from the PLAN snippet (`test_admin_config` / `test_bootstrap_arc` / `test_cluster_manager_arc`) — used the existing helpers `admin_config(port)` / `dummy_bootstrap()` / `dummy_cluster_manager()` in scope at `crates/envoy-admin/src/handler.rs::tests::{387,405,414}` instead.** The PLAN snippet at Task 4 Step 1 referenced names that don't exist in the file (a notational shorthand in the PLAN's brainstorming style). The two new tests live in the same `handler::tests` module as the 7 existing in-process integration tests, so the existing helpers are in scope by direct super-module use; no new helper introduction needed. Functionally identical to the PLAN snippet (the 2 tests construct the same handle set + invoke the same assertions).

2. **`admin_handler_new_6arg_tests::admin_handler_new_accepts_six_args_and_constructs` retained its 08.1 historical name + module name.** The test name + module name both reference "six args" / "6arg" — accurate descriptions of the 08.1 D13a shape that this test originally covered. Task 4's 7-arg widening makes those names mildly misleading, but renaming a sibling test that already passes (constructor-coverage tautology + `config()` accessor sanity-check) at this task would noise the diff for zero behavior change. Updated the test body to pass the new `drain` arg + added an inline doc-comment at `crates/envoy-admin/src/handler.rs:955-967` noting the historical name retention and pointing readers to the new sibling `admin_handler_new_takes_drain_state_as_seventh_arg` in `super::tests` as the Task-4-shape-specific coverage. (Test 9 at fixture 0015 + the future fuzz seed at Task 9 are the more authoritative post-08.2 surfaces; renaming this 06.1-era in-file constructor test is best deferred to a dedicated cleanup pass if reviewers prefer.)

3. **Clippy `doc_lazy_continuation` red on initial `drain()` accessor doc-comment.** The first draft of the `pub(crate) fn drain` doc-comment used `+`-joined list-style across continuation lines (`/// + render_healthcheck_fail + render_healthcheck_ok`), which `clippy::doc_lazy_continuation` (1.95.0) interprets as malformed Markdown list items. Reworded to use comma-separated prose; the doc-comment content is functionally identical (every render-fn reference + Task-5 reference is preserved). Recorded per the cross-task precedent of documenting incidental clippy-driven rewordings.

4. **Switched 3 `AdminHandler::new(…, registry, …)` move-by-value test call sites to `Arc::clone(&registry)`.** The pre-Task-4 sites at `handler.rs::tests::{handler_serves_ready_in_process, handler_returns_404_for_unknown_path, handler_returns_405_for_post_method, handler_response_carries_server_header, admin_handler_idle_read_times_out_at_5s, handler_response_carries_admin_headers}` moved `registry` directly into `AdminHandler::new`. Task 4 needs the same `registry` to also feed `DrainState::new(&registry)` on the same expression, so the move-by-value was changed to `Arc::clone(&registry)` (cheap refcount bump). The `handler_serves_stats_prometheus_in_process` site already used `Arc::clone(&registry)` (it needs the registry alive to register a counter post-construction); 6 sites adjusted total.

5. **`drain` binding in `envoy-bin::main` is not yet observed by the data-plane listeners.** Per the PLAN spec, Task 4 only threads `drain` into the admin handler; the tcp_proxy + HCM `set.spawn(async move { listener.serve(...) })` sites at `crates/envoy-bin/src/main.rs:235-240` and `crates/envoy-bin/src/main.rs:333-338` continue to call the still-1-arg `Listener::serve(shutdown)` signature unchanged. Task 6 (D12) widens `Listener::serve` to 2-arg and updates those two sites. Rust does NOT warn about this — the `drain` binding is referenced inside the `if let Some(admin_cfg)` block via `Arc::clone(&drain)`, which counts as a use; clippy stayed green (verified with `cargo clippy --workspace --all-targets --all-features -- -D warnings` at Step 5).

### LoC delta

| File | Insertions | Deletions |
|---|---|---|
| `crates/envoy-admin/src/handler.rs` | +110 | -14 |
| `crates/envoy-admin/src/endpoint.rs` | +28 | -34 |
| `crates/envoy-bin/src/main.rs` | +11 | 0 |
| **Total source:** | **+149** | **-48** |

Test-count delta: `envoy-admin` lib bucket grew **67 → 69** (+2, exactly the 2 new tests in `handler::tests`); `envoy-bin` lib bucket unchanged at 8 (Task 4 widened the binary's startup wiring but did not add new envoy-bin lib tests — the binary-level coverage is the existing `cargo build -p envoy-bin --bin envoy-bin` compile check, plus the upcoming fixture 0015 differential at Task 8). Workspace total grew by the same +2 (no other crate touched).

No new top-level Cargo deps. `envoy_listener` was already on `envoy-admin`'s `[dependencies]` list (Task 3 added the `envoy_listener::DrainState` references through the re-export at `crates/envoy-admin/src/lib.rs:17`); the new `use envoy_listener::{..., DrainState}` import in `handler.rs:11` is purely a name-scope addition. Crate root `crates/envoy-admin/src/lib.rs` still carries `#![forbid(unsafe_code)]`; zero new unsafe blocks.

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff).

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean across all 8 workspace crates, zero warnings, zero errors). The initial draft of the `drain()` accessor doc-comment tripped `clippy::doc_lazy_continuation` (6 errors at `handler.rs:151-158`) — reworded `+`-joined references to comma-separated prose (per-task deviation #3); the rewording landed before Step 5's authoritative gate run.

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 8 workspace crates + 2 helper bin crates compiled — `envoy-admin` rebuilt because of the new field + accessor + dispatch arms, `envoy-bin` rebuilt because of the new `drain` binding + 7th arg at the `AdminHandler::new` call site, all downstream test compilation succeeded).

**Gate 4 — `cargo test --workspace`:** PASS (exit 0; every per-bucket `test result:` line reads `ok. N passed; 0 failed`; the `envoy-admin` lib bucket grew from 67 → 69 tests — the 2 new `handler::tests::{admin_handler_new_takes_drain_state_as_seventh_arg, drain_listeners_endpoint_invokes_drain_via_render_with}`). Focused re-run: `cargo test -p envoy-admin --lib` reads `69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s`. No flakes observed on the workspace run — every previously-flaky bucket (`differential::backend::tests::tcp_proxy_backend_*` per Task 3 PROGRESS) ran clean on the single authoritative run.

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

**None at the differential / fixture surface.** Task 4 wires the `handler.drain()` accessor through the dispatch path (the `render_with` arms now invoke the real render fns instead of `todo!()`-panicking), but the 3 POST endpoints' end-to-end wire surface is exercised only by fixture 0015 at Task 8 (the first differential surface that POSTs to `/drain_listeners` + observes the post-drain `/ready 503` and `/server_info.state == "DRAINING"` mappings — those mappings themselves land at Task 5's D5e + D-ready patch). All 14 Docker-gated fixtures (0001-0014) remain GREEN by construction (zero changed wire-protocol behavior on any already-shipped endpoint; the only behavioral change at Task 4 is that POSTing to the 3 new endpoints now returns 200 OK instead of crashing on `todo!()`, and no shipped fixture POSTs to those endpoints — fixture 0015 is the first). The 08.1 state-4 anchor CI run `25964680619` HEAD `03e6435` remains the authoritative bridge-CI evidence until the 08.2 state-4 anchor at Task 11.

The `#[allow(dead_code)]` removal at the 3 render fns + `empty_200_ok()` helper is a Task-4 hard requirement per the inline doc-comments Task 3 added (each annotation's doc-comment explicitly said "Task 4 removes the allow"); the production-side `render_with` arms now reach all four declarations, so clippy under `-D warnings` would itself flag the `#[allow(dead_code)]` annotations as `useless_attribute` if they were left in place. Confirmed removed at Task 4 — Gate 2 (clippy) green is the structural evidence.

## Task 5 (D5e + D-ready — `/server_info` state-source rebind + `/ready` drain-aware response)

### Work summary

Patches `render_server_info`'s `state` field source from the 08.1 literal `"LIVE"` to a `DrainState`-derived match: `match handler.drain().current() { Live | HealthcheckFailing => "LIVE", Draining => "DRAINING" }` per parent-08 SPEC §5.5 wire-state mapping. The `ServerInfoBody<'a>.state: &'static str` shape is unchanged at the 08.1 → 08.2 boundary — only the value-binding source moves from the constant literal to the live-state read.

Widens `/ready` from the 06.1 hardcoded 200 `"LIVE\n"` shape to a 3-arm drain-aware response. The old `fn render_ready()` inside `impl AdminEndpoint` is removed; a new `pub(crate) fn render_ready_with(handler: &AdminHandler) -> envoy_http1::Response` free fn lives alongside the other handler-aware renderers (`render_config_dump` / `render_server_info` / `render_clusters` / `render_listeners`). The three arms per parent-08 SPEC §5.5:
- `DrainStage::Live` → status 200, reason `"OK"`, body `"LIVE\n"`
- `DrainStage::HealthcheckFailing` → status 503, reason `"Service Unavailable"`, body `"Service Unavailable\n"`
- `DrainStage::Draining` → status 503, reason `"Service Unavailable"`, body `"DRAINING\n"`

All three response shapes carry `content-type: text/plain` + `content-length: <body.len()>` headers (the established admin-response convention; the 06.1 `render_ready` shape did the same). The `reason` field is set explicitly via `Some(...)` for all three arms — preserves the 06.1 convention where `Ready`'s wire reason phrase comes from the renderer rather than falling through to `reason_for_status`.

Updates the `AdminEndpoint::render_with` dispatch tree to route `AdminEndpoint::Ready → render_ready_with(handler)`. The `AdminEndpoint::render` (registry-only path) `Ready` arm becomes `unreachable!("Ready requires handler-scoped DrainState; dispatch via AdminEndpoint::render_with")` — matches the existing pattern for the other handler-scoped endpoints (`ConfigDump`, `ServerInfo`, `Clusters`, `Listeners`, and the 08.2 D9/D10 trio). The `_ => self.render(handler.registry())` fallback at the bottom of `render_with` still carries the two purely registry-only endpoints (`Stats`, `StatsPrometheus`) through to the original `render` path.

### Tests landed (5)

All 5 tests live in a new `ready_drain_tests` submodule colocated at the bottom of `crates/envoy-admin/src/endpoint.rs`, positioned after the existing `drain_admin_tests` submodule (mirrors the per-task per-submodule precedent: Task 3 added `drain_admin_tests`; Tasks 6/7/8/9 added `config_dump_tests` / `server_info_tests` / `clusters_tests` / `listeners_tests` at 08.1). The module hosts one `test_handler_with_drain(drain: Arc<DrainState>) -> AdminHandler` helper that mirrors the existing `handler_with_bootstrap` in `config_dump_tests` but accepts a pre-constructed `Arc<DrainState>` so each test can drive the underlying state transition (`drain()` / `fail_healthcheck()`) BEFORE invoking the render fn.

1. **`server_info_state_is_draining_when_drain_state_is_draining`** — Constructs a fresh `Arc<DrainState>`, calls `drain.drain()`, builds an `AdminHandler` carrying that drain, invokes `render_server_info(&handler)`, parses the JSON body, asserts `body["state"] == "DRAINING"`.
2. **`server_info_state_is_live_when_drain_state_is_healthcheck_failing`** — Same shape but with `drain.fail_healthcheck()` instead; asserts `body["state"] == "LIVE"` (validates the parent-08 SPEC §5.5 "server-state is INDEPENDENT of healthcheck-failure" semantic — only `/ready` flips to 503 under HealthcheckFailing; `/server_info.state` stays LIVE).
3. **`ready_returns_200_live_when_drain_state_is_live`** — Default `DrainState::new` stage is `Live`; dispatches `AdminEndpoint::Ready.render_with(&handler)`, asserts `status=200`, `reason=Some("OK")`, body `b"LIVE\n"`.
4. **`ready_returns_503_draining_when_drain_state_is_draining`** — Calls `drain.drain()` pre-render; asserts `status=503`, `reason=Some("Service Unavailable")`, body `b"DRAINING\n"`.
5. **`ready_returns_503_service_unavailable_when_drain_state_is_healthcheck_failing`** — Calls `drain.fail_healthcheck()` pre-render; asserts `status=503`, `reason=Some("Service Unavailable")`, body `b"Service Unavailable\n"`.

Red phase (Step 2) verified pre-implementation: focused run `cargo test -p envoy-admin --lib ready_drain_tests` produced `test result: FAILED. 2 passed; 3 failed; 0 ignored` — the 3 stage-transition tests (`server_info_state_is_draining_…`, `ready_returns_503_draining_…`, `ready_returns_503_service_unavailable_…`) failed with `left: Some("LIVE"); right: Some("DRAINING")` and `left: 200; right: 503` assertions (the literal-`"LIVE"` state field and the hardcoded `200 LIVE` body shapes from the pre-Task-5 surface). The 2 Live-stage tests passed by coincidence pre-implementation because the existing 06.1 shape ALREADY returns 200 LIVE / state=LIVE under default `DrainStage::Live`; they serve as negative-control assertions that the impl preserves the Live path bilaterally. Green phase (Step 4) verified post-implementation: `cargo test -p envoy-admin --lib` reads `test result: ok. 74 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s` (69 → 74, +5 exactly the new tests in `ready_drain_tests`).

### Per-task deviations from PLAN

1. **The existing 06.1 `render_ready_returns_200_live` test was UPDATED rather than REPLACED.** Spec Step 4 said "update it to call `Ready.render_with(&test_handler())` instead". The test body at `crates/envoy-admin/src/endpoint.rs:598-624` (pre-Task-5) called `AdminEndpoint::Ready.render(&reg)` with a bare `StatsRegistry`; post-Task-5 it constructs a handler via `handler_with_bootstrap(TINY_BOOTSTRAP)` (reusing the 08.1 Task-6 helper hoisted in `config_dump_tests`) and calls `AdminEndpoint::Ready.render_with(&handler)`. All four shape assertions (status=200, reason="OK", body="LIVE\n", content-type/content-length headers) are preserved verbatim. Added an inline doc-comment explaining the Task-5 dispatch reroute + pointing to the colocated `ready_drain_tests` submodule for the new per-stage coverage. Functionally identical assertion set; per-stage tests are a strict superset of the original 06.1 shape coverage.

2. **`test_handler_with_drain` helper is a near-duplicate of `handler_with_bootstrap` (rather than reusing it).** The PLAN said the new helper "mirrors handler.rs::tests helpers". `config_dump_tests::handler_with_bootstrap` already constructs a fresh per-call `Arc<DrainState>` internally and would not let the test pre-mutate the drain state. The cleanest path was to introduce a sibling helper in `ready_drain_tests` that takes the pre-mutated `Arc<DrainState>` as an arg + builds the handler around the supplied drain (otherwise the test would need to mutate drain post-handler-construction via the pub(crate) accessor — same effect but a less direct test-time API). The 8-line duplication is intentional + cheap; an alternative refactor of `handler_with_bootstrap` to accept an `Option<Arc<DrainState>>` was rejected as out-of-scope churn for a 5-test cohort.

3. **The new submodule lives in `ready_drain_tests` (NOT `server_info_drain_tests` or split across 2 submodules).** All 5 tests share the `test_handler_with_drain` helper and operate against the same two state-flipping primitives (`drain()` / `fail_healthcheck()`); splitting into two submodules would force helper duplication or a sibling-import indirection without test-organization benefit. The submodule name reflects that the dominant new surface is `/ready` widening; the 2 `/server_info` tests are co-located because they exercise the same `DrainState` reads from the same handler.

4. **No data-plane or backstop changes required.** The in-process backstop `crates/envoy-bin/tests/admin_ready.rs` calls `GET /ready` against a freshly-spawned `envoy-bin` whose `DrainState` defaults to `DrainStage::Live`; the new dispatch path returns the same 200 OK "LIVE\n" surface as the pre-Task-5 shape (verified by `cargo test -p envoy-bin --test admin_ready` → `1 passed; 0 failed`). Fixtures 0001-0014 stay GREEN by construction — none of them POSTs to `/drain_listeners` or `/healthcheck/fail`, so the `state` field stays "LIVE" and `/ready` stays 200 LIVE under the new dispatch. The first fixture to exercise the new 503 path is fixture 0015 at Task 8.

### LoC delta

| File | Insertions | Deletions |
|---|---|---|
| `crates/envoy-admin/src/endpoint.rs` | +201 | -20 |
| **Total source:** | **+201** | **-20** |

Test-count delta: `envoy-admin` lib bucket grew **69 → 74** (+5, exactly the 5 new tests in `endpoint::ready_drain_tests`); no other crate touched. Workspace total grew by the same +5.

No new top-level Cargo deps. `envoy_listener::DrainStage` is reached through the existing `envoy_listener` dep on `envoy-admin` (Task 3 added it; Task 4 carried it forward through `handler.rs`). Crate root `crates/envoy-admin/src/lib.rs` still carries `#![forbid(unsafe_code)]`; zero new unsafe blocks.

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff).

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean across all 8 workspace crates, zero warnings, zero errors).

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 8 workspace crates compiled — `envoy-admin` rebuilt because of the `render_server_info` state-source patch + the new `render_ready_with` free fn + the dispatch tree rewire + the new test submodule; all downstream test compilation succeeded).

**Gate 4 — `cargo test --workspace`:** PASS — every per-bucket `test result:` line reads `ok. N passed; 0 failed`; the `envoy-admin` lib bucket grew from 69 → 74 tests (the 5 new `endpoint::ready_drain_tests::{server_info_state_is_draining_when_drain_state_is_draining, server_info_state_is_live_when_drain_state_is_healthcheck_failing, ready_returns_200_live_when_drain_state_is_live, ready_returns_503_draining_when_drain_state_is_draining, ready_returns_503_service_unavailable_when_drain_state_is_healthcheck_failing}`). Focused re-run: `cargo test -p envoy-admin --lib` reads `74 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s`. The in-process backstop `cargo test -p envoy-bin --test admin_ready` reads `1 passed; 0 failed` (verifies the Live-default dispatch reroute preserves the end-to-end wire shape). One flake observed on the first workspace run (`differential::backend::tests::tcp_proxy_backend_spawns_and_echoes` + `…_drop_terminates_child` failed with "tcp-echo-server never became accept-ready" — port-contention flake; same pair flaked at Task 3 / Task 4 PROGRESS); a focused re-run `cargo test -p differential --lib backend::tests::tcp_proxy_backend` passed both deterministically (`2 passed; 0 failed`). The flake is orthogonal to Task 5's surface (no code path involved in the change touches `differential::backend`).

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

**None at the differential / fixture surface.** Task 5 lands the wire-state mapping (per parent-08 SPEC §5.5) BUT no shipped fixture (0001-0014) POSTs to `/drain_listeners` or `/healthcheck/fail`, so every fixture observes `DrainStage::Live` for its `/server_info.state` (→ "LIVE", unchanged) and `/ready` (→ 200 OK "LIVE\n", unchanged) reads. Fixture 0011 (06.1 admin stats) stays GREEN: it scrapes `/stats` + `/stats/prometheus`, both of which are unaffected by Task 5 (no DrainState read on those paths). Fixture 0014 (08.1 admin config_dump) stays GREEN: it scrapes `/config_dump` (untouched by Task 5) + `/server_info` (state stays "LIVE" because no drain POST landed) + `/clusters` + `/listeners` + `/ready` (200 LIVE under default `Live` stage). Fixture 0015 at Task 8 is the first differential surface to exercise the new 503 path. The 08.1 state-4 anchor CI run `25964680619` HEAD `03e6435` remains the authoritative bridge-CI evidence until the 08.2 state-4 anchor at Task 11.

Regression-equivalence on fixtures 0001-0014 preserved by construction: the bilateral `state == "LIVE"` and `/ready == 200 LIVE` assertions remain true because both upstream Envoy AND envoy-rust observe `DrainStage::Live` throughout each fixture's scrape window (no fixture issues a drain POST).

## Task 6 (D12 — `Listener::serve` 2-arg widening + `listener_manager.total_listeners_active` RAII guard)

### Work summary

Widens `Listener::serve` from 1-arg `(shutdown)` to 2-arg `(shutdown, drain: Arc<DrainState>)`. Adds a second `tokio::select!` arm `_ = drain.drain_signal() => { … }` positioned between the existing shutdown arm and the accept arm; either signal triggers the same drain code path (`drop(listener); break;` → fall through to the post-loop drain-wait + `DRAIN_BUDGET` timeout block). The `tokio::pin!(shutdown)` placement is preserved verbatim (the shutdown arm reads `&mut shutdown`); the new arm reconstructs `drain.drain_signal()` on each loop iteration, which is correct per Task 1 fixup's TOCTOU fix: `DrainState::drain_signal` anchors its `Notified` snapshot BEFORE the state load, and an already-`Draining` state short-circuits to `std::future::ready(())` so the arm fires on the very next iteration. Only the log message between the two arms differs (`"listener shutdown signal received; draining"` vs `"listener drain signal received; draining"`).

Adds a `ListenerManagerActiveGuard(Arc<envoy_stats::Gauge>)` RAII helper at module scope (above `impl std::fmt::Debug for Listener`). `ListenerManagerActiveGuard::new(gauge)` calls `gauge.inc()` then stores the `Arc`; `impl Drop` calls `self.0.dec()`. Inside `Listener::serve`, the guard is constructed FIRST (`let _lm_guard = ListenerManagerActiveGuard::new(Arc::clone(self.listener_manager_active()));`) so its Drop fires LAST per Rust's reverse-declaration drop order — that places the dec AFTER the post-loop drain-wait + DRAIN_BUDGET timeout block, so the gauge returns to 0 only once stragglers have joined (or been aborted). Mirrors the 06.3 `cx_active` per-connection guard pattern at per-listener granularity.

Removes the 2 `#[allow(dead_code)]` annotations Task 2 added: one on the `listener_manager_active` field at `crates/envoy-listener/src/lib.rs:109` (pre-Task-6); one on the `pub(crate) fn listener_manager_active()` accessor at `crates/envoy-listener/src/lib.rs:194` (pre-Task-6). Both annotations carried inline doc-comments at Task 2 explicitly naming Task 6 as the removal point ("Task 6 (D12) removes the allow when it hoists the gauge into the RAII guard at `serve` entry/exit"). The production-side `ListenerManagerActiveGuard::new` call now consumes both the accessor and the field's Arc, so leaving the annotations would itself trip `clippy::useless_attribute` under `-D warnings`.

Threads `Arc::clone(&drain)` into both data-plane `Listener::serve(...)` call sites in `crates/envoy-bin/src/main.rs`: the tcp_proxy arm at `main.rs:244-253` (was `main.rs:234-240` pre-Task-6, shifted +1 by the surrounding edit) and the HCM arm at `main.rs:343-353` (was `main.rs:333-338`). At each site the existing `let shutdown = token.clone();` is followed by a new `let drain_for_listener = std::sync::Arc::clone(&drain);` rebind so the `Arc<DrainState>` moves into the `set.spawn(async move { … })` closure alongside `shutdown` (the closure can't capture `drain` by reference; the source-level rename to `drain_for_listener` makes the move explicit and avoids shadowing the outer `drain` binding still in scope for the admin-handler construction at `main.rs:368-377`). The echo path at `main.rs:181-189` uses `echo::serve` directly + the admin path at `main.rs:368-389` uses `envoy_admin::serve` directly — both naturally excluded from drain observation per parent-08 SPEC §5.5 + 08.2 PLAN architecture-decision lock-in #12.

Updates all 8 existing in-file `Listener::serve(...)` test call sites in `crates/envoy-listener/src/lib.rs::tests` to the new 2-arg signature: `serves_accepts_and_dispatches_to_handler`, `serves_honors_shutdown_signal`, `serves_drains_in_flight_connection_within_budget`, `serves_aborts_stragglers_past_drain_budget`, `listener_cx_active_increments_on_accept_decrements_on_close`, `listener_cx_active_monotonic_then_decreasing_under_burst`, `listener_cx_accept_failed_increments_on_accept_error`, and `listener_increments_cx_total_on_accept`. Each test that already had a `registry: Arc<StatsRegistry>` in scope (for `Listener::bind` / counter+gauge re-registration) clones it once via `Arc::clone(&registry)` and constructs `let drain = Arc::new(DrainState::new(&registry));` immediately before the `tokio::spawn(listener.serve(…, drain))` call. Tests that previously moved `mk_registry()` directly into `Listener::bind` were widened to bind the value to a local `registry` first then pass `Arc::clone(&registry)` to `bind` (3 sites required this adjustment; the 5 cx_* tests already used `Arc::clone(&registry)`).

### Tests landed (2)

Both colocated in the existing `tests` module at the bottom of `crates/envoy-listener/src/lib.rs`, positioned immediately before the `drain_budget_constant_tests` module-level boundary (the canonical "new tests live at the bottom of the existing module" precedent from Tasks 2/3/4/5 in this phase + 06.1/06.3 cohort tests in this same file).

1. **`serve_returns_when_drain_signal_fires`** (multi-thread tokio test) — Binds a `Listener` on `127.0.0.1:0` with `EchoHandler`, constructs a fresh `Arc<DrainState>` against the same registry, spawns `listener.serve(std::future::pending::<()>(), Arc::clone(&drain))` (the shutdown future NEVER resolves, so the only way `serve` can exit is via the drain arm), yields 100ms so the spawned task reaches its `tokio::select!` and anchors the first `drain_signal()` snapshot, then calls `drain.drain()` from the main task. Asserts (a) `tokio::time::timeout(DRAIN_BUDGET + 500ms, serve_handle)` resolves — verifying the new select arm fired and the drain code path ran to completion within budget — and (b) post-serve `snapshot.get("listener_manager.total_listeners_active")` returns `Some(StatHandle::Gauge(g))` with `g.value() == 0`, verifying the RAII guard's `Drop` decremented the gauge after the post-loop drain-wait block completed. The `Some(...)` assertion uses `.expect("listener_manager.total_listeners_active gauge must be registered")` rather than a silent `if let Some(...)` per the deviation #1 hardening below.
2. **`serves_honors_shutdown_signal_with_drain_param`** — Mirror of the existing `serves_honors_shutdown_signal` shape against the new 2-arg signature. Constructs a fresh `Arc<DrainState>` carrying no fired drain, spawns `listener.serve(rx_await_shutdown, drain)`, fires shutdown immediately via `tx.send(())`, asserts serve resolves within 2s. Verifies the shutdown arm remains functional verbatim alongside the new drain arm — the select arms are additive, not exclusive.

Red phase (Step 2) verified pre-implementation: `cargo test -p envoy-listener --lib` produced **10 × E0061** (`this method takes 1 argument but 2 arguments were supplied`) — 8 from the existing test-call-site updates above plus 2 from the new tests. Both new tests cited `pub async fn serve` (1-arg) at `lib.rs:209` as the method definition; both also flagged the unexpected `Arc<DrainState>` 2nd argument with the canonical `help: remove the extra argument` suggestion. The full red-phase build error count was a clean 10 (no E0599 / no E0277 / no other downstream type errors), confirming the pre-impl shape needs ONLY the signature widening + the RAII guard hoist to turn green. Green phase (Step 4) verified post-implementation: `cargo test -p envoy-listener --lib` reads `test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.06s` (28 → 30, +2 exactly the new tests).

### Per-task deviations from PLAN

1. **`serve_returns_when_drain_signal_fires` post-serve gauge assertion uses `.expect("…")` + `assert_eq!` instead of the PLAN-snippet `if let Some(...)`.** The PLAN's worked example (Step 1) used `if let Some(stat) = snapshot.get("…") { match stat { StatHandle::Gauge(g) => assert_eq!(g.value(), 0, …), _ => panic!(…) } }`. The critical-execution-notes section flagged this as a soft assertion — if the gauge were silently missing, the test would pass without observing the post-drop state. Hardened at `crates/envoy-listener/src/lib.rs:1019-1029` to `let handle = snapshot.get(...).expect("listener_manager.total_listeners_active gauge must be registered");` followed by the same `match handle { StatHandle::Gauge(g) => assert_eq!(…), _ => panic!(…) }`. Functionally a strict superset of the PLAN assertion: the gauge-registered invariant is now load-bearing on the test outcome rather than silently bypassed.

2. **`serve_returns_when_drain_signal_fires` uses a 100ms sleep to "let serve reach its select".** The PLAN prescribed this directly (Step 1 verbatim: "yield 100ms so serve registers as drain_signal waiter"). The critical-execution-notes section flagged this as flake-prone vs Task 1's `Barrier` rendezvous pattern. Considered alternatives:
   - **Barrier**: The serve body owns its `tokio::select!` privately — the spawned task would need to participate in the barrier BEFORE the select, but serve's body is opaque to the test (any in-serve barrier would require widening serve's signature again or wrapping the `EchoHandler::handle` to trip the barrier on first invocation; widening serve is out of scope for Task 6, and wrapping `handle` would change WHAT signals "serve is in select" — `handle` only fires AFTER accept, which is reached AFTER select).
   - **`tokio::task::yield_now()` loop**: Same problem — no observable "serve is at select" surface from the outside.
   - **Status quo**: The 100ms is a generous budget for spawning + reaching the select on macOS/Linux x86_64/arm64 dev machines + CI. The DRAIN_BUDGET + 500ms outer timeout (5500ms total) is the test's actual deadline; the 100ms internal sleep is well within the slack. If CI flakes here, the most cost-effective fix is bumping the sleep to 250ms-500ms (still well within 5500ms slack). Recorded under deviations rather than altering the PLAN prescription, per the doctrine ("the 100ms sleep is a known flake-prone pattern; if you found a deterministic alternative, document"). No deterministic alternative found that doesn't require widening `Listener::serve`'s public surface — deferred to a future hardening pass if real CI flake data emerges.

3. **The new RAII guard lives at module scope (above `impl Listener`), not inside `impl Listener`.** Rust doesn't allow nested `impl Drop` blocks inside an outer `impl`; the helper struct + its Drop must live at module scope. Placed at `crates/envoy-listener/src/lib.rs:109-129` — immediately after the `Listener` struct declaration and before `impl std::fmt::Debug for Listener` so a reader scanning the file sees the field declaration → guard helper → Debug impl → main impl in source order. The `struct ListenerManagerActiveGuard(Arc<envoy_stats::Gauge>);` is private (no `pub`) — only `Listener::serve` constructs one, mirroring 06.3's `ConnGaugeGuard` (similarly private at the per-connection level).

4. **Both envoy-bin call sites rebind `drain` to `drain_for_listener` before the `set.spawn(async move { … })` move.** The bare `Arc::clone(&drain)` could have been inlined inside the closure as a positional arg to `.serve(…, Arc::clone(&drain))`, BUT the surrounding `async move { listener.serve(…).await.map_err(…) }` closure captures `listener` and `shutdown` by move — adding the `Arc::clone(&drain)` inline would mean the clone happens INSIDE the moved closure (the inner `&drain` would be a move-captured reference, which is `&Arc<DrainState>` not `&'static Arc<DrainState>`; the compiler rejects this as a borrow that outlives the outer scope). The cleanest fix is to clone OUTSIDE the closure into a fresh local (`drain_for_listener`) and move THAT into the closure. Naming the rebind `drain_for_listener` rather than reusing `drain` makes the move semantics explicit and avoids shadowing the outer `drain` binding still in scope for the admin-handler construction at `main.rs:368-377` (which also needs `Arc::clone(&drain)`).

5. **No new ADRs (per the PLAN architecture-decision lock-in #1: 08.2 ships zero new ADRs).** The RAII guard pattern is a direct mirror of 06.3's `ConnGaugeGuard` — no new design freedom is being claimed at Task 6. The 2-arg widening of `Listener::serve` was authorized at parent-08 SPEC §5.5 / 08.2 SPEC §3 D12 and walked through in the 08.2 PLAN's architecture-decision section (Task 6 spec, "D12 — Listener::serve 2-arg widening"). The ledger head stays **ADR-0032**.

### Confirmations

- **The 2 `#[allow(dead_code)]` annotations on `Listener::listener_manager_active` field + accessor were REMOVED.** Confirmed by:
  - The field declaration at `crates/envoy-listener/src/lib.rs:106` now reads `listener_manager_active: Arc<envoy_stats::Gauge>,` with NO `#[allow(dead_code)]` immediately above.
  - The accessor at `crates/envoy-listener/src/lib.rs:194-196` now reads `pub(crate) fn listener_manager_active(&self) -> &Arc<envoy_stats::Gauge> { &self.listener_manager_active }` with NO `#[allow(dead_code)]` immediately above.
  - Gate 2 (clippy with `-D warnings`) is GREEN — clippy would flag a useless `#[allow(dead_code)]` on a now-consumed item as `clippy::useless_attribute`; the clean clippy verdict is structural evidence the annotations were both removed AND no longer needed.
- **All existing in-file `Listener::serve(...)` test call sites updated to 2-arg.** 8 call sites total — enumerated above under "Work summary" final paragraph. Verified by `grep -c "\.serve(" crates/envoy-listener/src/lib.rs` → 10 matches (8 updated test sites + the 2 new tests' `.serve(...)` calls). Zero remaining 1-arg `.serve(...)` calls anywhere in the crate; the build would not compile otherwise (the unused `drain` arg would not trigger a type error, but every test site that needs the new arg was widened).
- **Both envoy-bin `listener.serve(...)` call sites updated to 2-arg.** tcp_proxy arm at `crates/envoy-bin/src/main.rs:244-253` (was 234-240 pre-Task-6) + HCM arm at `crates/envoy-bin/src/main.rs:343-353` (was 333-338 pre-Task-6). Both now pass `drain_for_listener: Arc<DrainState>` as the new 2nd arg. Verified by Gate 3 (`cargo build --workspace --all-targets`) GREEN — envoy-bin would not compile otherwise.

### LoC delta

| File | Insertions | Deletions |
|---|---|---|
| `crates/envoy-listener/src/lib.rs` | +241 | -47 |
| `crates/envoy-bin/src/main.rs` | +10 | -2 |
| **Total source:** | **+251** | **-49** |

Test-count delta: `envoy-listener` lib bucket grew **28 → 30** (+2, exactly the 2 new tests in `tests` module); no other crate touched. Workspace total grew by the same +2.

No new top-level Cargo deps. `DrainState` was already re-exported from `envoy-listener::lib` at Task 1 (`pub use drain::{DrainStage, DrainState}`) and consumed in-file at this crate's `tests` module via the existing `use super::*;`. `envoy_listener::DrainState` is already on envoy-bin's existing dep graph (Task 4 added the `Arc::new(envoy_listener::DrainState::new(&registry))` startup construction). Crate root `crates/envoy-listener/src/lib.rs` still carries `#![forbid(unsafe_code)]`; zero new unsafe blocks.

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff). One iteration: initial draft of the `serve_returns_when_drain_signal_fires` `tokio::time::timeout(DRAIN_BUDGET + Duration::from_millis(500), serve_handle)` call exceeded the 100-col line limit (rustfmt rewrote it to break across three lines); `cargo fmt --all` applied the reformat in-place, no semantic change.

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean across all 8 workspace crates, zero warnings, zero errors). The 2 `#[allow(dead_code)]` removals were a hard prerequisite — leaving them would have tripped `clippy::useless_attribute` since the field + accessor are now consumed by `ListenerManagerActiveGuard::new`.

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 8 workspace crates + 2 helper bin crates compiled — `envoy-listener` rebuilt because of the widened serve signature + RAII guard + new test call shape; `envoy-bin` rebuilt because of the 2 updated call sites + the new `drain_for_listener` rebind; all downstream test compilation succeeded).

**Gate 4 — `cargo test --workspace`:** PASS — every per-bucket `test result:` line reads `ok. N passed; 0 failed`; the `envoy-listener` lib bucket grew from 28 → 30 tests (the 2 new `tests::{serve_returns_when_drain_signal_fires, serves_honors_shutdown_signal_with_drain_param}`). Focused re-run: `cargo test -p envoy-listener --lib` reads `30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.06s`. No flakes observed on the single authoritative workspace run — the 100ms sleep in `serve_returns_when_drain_signal_fires` did not flake on this run; should it flake on CI, the deviation #2 note above recommends bumping to 250-500ms (still within the DRAIN_BUDGET + 500ms outer 5500ms deadline).

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

**None at the differential / fixture surface.** Task 6 wires the wire-level drain observation into the data-plane listener accept loop, but no shipped fixture (0001-0014) POSTs to `/drain_listeners` — the `drain.drain_signal()` arm in `Listener::serve` is reachable only after the admin POST handler fires `handler.drain().drain()`. Fixture 0015 at Task 8 is the first differential surface to drive a POST `/drain_listeners` against a live envoy-rust + observe both the listener's `serve` returning (graceful drain) and the `listener_manager.total_listeners_active` gauge decrementing to 0. The 08.1 state-4 anchor CI run `25964680619` HEAD `03e6435` remains the authoritative bridge-CI evidence until the 08.2 state-4 anchor at Task 11.

Regression-equivalence on fixtures 0001-0014 preserved by construction: the bilateral `Listener::serve(shutdown, drain)` continues to honor the shutdown arm verbatim (verified by `serves_honors_shutdown_signal` AND the new `serves_honors_shutdown_signal_with_drain_param` test); no fixture issues a drain POST, so the new drain arm never fires within any fixture's scrape window. The `listener_manager.total_listeners_active` gauge increments to 1 (or 2 if a fixture spawns both a tcp_proxy and HCM listener, though no shipped fixture does) at envoy-rust startup and stays at 1 through the entire fixture run — Task 7 (D16 admin scrape extension) will surface the gauge in `/stats` + `/stats/prometheus` so the value is observable, but no current fixture asserts on it. The first differential-fixture surface asserting on the gauge value's drop-to-0 post-drain lands at Task 8 (fixture 0015).
