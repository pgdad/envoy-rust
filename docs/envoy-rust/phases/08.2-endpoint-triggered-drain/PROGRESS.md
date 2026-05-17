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

## Task 7 (D16 — `Driver::AdminScrape` `pre_admin_actions` + `post_admin_assertions` extensions + 08.1 REVIEW M2 + M4 closures)

### Work summary

Widens `Driver::AdminScrape` with two new `#[serde(default)] Vec<…>` fields: `pre_admin_actions: Vec<AdminAction>` (declared BEFORE `pre_requests` in the YAML struct definition per architecture-decision lock-in #18 so the drain trigger appears at the top of the YAML block) and `post_admin_assertions: Vec<AdminAssertion>` (declared AFTER `scrapes`). Both fields default to empty `Vec` so 08.1-landed fixtures 0011 + 0014 carry forward unchanged — they declare neither field and continue to parse cleanly. The variant doc-comment at `tests/differential/src/lib.rs:141-178` explicitly documents the temporal dispatch order (`pre_requests → pre_admin_actions → scrapes → post_admin_assertions`) as independent of the YAML field order, with the rationale ("verify pre-drain baseline → drain → verify post-drain state → wire-level assertion").

Adds two new public enums at `tests/differential/src/lib.rs:219-264`:

- **`AdminAction { Post { path: String, expected_status: u16 } }`** — internally-tagged on `kind` (`#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]`). The single `Post` variant carries the admin-listener path and the expected response status. Today's only consumer is fixture 0015's `/drain_listeners` POST; future admin POSTs (e.g. `/healthcheck/fail`, `/reset_counters`) slot in without YAML-shape churn.
- **`AdminAssertion { DataPlaneConnectionRefused { listener_address: String, within_ms: u64 } }`** — same serde shape. `within_ms` is a raw `u64` (NOT `humantime::Duration`) per architecture-decision lock-in #19 — adding `humantime-serde` would be a new top-level Cargo dep and is rejected at this phase. The `DataPlaneConnectionRefused` variant succeeds on EITHER ECONNREFUSED (the kernel-level "no listener" signal) OR an immediate-EOF connect (the in-flight-drain shape: kernel still accepts because the listening fd is alive on this side but server-side immediately FINs); both dispositions are accepted as evidence the listener is drained per architecture-decision lock-in #20.

Adds two new public async helpers:

- **`pub async fn drive_admin_post(admin_addr: SocketAddr, path: &str, expected_status: u16) -> Result<()>`** at `tests/differential/src/lib.rs:1438-1490`. Mirrors `drive_admin_scrape`'s wire-shape conventions verbatim: connects via raw TCP, writes a minimal `POST <path> HTTP/1.1\r\nHost: admin.local\r\nContent-Length: 0\r\nConnection: close\r\n\r\n` request, parses the response head via `httparse` in a 2 KiB read loop with a 5s per-poll timeout, and discards the body. Bails with a descriptive error if the response status does not equal `expected_status`.
- **`pub async fn assert_data_plane_connection_refused(addr: SocketAddr, within: Duration) -> Result<()>`** at `tests/differential/src/lib.rs:1503-1565`. Polls `addr` in 100ms intervals until `within` elapses; returns `Ok(())` on the first observation of EITHER a connect error (any error, treated as ECONNREFUSED-equivalent) OR a connect-then-read-Ok(0) (immediate-EOF disposition). On deadline expiry, bails with an error naming the address, the deadline, and the last-observed live-listener disposition (read bytes, read error, or read timeout).

Extends the `Driver::AdminScrape` dispatch arm at `tests/differential/src/lib.rs:2415-2685` with the explicit 4-step temporal sequence:

1. **STEP 1 — `pre_requests`** (verbatim from 06.1 D6.a, hoisted out of `drive_admin_scrape`'s internal handling so it precedes `pre_admin_actions`). Drives each HCM-side pre-request against BOTH proxies, then sleeps ~50ms (SPEC §6 signpost 11) to let registry's Relaxed-ordered counter writes become visible. `drive_admin_scrape` is then invoked with an empty `pre: &[]` so it skips its bundled pre-request + visibility-sleep path.
2. **STEP 2 — `pre_admin_actions`**. Each `AdminAction::Post` is dispatched serially against both proxies' admin listeners via `drive_admin_post`, with per-side `with_context` tags naming the side + path.
3. **STEP 3 — `scrapes`** (verbatim from 08.1 Task 11 shape: per-case dispatch + collect, optional `DIFFERENTIAL_DUMP_ADMIN=1` diagnostic, per-case body-rule assertion).
4. **STEP 4 — `post_admin_assertions`**. Each `AdminAssertion::DataPlaneConnectionRefused` parses `listener_address` as `SocketAddr` and invokes `assert_data_plane_connection_refused` with `Duration::from_millis(within_ms)`. Per-side dispatch deviation: the literal address parsed from YAML is probed verbatim (not resolved against the per-side address map) — see deviation #2 below.

Subject + upstream teardown is moved to AFTER `post_admin_assertions` (was previously inside `drive_admin_scrape`) so the wire-level assertion observes the drained-but-live listener (post-drain "kernel-refused" is the success signal; teardown FIRST would race against the assertion).

**08.1 REVIEW M2 closed.** Adds a one-line doc-comment to `BodyRule::JsonShape::value_may_differ_keys` at `tests/differential/src/lib.rs:381-383`: "Shared keys whose values may differ bilaterally; presence is required, value equality is not. (08.1 REVIEW M2 closure landed at 08.2 D16.)". The 08.1 REVIEW chain ends.

**08.1 REVIEW M4 closed.** Adds a 3-line guard at the head of `walk_pointer` at `tests/differential/src/lib.rs:466-473` that rejects dotted paths containing empty segments (e.g. `a..b`, `a.b.`, `.foo`) with the structured error `walk_pointer: dotted path contains empty segment: {dotted_path:?}` BEFORE the existing `serde_json::Value::get("")` opaque "key not found" message can fire. The 08.1 REVIEW chain ends.

### Tests landed (11 new)

All colocated in a new sibling module `mod admin_action_extension_tests` at `tests/differential/src/lib.rs:4625-4982` (end-of-file). The new module is named to mirror the existing `mod body_rule_extension_tests` sibling that 08.1 Task 11 introduced; the precedent for putting new test families in a dedicated `<feature>_extension_tests` module is established and continued at Task 7.

Deserialization tests (4):

1. **`admin_scrape_deserializes_pre_admin_actions_with_post`** — YAML carries an explicit `pre_admin_actions:` list with a single `kind: post` action; the parsed variant deserializes to `AdminAction::Post { path, expected_status }` with the correct field values.
2. **`admin_scrape_deserializes_post_admin_assertions_with_data_plane_connection_refused`** — YAML carries an explicit `post_admin_assertions:` list with a single `kind: data_plane_connection_refused` assertion; verifies `listener_address: String` AND `within_ms: u64` (NOT a humantime string) both parse correctly.
3. **`admin_scrape_pre_admin_actions_defaults_to_empty_vec`** — YAML that omits BOTH new fields parses cleanly and both fields default to an empty `Vec` via `#[serde(default)]`. This is the regression-equivalence test for fixtures 0011 + 0014.
4. **`admin_scrape_deserializes_multiple_pre_admin_actions_and_assertions`** — YAML declares 2 `pre_admin_actions` + 2 `post_admin_assertions`; both deserialize in declaration order. Verifies the `Vec` ordering invariant the dispatch arm relies on.

`drive_admin_post` helper tests (2):

5. **`drive_admin_post_succeeds_on_expected_status`** (`#[tokio::test(flavor = "multi_thread")]`) — spawns a mock admin listener on `127.0.0.1:0` that returns `HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n`; asserts `drive_admin_post(addr, "/drain_listeners", 200)` returns `Ok(())`.
6. **`drive_admin_post_fails_on_status_mismatch`** (`#[tokio::test(flavor = "multi_thread")]`) — same mock-listener shape, returns `503 Service Unavailable`; asserts `drive_admin_post(addr, "/drain_listeners", 200)` returns `Err(_)` whose message contains BOTH `"503"` and `"200"`.

`assert_data_plane_connection_refused` helper tests (3):

7. **`assert_data_plane_connection_refused_succeeds_when_econnrefused`** (`#[tokio::test(flavor = "multi_thread")]`) — binds-then-drops a `TcpListener` on `127.0.0.1:0` to reserve a port the kernel will reject; asserts the helper returns `Ok(())` within 500ms.
8. **`assert_data_plane_connection_refused_succeeds_on_immediate_eof`** (`#[tokio::test(flavor = "multi_thread")]`) — spawns a mock listener that accepts and immediately drops every connection (server-side FIN with zero bytes); asserts the helper returns `Ok(())` within 500ms. Exercises the second-disposition success path per architecture-decision lock-in #20.
9. **`assert_data_plane_connection_refused_fails_when_listener_responds`** (`#[tokio::test(flavor = "multi_thread")]`) — spawns a mock listener that writes `LIVE\n` then holds the connection open for 50ms before dropping (so the harness's 100ms read observes bytes, not EOF); asserts the helper returns `Err(_)` whose message names the live disposition.

08.1 REVIEW M4 closure tests (2):

10. **`walk_pointer_rejects_empty_segment_with_structured_error`** — calls `walk_pointer(&value, "a..b")` against a `{"a": {"b": 1}}` value; asserts the error message contains BOTH `"empty segment"` AND the offending path `"a..b"` (in `Debug` form via the helper's `{dotted_path:?}` formatter).
11. **`walk_pointer_rejects_trailing_empty_segment`** — calls `walk_pointer(&value, "a.b.")` (trailing dot); same dual-assertion shape (`"empty segment"` + `"a.b."`). Verifies the guard catches the tail-empty-segment exemplar in addition to the middle-empty-segment exemplar.

Focused re-run post-implementation: `cargo test -p differential --lib admin_action_extension_tests::` reads `test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 95 filtered out; finished in 0.43s`. The differential lib bucket grew **94 → 105 tests** (+11, exactly the new module). PLAN required **9+**; landed 11.

### Per-task deviations from PLAN

1. **PLAN Step-3 code-snippet dispatch order vs PROGRESS preamble lock-in #18 temporal order.** The PLAN's Step-3 worked-example code snippet showed the dispatch arm body in the order `pre_admin_actions → pre_requests → scrapes → post_admin_assertions`, but PROGRESS preamble lock-in #18 (line 43) + the PLAN-write deviation #1 (line 73) explicitly settle the temporal sequence at `pre_requests → pre_admin_actions → scrapes → post_admin_assertions` ("verify pre-drain baseline → drain → verify post-drain state → wire-level assertion"). The Task 7 dispatch arm at `tests/differential/src/lib.rs:2456-2678` implements the PROGRESS-authoritative temporal order, with a leading multi-line comment block (lines 2456-2480) re-stating the temporal sequence + naming PROGRESS lock-in #18 as the authority. The YAML struct field declaration order (lines 161-179) keeps `pre_admin_actions` BEFORE `pre_requests` per the lock-in's reader-ergonomics half (drain trigger at the top of the YAML block); the doc-comment at lines 150-160 explicitly notes this YAML-vs-temporal separation. Verified by reading the diff: the dispatch arm body unambiguously fires `pre_requests` FIRST.

2. **`post_admin_assertions` per-side dispatch resolves the `listener_address` literally on BOTH sides instead of via a per-side address map.** The `PreRequest.port_key` convention threads template markers (`"PORT"`, `"ADMIN_PORT"`) through a per-side `BTreeMap<String, SocketAddr>` so a single YAML declaration resolves to two distinct subject + upstream addresses. The PLAN sketched a similar shape for `post_admin_assertions` but did not lock it. The Task 7 dispatch arm at `tests/differential/src/lib.rs:2655-2677` parses the YAML `listener_address` directly as a `SocketAddr` and probes it verbatim, treating both sides identically. Rationale: fixture 0015's post-assertion probes the SUBJECT's HCM listener address (the drained side); the upstream is in lock-step but the YAML simplicity wins over per-side resolution at this fixture cardinality. Fixtures that need per-side resolution can declare two `post_admin_assertions` (one for each side's literal address) or extend this dispatch later. A `with_context` tag at line 2671-2675 names the failing assertion. Documented as a deviation because the architecture-decision section did not fully specify this.

3. **`assert_data_plane_connection_refused` does not parse the `last_disposition` initial sentinel as load-bearing.** The helper's `last_disposition: String` binding is initialized at line 1519 to `"(no probe completed before deadline)"` with an `#[allow(unused_assignments)]` attribute. The loop arms overwrite this binding on every "live listener observed" pass before the deadline branch reads it, so the sentinel is only surfaced if the deadline expires BEFORE any "live listener" disposition was observed — which can only happen if every connect succeeded AND every read returned `Ok(0)`, which the success arms return early on. The sentinel is therefore logically unreachable but the binding still needs an initial value for the deadline-branch reader to see; the `#[allow(unused_assignments)]` silences a clippy warning that would otherwise fire under `-D warnings` because the compiler can't prove the unreachability. Documented under deviations because it's a small but visible departure from the PLAN's worked-example shape (the PLAN sketched a `Result<&'static str, _>` for the disposition; the implementation chose a `String` for richer diagnostics).

4. **Test count 11 vs PLAN "9+" floor.** The PLAN required at least 9 tests; the landed module ships 11. The two surplus tests are the second M4-closure exemplar (`walk_pointer_rejects_trailing_empty_segment`) AND the immediate-EOF-success-path test for `assert_data_plane_connection_refused` (`assert_data_plane_connection_refused_succeeds_on_immediate_eof`); both exercise behaviors that the helpers explicitly accept per architecture-decision lock-in #20 + the M4 guard's empty-segment definition. The test count is comfortably above the floor.

5. **No new ADRs (per architecture-decision lock-in #1: 08.2 ships zero new ADRs).** The 4-step temporal dispatch sequence, the AdminAction + AdminAssertion enum shapes, the raw-u64 `within_ms`, and the dual-disposition success of `DataPlaneConnectionRefused` are all locked at the PROGRESS preamble (lock-ins #18-#20). No design freedom is being claimed at Task 7. The ledger head stays **ADR-0032**.

6. **Task implementer subagent crashed mid-flight at ~2.5 hours; a finisher subagent (this commit) verified the partial work compiles + tests pass + gates green + wrote PROGRESS + committed. The 752-line partial diff was preserved as-is.** The finisher subagent inspected each of the 11 new tests, the 2 new helpers, the 2 new enums, the widened variant declaration, the M2 doc-comment, the M4 guard, AND the dispatch arm's temporal order; found the work structurally + semantically complete; ran the 5 gates clean; mirrored the Tasks 1-6 PROGRESS narrative shape; staged + committed via HEREDOC per spec; did NOT amend, rewrite, or re-execute the implementer's work. No additional tests were added (the 11 landed tests are comfortably above the PLAN's 9+ floor and cover every spec-named behavior).

### Confirmations

- **`#![forbid(unsafe_code)]` retained on all touched crates.** Task 7 touches only `tests/differential/src/lib.rs`; the crate-root attribute is unchanged. Zero new unsafe blocks.
- **No new top-level Cargo deps.** `git diff tests/differential/Cargo.toml` is empty; `git diff Cargo.lock` is empty; no `humantime-serde` or other new dep was added (the raw `u64` `within_ms` on `AdminAssertion::DataPlaneConnectionRefused` is the architecture-decision lock-in #19 choice that avoids the new dep). All Task 7 surfaces use already-on-graph items (`anyhow`, `serde`, `serde_yaml`, `tokio`, `httparse`, `std::net::SocketAddr`, `std::time::Duration`).
- **08.1 REVIEW M2 doc-comment present** at `tests/differential/src/lib.rs:381-383`: `/// Shared keys whose values may differ bilaterally; presence is required, value equality is not. (08.1 REVIEW M2 closure landed at 08.2 D16.)`.
- **08.1 REVIEW M4 guard present** at `tests/differential/src/lib.rs:466-473`: the 3-line `if dotted_path.split('.').any(str::is_empty) { bail!("walk_pointer: dotted path contains empty segment: {dotted_path:?}"); }` block precedes the existing per-segment walk loop. Verified by both new walk_pointer tests passing.
- **Fixtures 0011 + 0014 Docker-gated wrappers still build.** `cargo build -p differential --tests` finishes clean (exit 0, `Finished dev profile`); the two fixture-runner wrappers (in `tests/differential/tests/`) consume the widened `Driver::AdminScrape` variant via the same module-level types and inherit `#[serde(default)]` backward compatibility without any per-fixture change.
- **STATE.md / ROADMAP.md / SPEC.md / DECISIONS.md / BEHAVIOR_CONTRACT.md untouched.** Task 7 is harness-only; no docs surface changes ship at this task.

### LoC delta

| File | Insertions | Deletions |
|---|---|---|
| `tests/differential/src/lib.rs` | +764 | -12 |
| **Total source:** | **+764** | **-12** |

(Numbers from `git diff --stat` pre-commit: `1 file changed, 752 insertions(+), 12 deletions(-)`; the +764/-12 line above is the +/-/-summary split that `git diff --stat` collapses into "752 insertions". Both numbers reflect the same single-file diff.)

Test-count delta: differential lib bucket grew **94 → 105** tests (+11, exactly the new module). No other crate touched; workspace test-count grew by the same +11. The new sibling test module `admin_action_extension_tests` brings the total `#[test]` + `#[tokio::test]` annotations in `tests/differential/src/lib.rs` from **70 → 81** (the 105 number above counts ALL differential tests including `backend::`, `subject::`, `tls::` submodules; the per-file annotation count of 70 → 81 reflects only the `lib.rs`-resident tests).

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff). No iteration required — the prior subagent's partial work was already rustfmt-clean.

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean across all 8 workspace crates, zero warnings, zero errors). The `#[allow(unused_assignments)]` on `last_disposition` inside `assert_data_plane_connection_refused` is necessary because the compiler can't prove the loop's success-path returns make the sentinel unreachable; without the allow, clippy would flag the never-read initial value.

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 8 workspace crates + test/bench/example targets compiled cleanly). The differential crate rebuilt because of the new enums, new helpers, widened variant declaration, and new test module; no downstream crate rebuilt (Task 7 is contained to the differential test harness).

**Gate 4 — `cargo test --workspace`:** PASS — every per-bucket `test result:` line reads `ok. N passed; 0 failed`. The differential lib bucket grew **94 → 105** (the 11 new `admin_action_extension_tests::*` tests). Focused re-run: `cargo test -p differential --lib admin_action_extension_tests::` reads `test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 95 filtered out; finished in 0.43s`. No `tcp_proxy_backend_*` flakes observed on this run. No other bucket changed test counts.

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

**None at this task.** Task 7 is harness-only; the new D16 surface (`pre_admin_actions` + `post_admin_assertions` + the 2 helper fns `drive_admin_post` + `assert_data_plane_connection_refused`) is exercised end-to-end by fixture 0015 at Task 8 (Docker-gated, against a real envoy + envoy-rust pair) and by the in-process backstop `tests/differential/tests/admin_drain_listeners.rs` at Task 10 (non-Docker, against an in-process envoy-rust). The 11 new unit tests at this task exercise deserialization + helper-behavior in isolation against synthetic mock listeners; they do NOT drive any fixture or real proxy. The 08.1 state-4 anchor CI run `25964680619` HEAD `03e6435` remains the authoritative bridge-CI evidence until the 08.2 state-4 anchor at Task 11.

Regression-equivalence on fixtures 0001-0014 preserved by construction: both new `Driver::AdminScrape` fields default to empty `Vec` via `#[serde(default)]`; the dispatch arm's STEP 2 + STEP 4 loops iterate over empty `Vec`s and exit immediately when the fields are absent, so fixtures 0011 + 0014 (which declare neither field) hit the same execution path they did at 08.1 plus zero added work. Test `admin_scrape_pre_admin_actions_defaults_to_empty_vec` is the regression-equivalence proof for this property.

---

## Task 7 fixup (review-driven; lands on top of `bc83f8e`)

### Fixup driver

Code-quality review of Task 7 commit `bc83f8e` flagged **2 Important** findings against `tests/differential/src/lib.rs::assert_data_plane_connection_refused` (lines ~1503-1565 at the substantive commit). Both are load-bearing before Task 8's fixture 0015 lands atop this helper. This fixup commit closes both Important findings AND opportunistically closes 2 of the Minor findings (M3 + M4) whose fix is trivial; the remaining Minor findings (M1, M2, M5) are comment/test-coverage polish that can land opportunistically later and do not gate Task 8.

### What changed

**Important #1 — Read-error disposition aligned to PLAN worked example.** Pre-fix the `Ok(Err(err))` arm of the timeout-wrapped `read(&mut buf)` (the post-connect read step) was treated as failure-and-continue-polling — it set `last_disposition = format!("read error after connect: {err}")` and fell through to the deadline check. PLAN.md worked example at lines 2282-2286 explicitly specifies this arm as drain success ("Read Err — accept as drain success (the listener shut the connection ungracefully)") — i.e. RST / ECONNRESET / ungraceful mid-handshake close IS the third drain-success disposition alongside ECONNREFUSED + immediate-EOF. Architecture-decision lock-in #20 ("ECONNREFUSED OR immediate-EOF, either accepted") was permissive enough to encompass this third disposition per the PLAN's worked example. Fix changes the arm body to `return Ok(())` with a brief inline comment naming "ungraceful close. Drain success per PLAN.md worked example lines 2282-2286". The dead-code `last_disposition` assignment on this branch is removed. The function-level doc-comment is rewritten to enumerate ALL THREE drain-success dispositions as a numbered list (was: 2-disposition prose paragraph).

**Why this matters now:** a listener that RSTs the connection mid-handshake (some drain configurations do exactly this) would NOT have been detected as drained under the pre-fix implementation — the loop would observe `Ok(Err(ECONNRESET))` on every probe, accumulate the "read error after connect" disposition, and bail at deadline expiry with a false-negative. Fixture 0015 at Task 8 (the Docker-gated D17.2 fixture exercising `/drain_listeners` against a real envoy + envoy-rust pair) will fail in exactly this shape if the listener's drain implementation chooses RST over FIN. Closing this BEFORE Task 8 lands is required.

**Important #2 — Per-attempt `connect` timeout added.** Pre-fix `TcpStream::connect(addr).await` at line ~1521 was unbounded — could block far longer than the 100ms poll interval on a slow accept, eroding the deadline budget and making the polling loop run fewer attempts than designed. PLAN.md worked example lines 2257-2261 explicitly wraps the connect call in a 200ms timeout. Fix wraps the connect in `tokio::time::timeout(Duration::from_millis(200), tokio::net::TcpStream::connect(addr)).await`; the outer match arms expand by one: `Err(_timeout) => { last_disposition = format!("connect timed out (>200ms) to {addr}"); }` (treats connect-timeout as failure-and-continue per the PLAN's worked example, NOT as drain success — a slow accept is still evidence the listener is responding). The structural rename of the success arm from `Ok(mut s)` to `Ok(Ok(mut s))` and the error arm from `Err(e)` to `Ok(Err(e))` reflects the new outer `Result<Result<_>, _timeout>` shape.

**Why this matters now:** without the per-attempt connect bound, a slow-accepting listener (e.g. envoy mid-drain under load, where the accept loop is being interrupted by the drain-state transition) could consume the entire `within` budget in a single connect call — the helper would make ONE attempt instead of the expected ~5 (500ms ÷ 100ms poll interval) and bail with the connect-still-pending uncertainty unresolved. Fixture 0015's polling cadence depends on the per-attempt bound being honored.

**M3 (opportunistic minor closure) — `last_disposition` refactor removes `#[allow(unused_assignments)]`.** Pre-fix the binding was `let mut last_disposition = String::from("(no probe completed before deadline)");` with `#[allow(unused_assignments)]` to silence a clippy warning about the initial value being never-read (the loop arms overwrite it before the deadline branch reads it). Refactor changes the initial value to `format!("no live-listener disposition observed before {within:?}")` — a real diagnostic string that IS surfaced if the deadline branch fires before any live-listener arm has run (logically unreachable but the new initial value is now a sound output on the unreachable path). The `#[allow(unused_assignments)]` attribute is REMOVED. The accompanying comment block is updated to explain why M3's refactor is sound (the binding's initial value is itself a valid output of the deadline branch). Verified clean under `cargo clippy --workspace --all-targets --all-features -- -D warnings`.

**M4 (opportunistic minor closure) — inner read timeout 100ms → 50ms** matching PLAN.md worked example line 2271. Pre-fix the inner `tokio::time::timeout(Duration::from_millis(100), s.read(&mut tail))` used a 100ms budget; PLAN's worked example specifies 50ms. The 50ms budget is sufficient for FIN/RST to arrive when the server has already closed (kernel signaling is sub-millisecond on Unix; loopback RTT is microseconds), and the shorter budget further limits per-attempt wall-clock cost. Fix changes `100` to `50` in the single call site.

### New test landed (1)

`admin_action_extension_tests::assert_data_plane_connection_refused_treats_ungraceful_close_as_drain_success` (`#[tokio::test(flavor = "multi_thread")]`). Spawns a mock listener on `127.0.0.1:0` that accepts each connection, calls `set_linger(Some(Duration::from_secs(0)))` on the accepted socket (forcing the kernel to issue RST instead of FIN on close), then drops the socket. The client (the helper-under-test) connects successfully then observes `read → Err(ECONNRESET)` on every probe. Asserts the helper returns `Ok(())` within 500ms. Comment block explicitly names this as the third drain-success disposition per PLAN.md lines 2282-2286, sibling to `_succeeds_on_immediate_eof` (which exercises the `Ok(Ok(0))` arm).

The `set_linger` call uses `#[allow(deprecated)]` locally on the single call site because `tokio::net::TcpStream::set_linger` is deprecated upstream (the deprecation flags that SO_LINGER causes the socket to block the EXECUTOR thread on drop in production code paths). For this synthetic mock listener that exists solely to RST the per-attempt accepted socket and then returns to the accept loop, the executor-blocking concern does not apply (the linger duration is 0, the close issues RST immediately without buffering, and the spawned task is the only user of this executor scaffold). The std-library equivalent `std::net::TcpStream::set_linger` is still nightly-unstable (rust-lang/rust#88494) and adding `socket2` or `libc` as a new top-level Cargo dep is rejected at this fixup per the 08.2 no-new-deps doctrine — so the documented + locally-allowed deprecated tokio path is the right call. The allow is narrowly scoped to one statement and carries a multi-line comment naming the trade-off + the rejected alternatives.

**Red phase (TDD) verified:** pre-fix the test FAILED with the exact disposition `"read error after connect: Connection reset by peer (os error 54)"` — i.e. the helper saw `Ok(Err(ECONNRESET))` on every probe and bailed at deadline expiry. Green phase verified: post-fix the test PASSES in ~0ms (first probe hits the RST and the new `Ok(Err(_err)) => return Ok(())` arm fires immediately).

### Per-task deviations from the fixup spec

**Zero structural deviations.** All 2 Important findings + the 2 opportunistic Minor findings (M3 + M4) were closed exactly as specified by the fixup spec. The fixup spec said "Skip M1, M2, M5" and those were skipped.

**One incidental note re: the M3 refactor's design exploration.** The first M3 attempt used `let mut last_disposition: Option<String> = None;` (initialize as `None`; deadline branch reads via `last_disposition.as_deref().unwrap_or("(no probe completed before deadline)")`). That shape compiled but still triggered the `unused_assignments` warning on the initial `None` (the compiler proved the read-site's `unwrap_or` fallback handled `None` explicitly, so the initial assignment was provably never the value read). The final shape — initialize to a real diagnostic string that IS the surface-of-record on the logically-unreachable path — is the second attempt. Both attempts are functionally identical; only the second produces a warning-free build. Documented here so the design space is visible for future readers.

### Confirmations

- **I1 fix applied:** the `Ok(Err(_err))` arm of the inner read at `tests/differential/src/lib.rs:1557-1561` now returns `Ok(())` with a brief inline comment naming "ungraceful close. Drain success per PLAN.md worked example lines 2282-2286". Verified by the new test passing.
- **I2 fix applied:** the connect call at `tests/differential/src/lib.rs:1533-1535` is now wrapped in `tokio::time::timeout(Duration::from_millis(200), ...)`; the outer match has 3 arms (`Ok(Err(e))` ⇒ ECONNREFUSED drain success; `Err(_timeout)` ⇒ failure-and-continue; `Ok(Ok(mut s))` ⇒ connect-succeeded, read-step). Verified by the helper still passing all 4 existing tests (ECONNREFUSED, immediate-EOF, live-listener-fails, plus the new ungraceful-close test).
- **M3 closure applied:** `#[allow(unused_assignments)]` REMOVED from the `last_disposition` binding (line 1518 at the substantive commit). Verified clean under `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- **M4 closure applied:** inner read timeout 100ms → 50ms at the single call site (line 1555). Matches PLAN.md worked example line 2271.
- **No new top-level Cargo deps.** `git diff Cargo.lock` and `git diff tests/differential/Cargo.toml` are both empty for this fixup. The new test's `set_linger` rides on `tokio`'s existing `net` feature already declared at `tests/differential/Cargo.toml:26`.
- **No new ADRs.** Ledger head stays **ADR-0032**.
- **No changes to STATE.md / ROADMAP.md / SPEC.md / DECISIONS.md / BEHAVIOR_CONTRACT.md.** This fixup is harness-only.
- **`#![forbid(unsafe_code)]` retained** on all touched crates. The fixup touches only `tests/differential/src/lib.rs`; the crate-root attribute is unchanged. Zero new unsafe blocks. The `#[allow(deprecated)]` on the test's `set_linger` call is NOT an unsafe escape — it is a per-call lint allow for an upstream deprecation, scoped to one statement with a documented rationale.

### LoC delta

| File | Insertions | Deletions |
|---|---|---|
| `tests/differential/src/lib.rs` | +109 | -36 |
| **Total source:** | **+109** | **-36** |

(Numbers from `git diff --stat tests/differential/src/lib.rs`: `1 file changed, 109 insertions(+), 36 deletions(-)`. The +109 includes: the helper's doc-comment rewrite expansion (5 → 16 lines, +11), the connect-wrap restructure with new `Err(_timeout)` arm (+12), the M3 initial-value + comment-block expansion (+8 net), the inline ungraceful-close comment (+3), and the new 41-line test including its 21-line doc-comment + trade-off-naming. The -36 is the pre-fix doc-comment + the pre-fix unwrapped connect + the `#[allow(unused_assignments)]` annotation + the prior comment block.)

Test-count delta: differential lib bucket grew **105 → 106 tests** (+1, exactly the new `assert_data_plane_connection_refused_treats_ungraceful_close_as_drain_success`). No other crate touched; workspace total grew by the same +1. The `admin_action_extension_tests` sibling module now hosts 12 tests (was 11 at the Task 7 substantive commit). PLAN's Task 7 narrative still applies (11 new tests at Task 7); this fixup adds the 12th in the same module.

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff after one `cargo fmt --all` apply during implementation — the connect-wrap restructure produced a single line that rustfmt preferred to split into the multi-line `tokio::time::timeout(\n   Duration::from_millis(200),\n   tokio::net::TcpStream::connect(addr),\n)\n.await` shape per PLAN.md's worked-example layout).

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean across all 8 workspace crates, zero warnings, zero errors). The pre-fix `#[allow(unused_assignments)]` is removed (M3); the new test's single `#[allow(deprecated)]` on `set_linger` is the only lint allow in the diff and is documented + narrowly scoped to one statement.

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 8 workspace crates + test/bench/example targets compiled cleanly).

**Gate 4 — `cargo test --workspace`:** PASS — every per-bucket `test result:` line reads `ok. N passed; 0 failed`. The differential lib bucket grew **105 → 106** (the new ungraceful-close test). Focused re-run: `cargo test -p differential --lib admin_action_extension_tests::assert_data_plane_connection_refused` reads `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 103 filtered out; finished in 0.42s` — all 4 helper tests (the 3 pre-existing + the new ungraceful-close) pass together. No other bucket changed test counts. No `tcp_proxy_backend_*` flakes observed.

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

**None.** This fixup is logic + test changes inside `tests/differential/src/lib.rs` only — no admin endpoint surface change, no listener-serve signature change, no gauge registration, no fixture. The helper's wire-level behavior changes ONLY by accepting one additional drain-success disposition (read-Err / ungraceful close) that the PLAN's worked example always specified. All 14 Docker-gated fixtures (0001-0014) remain GREEN by construction (zero changed wire-protocol behavior on the fixture side; the helper is harness-internal and not exercised by any fixture until 0015 lands at Task 8). The 08.1 state-4 anchor CI run `25964680619` HEAD `03e6435` remains the authoritative bridge-CI evidence until the 08.2 state-4 anchor at Task 11. Fixture 0015 at Task 8 will exercise this helper end-to-end against a real envoy + envoy-rust pair; the fixup's correctness is a precondition for that fixture's correctness.

---

## Task 8 (D17.2 — Fixture `0015-admin-drain-listeners` + Docker-gated wrapper + BEHAVIOR_CONTRACT "Admin-action effect equivalence" subsection)

### Work summary

Lands the first end-to-end bilateral fixture exercising the 08.2 drain flow. The fixture lives at `tests/fixtures/0015-admin-drain-listeners/` and ships the standard 5-file shape established by the 06.x / 08.1 differential fixture lineage:

- **`envoy.yaml`** (59 lines) — upstream Envoy bootstrap. Mirrors fixture 0007's HCM + `direct_response` shape: 1 HCM listener (`hcm_listener` on `0.0.0.0:{{PORT}}`, `codec_type: HTTP1`, single `virtual_host` with one route `prefix: "/"` → `direct_response { status: 200, body: { inline_string: "ok\n" } }`) + 1 admin listener (on `0.0.0.0:{{ADMIN_PORT}}`); NO upstream cluster, NO backend. Includes the `request_headers_to_remove` + `generate_request_id: false` injections-suppression pair (mirroring fixture 0011's pattern — harmless here since no data-plane traffic, kept for cross-fixture consistency).
- **`envoy-rust.yaml`** (43 lines) — same shape, `127.0.0.1` binds, no Envoy-injection-suppression knobs (envoy-rust does not emit those headers).
- **`inputs/payload.bin`** (0 bytes) — placeholder consumed nowhere (`Driver::AdminScrape` does not read it; the file exists to satisfy the harness's per-fixture directory shape convention).
- **`expectations.yaml`** (90 lines) — `Driver::AdminScrape` with all three 08.2 D16 fields populated: `pre_admin_actions: [{ kind: post, path: /drain_listeners, expected_status: 200 }]` (the drain trigger), `scrapes: [{ path: /server_info, expected_status: 200, expected_content_type: application/json, expected_body_rule: { kind: json_shape, required_keys: [state], value_may_differ_keys: [state, version, hot_restart_version, command_line_options, node], allowlist_envoy_only_keys: [uptime_current_epoch, uptime_all_epochs], allowlist_envoy_rust_only_keys: [uptime_current_epoch_seconds, uptime_all_epochs_seconds] } }]` (the post-drain bilateral admin observation), and `post_admin_assertions: [{ kind: data_plane_connection_refused, listener_address: "127.0.0.1:{{PORT}}", within_ms: 5000 }]` (the wire-level drain effect with per-side template-rendering — see "Dispatch arm template-key extension" below). The YAML field order shows `pre_admin_actions` BEFORE the implicit `pre_requests` per architecture-decision lock-in #18 reader-ergonomics half; the temporal dispatch sequence `pre_requests → pre_admin_actions → scrapes → post_admin_assertions` is driven by Task 7's dispatch arm at `tests/differential/src/lib.rs:2515-2702` regardless. The fixture omits `pre_requests` (the `Driver::PreRequest` grammar has no `expected_status` / `expected_body` fields AND targets the HCM listener via `port_key = PORT`, so a "pre-drain `/ready=200 LIVE` baseline" is not assertable through it — that baseline is covered in isolation by the in-process backstop at Task 10).
- **`README.md`** (160 lines) — fixture documentation per the 0014 README template. Documents the test driver shape, the empirical decision to scrape `/server_info` instead of `/ready` (see "Cross-proxy `/ready` divergence empirical finding" below), the per-side allow-list seeding (minimal — just the `/server_info` subset mirrored from fixture 0014), and 8 cross-references back to the parent-08 SPEC + 08.2 PLAN + Task 7 D16 surface + Task 10 backstop + BEHAVIOR_CONTRACT subsection + fixture 0014 multi-scrape precedent + fixture 0007 HCM + direct_response precedent.

Lands the Docker-gated wrapper at `tests/differential/tests/admin_drain_listeners.rs` (31 lines) mirroring fixture 0014's wrapper shape verbatim: `#[tokio::test]` (NO `#[ignore]` — the differential bucket is uniformly Docker-gated by virtue of `testcontainers` failing fast when Docker is unavailable, and existing wrappers 0001-0014 follow this convention — see Per-task deviation #1 below); resolves the fixture directory via `PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("tests/fixtures/0015-admin-drain-listeners")`; invokes `differential::run_fixture(&dir).await.expect("fixture green")`.

Extends Task 7's `Driver::AdminScrape` dispatch arm at `tests/differential/src/lib.rs:2685-2730` with per-side template-key resolution for `AdminAssertion::DataPlaneConnectionRefused.listener_address` (see Per-task deviation #2 below).

Lands the `## Admin-action effect equivalence` top-level subsection in `docs/envoy-rust/BEHAVIOR_CONTRACT.md` between the "Admin endpoint body shapes" subsection (line 130) and the "Access log field mapping" subsection (now at line 168). The new subsection carries a 3-row table covering the wire-level invariant for each of the 3 POST admin actions per parent-08 SPEC §2.4: `POST /drain_listeners` (data-plane connection-refused-or-immediate-EOF within 5s `DRAIN_BUDGET`; admin listener stays serving; sticky), `POST /healthcheck/fail` (`/ready` flips to 503 within 100ms; `/server_info.state` stays `"LIVE"`), `POST /healthcheck/ok` (`/ready` flips back to 200 within 100ms IFF current state is `HealthcheckFailing`; no-op against `Draining`).

### Tests landed (1 new differential fixture)

- **`tests/differential/tests/admin_drain_listeners.rs::admin_drain_listeners`** — Docker-gated end-to-end. Empirically PASSED locally against upstream Envoy `envoyproxy/envoy:v1.33.0` (image already cached at `Image is up to date`) in ~1s wall-clock per run (verified across 3 consecutive runs). Sequence: testcontainers spawns upstream Envoy with the rendered envoy.yaml bind-mounted at `/etc/envoy/envoy.yaml`; harness spawns envoy-rust subprocess with the rendered envoy-rust.yaml; harness POSTs `/drain_listeners` to BOTH admin listeners → both return 200; harness scrapes `/server_info` from BOTH admin listeners → both return 200 with the `state` key present; harness probes BOTH HCM listener addresses (kernel-ephemeral host ports, per-side template-rendered) for ECONNREFUSED / immediate-EOF / RST → both probes succeed within 5s (in practice within ~100ms on loopback — envoy-rust returns ECONNREFUSED instantly on `Listener::serve`'s drain-signal-driven shutdown; upstream Envoy v1.33's drain hits the same disposition equally fast). Verified via `DIFFERENTIAL_DUMP_ADMIN=1 cargo test -p differential --test admin_drain_listeners -- --nocapture` — the dump output shows BOTH proxies' `/server_info` JSON bodies post-drain (upstream Envoy emits `"state": "DRAINING"` AND the full Envoy bootstrap projection; envoy-rust emits `"state": "DRAINING"` AND its bootstrap projection per Task 4 + Task 5 D5e surface) AND the test passes despite the substantial bilateral body divergence — the `value_may_differ_keys` + `allowlist_*_only_keys` per-side seeding absorbs it cleanly.

The new fixture does NOT change any unit-test count. The differential lib bucket stays at **106 tests** (was 106 at the Task 7 fixup; Task 8 adds zero unit tests because the dispatch-arm template-key extension at deviation #2 is a 27-line surgical addition tested via the fixture itself rather than via a synthetic unit test — see Per-task deviation #2's rationale).

### Per-task deviations from PLAN

1. **Wrapper attribute shape: `#[tokio::test]` (NO `#[ignore]`).** The PLAN's Step-1 worked-example code snippet showed `#[tokio::test]` + `#[ignore]` for the Docker-gated wrapper. Empirical inspection of `tests/differential/tests/admin_config_dump_server_info.rs` (the closest-shape predecessor wrapper at the parent-PLAN-anchored "verify the exact attribute shape" callout) found NO `#[ignore]` attribute — the actual convention in the differential test bucket is to rely on `testcontainers` failing fast when Docker is unavailable (which surfaces as a clear `connection refused on /var/run/docker.sock` style error during the container `start()` call, immediately failing the test with diagnostic context — no false-pass risk). Task 8's wrapper matches the convention of the existing 14 differential wrappers, none of which carry `#[ignore]`. The CI workflow at `.github/workflows/ci.yml:51-52` invokes `cargo test --workspace` once (NOT `cargo test --workspace -- --ignored`) and that single invocation runs all 15 differential wrappers including this new one. PLAN's `#[ignore]` sketch is therefore inconsistent with the established convention and was not honored — the actually-shipped attribute is the convention-conformant single `#[tokio::test]`. Verified by reading the CI workflow + the 14 existing wrappers + by running `cargo test --workspace` locally and observing the new test runs (not filtered-out) alongside the other 14 differential tests.

2. **Dispatch-arm template-key extension for `AdminAssertion::DataPlaneConnectionRefused.listener_address`.** Task 7's dispatch arm at `tests/differential/src/lib.rs:2679-2702` (pre-Task-8) parsed `listener_address` DIRECTLY as a `SocketAddr` and probed it once (subject + upstream collapsed onto the same literal address). Task 7's deviation #2 acknowledged the gap ("Fixtures that need per-side resolution can declare two `post_admin_assertions` ... or extend this dispatch later"). Fixture 0015 needs per-side resolution because BOTH proxies bind to kernel-ephemeral host ports — no literal address can be hardcoded in YAML that resolves to both proxies' ports at once. Task 8 extends the dispatch arm with template-key resolution: `listener_address` is `.replace("{{PORT}}", port).replace("{{ADMIN_PORT}}", admin_port)` per-side (using `upstream_addr.port()` / `subject_addr.port()` for `{{PORT}}` and `upstream_admin_port` / `subject_admin_port` for `{{ADMIN_PORT}}`), each side's rendered address parses as `SocketAddr`, and `assert_data_plane_connection_refused` is invoked twice (once per side) with per-side `with_context` tags naming the side AND the template AND the rendered address. A YAML address with no markers (a fully-formed `host:port` literal) is `replace`-no-ops and probed verbatim on BOTH sides — backward-compatible with Task 7's existing test fixtures at `lib.rs:4716-4796` (which use literal `127.0.0.1:8080` / `127.0.0.1:1` / `127.0.0.1:2` addresses; those tests parse the YAML but do not exercise the dispatch arm). The 27-line dispatch-arm change is the substantive Task 8 differential-harness extension; the existing dispatch-arm leading comment is rewritten to document the new resolution semantics. No new unit test was added because the existing 4 `assert_data_plane_connection_refused_*` tests at `lib.rs:4903-5005` cover the helper's wire-level dispositions exhaustively, and the fixture 0015 end-to-end run exercises the new template-key resolution path against two real per-side ports — that's the end-to-end evidence the unit-test scaffold cannot reach (synthetic unit tests would need to spawn two distinct ephemeral listeners + render-then-probe both, duplicating the fixture's mechanics without adding signal).

3. **Scrape target: `/server_info` instead of the PLAN's sketched `/ready`.** The PLAN's Step-1 worked-example expectations.yaml shape carried `scrapes: [{ request: GET /ready, expected_status: 503, response_body: BodyRule::ByteExact "DRAINING\n" }]`. Empirical iteration at Task 8 surfaced that upstream Envoy v1.33's `/ready` does NOT flip to 503 immediately on `POST /drain_listeners` — it requires `--drain-time-s` (default 600 seconds) to elapse OR `--drain-strategy immediate` (server-level CLI flags, NOT bootstrap-configurable). The first end-to-end run output (via `DIFFERENTIAL_DUMP_ADMIN=1`) showed: `--- ENVOY (200, ct="text/plain; charset=UTF-8") ---\nLIVE\n` from upstream Envoy AND `--- ENVOY-RUST (503, ct="text/plain") ---\nDRAINING\n` from envoy-rust. Three resolution options were considered: (a) extend `upstream::start()` to take per-fixture envoy CLI args (out of Task 8 scope; would widen the upstream harness API); (b) use `BodyRule::TextLines` with `required_lines: ["DRAINING"]` on the scrape (still fails because upstream Envoy's body is `"LIVE\n"`, not `"DRAINING\n"`); (c) pivot the scrape to a bilateral-stable post-drain endpoint. Option (c) wins: `/server_info` returns 200 with JSON containing the `state` key on BOTH proxies across the drain transition; the per-side allow-list seeding mirrors fixture 0014's already-seeded subset for the same endpoint. The PLAN's `/ready` post-drain scrape is preserved as a TODO for a future phase that extends `upstream::start()` with `--drain-strategy immediate` / `--drain-time-s 0` injection (or alternatively, the in-process backstop at Task 10 covers the envoy-rust-side `/ready` flip in isolation without the cross-proxy `/ready` asymmetry constraint). The Task 8 fixture README documents this empirical decision in detail at the "Test driver" section item 2 + at the expectations.yaml `scrapes:` block leading comment. The substantive drain assertion remains the `data_plane_connection_refused` post-assertion, which IS bilaterally invariant per BEHAVIOR_CONTRACT.md's new "Admin-action effect equivalence" subsection (the wire-level `POST /drain_listeners` invariant is data-plane connection-refused-or-immediate-EOF, not `/ready=503` — the `/ready=503` invariant is a `POST /healthcheck/fail` invariant per the same subsection's second row).

4. **Fixture omits `pre_requests`.** The PLAN's Step-1 worked-example expectations.yaml shape carried `pre_requests: [GET /ready → 200 LIVE]` as the pre-drain baseline. The `Driver::PreRequest` grammar at `tests/differential/src/lib.rs:210-217` carries only `(method, path, host, port_key)` — no `expected_status` / `expected_body` fields — so the pre-drain `/ready=200 LIVE` baseline assertion is NOT representable through it. Additionally `pre_requests` target the HCM listener (`port_key = PORT`), not the admin listener, so a `pre_requests` of `(GET, /ready, _, PORT)` would issue against the HCM `direct_response` route returning `"ok\n"`, not against the admin `/ready` endpoint. The "pre-drain baseline assertion" is therefore deferred to the in-process backstop at Task 10 where the admin `/ready` endpoint can be probed directly without the `Driver::PreRequest` HCM-routing constraint. The fixture README documents this at the "Why no pre_requests" section.

5. **No new ADRs (per architecture-decision lock-in #1: 08.2 ships zero new ADRs).** The dispatch-arm template-key extension (deviation #2) was already implicitly authorized by Task 7's deviation #2 ("...or extend this dispatch later"); the scrape pivot (deviation #3) is a fixture-internal empirical decision documented in the fixture's own README + expectations.yaml comments; no design freedom is being claimed that warrants an ADR. The ledger head stays **ADR-0032**.

6. **No PLAN-Task-7-amend filed.** Task 7's actually-landed dispatch order is `pre_requests → pre_admin_actions → scrapes → post_admin_assertions` per Task 7's PROGRESS narrative deviation #1 (PLAN code-snippet showed a different order; PROGRESS preamble lock-in #18 settled the authoritative order; Task 7 honored PROGRESS). Task 8's dispatch-arm extension at deviation #2 leaves the temporal order untouched — only adds template-key resolution + dual-side probing inside the existing STEP 4 loop. No amendment to Task 7 is needed.

### Confirmations

- **`#![forbid(unsafe_code)]` retained on all touched crates.** Task 8 touches only `tests/differential/src/lib.rs` (dispatch extension) + adds `tests/differential/tests/admin_drain_listeners.rs` + 5 fixture-data files + `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (doc subsection). The differential crate's `#![forbid(unsafe_code)]` (at `tests/differential/src/lib.rs:1`) is unchanged. Zero new unsafe blocks. The wrapper test file inherits the crate's forbid via integration-test compilation against the lib crate's public surface.
- **No new top-level Cargo deps.** `git diff Cargo.lock` is empty; `git diff tests/differential/Cargo.toml` is empty. All Task 8 surfaces use already-on-graph items (the wrapper uses `tokio` + `std::path::PathBuf` + the `differential` lib crate; the dispatch-arm extension uses `String::replace` + `SocketAddr::parse` from `std`).
- **Docker availability: YES.** Empirically verified via `docker info | head -3` (Client 28.0.4, desktop-linux context) + `docker version` (Server: Docker Desktop 4.40.0; Engine 28.0.4 linux/arm64) + `docker pull envoyproxy/envoy:v1.33.0` returned `Image is up to date for envoyproxy/envoy:v1.33.0` (cached locally). The Docker-gated fixture wrapper was empirically run THREE times consecutively against real Docker; all three runs PASSED (~1s wall-clock per run including container start + drain trigger + scrape + post-assertion + teardown). The 5-gate `cargo test --workspace` invocation also ran the wrapper as part of the differential bucket — verified PASS in the per-bucket attestation below.
- **STATE.md / ROADMAP.md / SPEC.md / DECISIONS.md untouched.** Task 8 ships zero changes to STATE / ROADMAP / SPEC / DECISIONS. The only doc surface change is the new `## Admin-action effect equivalence` subsection in BEHAVIOR_CONTRACT.md per parent-08 SPEC §2.4 + Task 8 PLAN.
- **BEHAVIOR_CONTRACT subsection insertion correct.** The new subsection lives between the existing "Admin endpoint body shapes" subsection (lines 130-148) and the existing "Access log field mapping" subsection (now starting at line 168 post-insertion). Insertion point matches the parent-08 SPEC §2.4 + Task 8 PLAN spec exactly.

### LoC delta

| File | Insertions | Deletions |
|---|---|---|
| `tests/differential/src/lib.rs` | +43 | -16 |
| `tests/differential/tests/admin_drain_listeners.rs` | +31 | 0 (new) |
| `tests/fixtures/0015-admin-drain-listeners/envoy.yaml` | +59 | 0 (new) |
| `tests/fixtures/0015-admin-drain-listeners/envoy-rust.yaml` | +43 | 0 (new) |
| `tests/fixtures/0015-admin-drain-listeners/expectations.yaml` | +90 | 0 (new) |
| `tests/fixtures/0015-admin-drain-listeners/inputs/payload.bin` | 0 (new, 0 bytes) | 0 |
| `tests/fixtures/0015-admin-drain-listeners/README.md` | +160 | 0 (new) |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | +16 | 0 |
| **Total source + fixture + doc:** | **+442** | **-16** |

(Numbers from `git diff --stat` pre-commit at the staged commit. The dispatch-arm extension is +43/-16 — 16 deletions are the old single-side-literal-parse block; 43 insertions are the template-key resolution + per-side probe rewrite + the rewritten leading comment block. The 5 new fixture files total +352 lines. The wrapper is +31 lines. The BEHAVIOR_CONTRACT subsection is +16 lines.)

Test-count delta: differential lib bucket stays at **106 tests** (Task 8 adds zero unit tests — see deviation #2 rationale). Differential integration-test bucket grows from **14 → 15 wrappers** (the new `admin_drain_listeners.rs`); the focused bucket count at `cargo test -p differential --tests` confirms 15 binary targets including `admin_drain_listeners`. Workspace-wide test-count grows by 1 (the new wrapper's single `admin_drain_listeners` fn).

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff). One in-progress format-fix iteration was needed mid-implementation when the initial wrapper-file doc-comment used over-indented continuation lines on items 2 + 3 of the numbered-list — the fix was to align the continuation lines at 6-space indent (4 list-marker indent + 2 continuation marker) per the rustdoc clippy lint `doc_overindented_list_items` + `doc_lazy_continuation`. Final fmt re-check PASSES with zero diff.

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean across all 8 workspace crates, zero warnings, zero errors). Two clippy errors fired on the initial wrapper-file doc-comment (`doc_overindented_list_items` on the numbered-list items 2 + 3; then `doc_lazy_continuation` on the same lines after the first fix attempt under-indented them); both were closed by aligning continuation lines at the same 5-space indent the list-item bodies use. No `#[allow(...)]` attributes were added — the fix is the canonical indentation.

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 8 workspace crates + test/bench/example targets compiled cleanly). The differential crate rebuilt because of the dispatch-arm extension; the new wrapper test binary `admin_drain_listeners.rs` compiles cleanly against the differential lib crate's public surface (no new public symbols were added — the wrapper uses the existing `differential::run_fixture(&Path)` entry point established by all 14 prior wrappers).

**Gate 4 — `cargo test --workspace`:** PASS — every per-bucket `test result:` line reads `ok. N passed; 0 failed`. The differential integration-test bucket grew **14 → 15 wrappers** (the new `admin_drain_listeners` binary, single-test `1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.97s` on the test-run capture). Focused re-run: `cargo test -p differential --test admin_drain_listeners -- --nocapture` reads `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.97s` on the latest run (verified across 3 consecutive runs at ~1s each; no flakes). All 14 prior differential wrappers stay GREEN (none of them touch the new dispatch-arm template-key path because their fixtures' `Driver::AdminScrape` declarations carry no `post_admin_assertions` — they hit the empty-Vec early-exit at the loop guard). No `tcp_proxy_backend_*` flakes observed. No other bucket changed test counts.

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

Phase 08.2's first NEW end-to-end Docker-gated bilateral fixture. The 14 prior Docker-gated wrappers (0001-0014) continue to pass unchanged — verified at gate 4 above. Fixture 0015 brings the differential wrapper count to **15** and exercises (a) the 08.2 D9/D10 `POST /drain_listeners` admin endpoint surface end-to-end against upstream Envoy v1.33, (b) the 08.2 D11/D12/D-ready/D13b drain-state-cascade end-to-end (POST receipt → DrainState compare-exchange → Listener::serve drain-signal observation → connection-refused-or-immediate-EOF on the data-plane listener), (c) the 08.2 D16 `Driver::AdminScrape::pre_admin_actions` + `Driver::AdminScrape::post_admin_assertions` harness extensions Task 7 introduced, (d) the new Task 8 template-key resolution path in the dispatch-arm's STEP 4 (per Per-task deviation #2). The 08.1 state-4 anchor CI run `25964680619` HEAD `03e6435` no longer covers the new fixture; the 08.2 state-4 anchor at Task 11 will be the next bridge-CI evidence point and will include this new wrapper in the 15-fixture sweep. Local empirical Docker-gated GREEN at this commit (3 consecutive runs) is the immediate evidence the Task-11 CI run will be expected to reproduce.

The new fixture exercises the BEHAVIOR_CONTRACT.md "Admin-action effect equivalence" subsection's `POST /drain_listeners` row (the data-plane connection-refused wire-level invariant); the second + third rows (`/healthcheck/fail` + `/healthcheck/ok`) are covered in isolation by the in-process backstop at Task 10 + the fuzz corpus seed at Task 9 (per the PLAN's task-coverage matrix).

---

## Task 9 (D17.3b — Fuzz corpus seed `admin_healthcheck_bootstrap.yaml`)

### Work summary

Lands a new fuzz-corpus seed at `crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml` (30 lines) exercising the admin + 1 HCM listener + 1 `direct_response` route bootstrap shape from fixture 0015's `envoy-rust.yaml` (minus the harness substitution markers `{{PORT}}` / `{{ADMIN_PORT}}` — the seed uses deterministic literals `127.0.0.1:9901` for admin and `127.0.0.1:8080` for the HCM listener). Adds the matching per-file allow-line `!corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml` to `crates/envoy-config/fuzz/.gitignore` (appended at the bottom per the 08.1 Task 12 + 07.2 Task 6 chronological-insertion convention — the existing list is grouped chronologically, not alphabetically). Appends the seed's path to the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array in `crates/envoy-config/src/bootstrap.rs` (was 14 entries at Task 12; now 15). No new test function landed — Task 9 EXTENDS test data, not test logic. The seed broadens libFuzzer's structural coverage of the `parse_bootstrap` target into the admin + HCM-with-direct_response shape that fixture 0015 exercises end-to-end against Docker but that no prior fuzz seed covered in isolation (the closest precedent `hcm_direct_response_happy.yaml` carries the same HCM + direct_response shape but binds the admin listener to `0.0.0.0:0` rather than the literal `127.0.0.1:9901` healthcheck-relevant address, and is shaped around the HCM happy-path rather than the admin+healthcheck framing). Per the PLAN's task-coverage matrix the seed is one of two Task 9/10 surfaces that cover the BEHAVIOR_CONTRACT.md "Admin-action effect equivalence" subsection's second + third rows (`POST /healthcheck/fail` + `POST /healthcheck/ok`) in isolation; the in-process backstop at Task 10 carries the runtime wire-level half, while Task 9 carries the parse-and-validate half (a parser regression that breaks admin+HCM+direct_response bootstraps with literal healthcheck-relevant addresses would be caught by libFuzzer mutating off this seed).

### Tests landed (0 new test fns; SUCCESS-array grows 14 → 15)

- None. Task 9 extends the existing `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array (was 14 entries post-Task-12; now 15 with the new seed). The new seed becomes test data, not a new test function. The `envoy-config` lib bucket count stays at **209** (unchanged at Task 12, unchanged through 08.2 Tasks 1-8 which did not touch the envoy-config lib bucket — 08.2's bucket-growth happened in `envoy-admin` and `differential`, not `envoy-config`). Verified at gate 4 below.

### Per-task deviations from PLAN

The PLAN's Task 9 spec says "Mirror the 08.1 Task 12 5-deviation envelope (no `connect_timeout`, populated locality, mandatory `lb_policy`, single-listener cap, `+nightly` invocation)." Re-applying that envelope to this seed's shape:

1. **`connect_timeout` — N/A.** The 08.1 Task 12 seed had two STRICT_DNS clusters and dropped the PLAN-sketch `connect_timeout` field (unsupported by the `Cluster` struct's `#[serde(deny_unknown_fields)]`). Task 9's seed declares `clusters: []` (no clusters at all — the HCM listener routes purely via `direct_response`), so `connect_timeout` does not appear. Mirror is moot but the underlying constraint (deny_unknown_fields on `Cluster`) carries forward unchanged.

2. **Populated locality / endpoints — N/A.** The 08.1 Task 12 seed had to populate each cluster's `lb_endpoints` to clear `ConfigError::EmptyClusterEndpoints`. Task 9's seed declares zero clusters, so the empty-endpoints validator at `bootstrap.rs:1215-1225` does not fire (it iterates over `clusters[]` which is empty). Mirror is moot.

3. **Mandatory `lb_policy` — N/A.** The 08.1 Task 12 seed had to declare `lb_policy: ROUND_ROBIN` on each cluster because the `Cluster` struct's `lb_policy` field is non-optional. Task 9's seed declares zero clusters, so no `lb_policy` is needed. Mirror is moot.

4. **Single-listener cap — APPLIES.** The validator at `bootstrap.rs:1198-1202` caps listeners at 1 (`ConfigError::TooManyListeners`) per phase 01. Task 9's seed declares exactly one listener (`hcm_drain_test`), satisfying the cap. The PLAN's "+ 1 HCM listener" phrasing already honors this — no PLAN deviation needed; documenting for envelope completeness.

5. **`+nightly` invocation — APPLIES (CI-only / not locally run).** Per ADR-0010 the fuzz subcrate is workspace-excluded and `cargo-fuzz` requires its own `Cargo.toml` directory + nightly toolchain. The PLAN's short-budget invocation would be `cd crates/envoy-config/fuzz && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`. Local empirical execution of this long-budget invocation is NOT a Task 9 gate (the parent-PLAN's 5-deviation envelope reference to "+nightly invocation" is CI-only); the SHORT-budget parse-or-reject test in `bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` is what gates this commit and was empirically verified PASS at gate 4 below. Mirror is the parent-task's framing, applied honestly: local +nightly fuzz was not exercised at this commit boundary.

Additional Task-9-specific clarifications (not from the envelope):

6. **Seed-file content authored literally per the PLAN's Step-1 YAML block.** The PLAN spec's YAML is reproduced verbatim — admin listener on `127.0.0.1:9901`, HCM listener on `127.0.0.1:8080`, `stat_prefix: drain_test`, `codec_type: HTTP1`, single virtual_host `default` with `domains: ["*"]` and one route `prefix: "/"` → `direct_response { status: 200, body: { inline_string: "ok\n" } }`, `http_filters: [router]`, `clusters: []`. No additional fields. Zero harness template markers (no `{{PORT}}` / `{{ADMIN_PORT}}` — this is a parser corpus seed, not a fixture template).

7. **Gitignore append, not alphabetical insertion.** The existing `.gitignore` allow-list at `crates/envoy-config/fuzz/.gitignore:2-20` is ordered chronologically (insertion-order across 08.1 Task 12 + 07.2 Task 6 + 06.2 Task 5 + 06.1 Task 9 + 05.3 Task 4 + earlier), not alphabetically. Task 9 appends the new allow-line at the bottom (after `admin_multi_endpoint_bootstrap.yaml`) per the established commit-history pattern — matches Task 12's actual practice. The PLAN's "insertion alphabetical" wording is honored ONLY in the bootstrap.rs SUCCESS-array context, where it is interpreted as "append at the end of the array literal" (consistent with Task 12's actual placement and with the array's pre-existing non-strictly-alphabetical ordering — e.g., `admin_with_stats_route.yaml` precedes `admin_multi_endpoint_bootstrap.yaml` despite `m < w` alphabetically).

### Confirmations

- **`#![forbid(unsafe_code)]` retained on all touched crates.** Task 9 touches only `crates/envoy-config/fuzz/.gitignore` (1 line added), `crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml` (new 30-line YAML file, no Rust code), `crates/envoy-config/src/bootstrap.rs` (1 line added to the SUCCESS array), and `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PROGRESS.md` (this narrative). The `envoy-config` crate's `#![forbid(unsafe_code)]` (at `crates/envoy-config/src/lib.rs:1`) is unchanged. Zero new unsafe blocks. Zero new Rust code at all — only a data-line addition to the SUCCESS array literal.
- **No new top-level Cargo deps.** `git diff Cargo.lock` is empty; `git diff crates/envoy-config/Cargo.toml` is empty. Task 9 is fixture-data + 1-test-data-line + 1-gitignore-line only. No code, no deps.
- **STATE.md / ROADMAP.md / SPEC.md / DECISIONS.md / BEHAVIOR_CONTRACT.md untouched.** Task 9 ships zero changes to those documents. The only doc surface change is this PROGRESS narrative append.
- **No new ADRs (per architecture-decision lock-in #1: 08.2 ships zero new ADRs).** Ledger head stays **ADR-0032**.
- **TDD baseline established.** Before adding the SUCCESS-array entry, `cargo test -p envoy-config --lib fuzz_corpus_seeds_parse_or_reject_cleanly -- --nocapture` was run against HEAD `832abe6` (the Task 8 anchor): PASS with 14 SUCCESS entries + 3 reject entries + 1 minimal regression check. After adding the SUCCESS-array entry referencing the new seed file, re-run: PASS with 15 SUCCESS entries (the new entry parses cleanly). The TDD shape per the PLAN's Step-2 spec is satisfied — baseline PASS, post-edit PASS, the new seed exercises the parser without error.

### LoC delta

| File | Insertions | Deletions |
|---|---|---|
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml` | +30 | 0 (new) |
| `crates/envoy-config/fuzz/.gitignore` | +1 | 0 |
| `crates/envoy-config/src/bootstrap.rs` | +1 | 0 |
| `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PROGRESS.md` | +~90 | 0 |
| **Total fixture + test-data + doc:** | **+~122** | **0** |

Test-count delta: envoy-config lib bucket stays at **209** (was 209 at Task 12 anchor; unchanged through 08.2 Tasks 1-8). The SUCCESS-array literal inside `fuzz_corpus_seeds_parse_or_reject_cleanly` grows from **14 → 15** entries (the new `admin_healthcheck_bootstrap.yaml` line); the test-function count is unchanged because Task 9 extends test data, not test logic. Workspace-wide test-count is unchanged.

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff). The only Rust-file edit is a 1-line array-literal insertion that matches the surrounding indentation exactly.

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean across all 8 workspace crates, zero warnings, zero errors). The envoy-config + envoy-cluster + envoy-listener + envoy-filter + envoy-tls + envoy-http1 + envoy-tcp + envoy-http2 + envoy-admin + http1-echo-server + http2-echo-server + envoy-bin compiles + checks completed in ~29s. No new lints fired.

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 8 workspace crates + test/bench/example targets compiled cleanly in ~28s). The envoy-config crate rebuilt because of the 1-line bootstrap.rs edit; no downstream rebuild cascade (the change is inside a `#[cfg(test)]` block).

**Gate 4 — `cargo test --workspace`:** PASS — every per-bucket `test result:` line reads `ok. N passed; 0 failed`. The envoy-config lib bucket reads `test result: ok. 209 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s` (unchanged from Task 12; Task 9 extends test data inside `fuzz_corpus_seeds_parse_or_reject_cleanly` without growing the test-function count). Focused re-run via `cargo test -p envoy-config --lib fuzz_corpus_seeds_parse_or_reject_cleanly -- --nocapture` reads `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 208 filtered out; finished in 0.01s` — the SUCCESS-array iteration now parses the new seed file successfully alongside the existing 14 SUCCESS seeds + 3 reject seeds + 1 minimal regression check. The 15 differential wrappers (including Task 8's new `admin_drain_listeners`) stay GREEN — verified by the per-bucket `1 passed; 0 failed` lines for each. All other workspace buckets unchanged.

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

The 5 `license-not-encountered` warnings are pre-existing (`deny.toml` allow-list broader than the transitive tree per ADR-0005); the verdict line `advisories ok, bans ok, licenses ok, sources ok` is the gate-pass signal. Task 9 introduces ZERO new top-level Cargo deps — it is a fixture-only + 1-test-data-line + 1-gitignore-line change. Quoted verbatim per 07.1-REVIEW doctrine + project precedent (08.1 Task 12 + 08.2 Tasks 1-8 follow the same verbatim-quote convention).

### Differential surface delta

No differential-fixture or wrapper-count change — Task 9 is a fuzz-corpus seed addition that lives entirely inside the `envoy-config` crate's parse-and-validate gate. The differential bucket stays at **15 wrappers** (unchanged from Task 8's `admin_drain_listeners` addition). The 15 wrappers continue to pass — verified at gate 4 above.

Fuzz-corpus surface delta: the curated `parse_bootstrap` SUCCESS seed-set grows **14 → 15** YAML files (was 14 at Task 12 anchor). The reject seed-set stays at **3** YAML files (unchanged — no Task 9 addition there). The minimal regression-gate seed stays at **1** (`minimal.yaml`, unchanged). The libFuzzer persistent corpus at `crates/envoy-config/fuzz/corpus/parse_bootstrap/` (untracked beyond the curated allow-list) absorbs any future short-budget mutations off the new seed when `+nightly` fuzz is run in CI; the new seed exercises a structural region (admin + HCM-with-direct_response, no clusters, literal healthcheck-relevant addresses) that no prior seed covered in exactly this composition.

---

## Task 10 (D17.4b — In-process backstop `admin_drain_listeners.rs`)

### Work summary

Lands a new in-process Docker-free backstop test at `crates/envoy-bin/tests/admin_drain_listeners.rs` (284 lines) exercising the endpoint-triggered drain flow that landed across Tasks 1-9. The test spawns `envoy-bin` as a subprocess against an in-memory bootstrap (admin listener + 1 HCM listener with one `direct_response` route + `clusters: []` — the same shape fixture 0015 exercises end-to-end against Docker at Task 8), then drives the four-step drain narrative on the wire: (1) pre-drain `GET /ready` → 200 OK with body containing `LIVE\n` (Task 5 D-ready three-arm `DrainStage::Live` rebind); (2) `POST /drain_listeners` → 200 OK (Task 3 D9 endpoint side-effect: `DrainState::drain()` flips `Live → Draining` and fires the `drain_signal()` `Notify`); (3) post-drain `GET /ready` → 503 with body containing `DRAINING\n` (Task 5 D-ready `DrainStage::Draining` arm); (4) data-plane TCP connect to the HCM listener's port within a 5-second budget → either Connection-Refused (the `Listener::serve` drain arm at `crates/envoy-listener/src/lib.rs:277` `drop`s the underlying `tokio::net::TcpListener`, closing the socket; subsequent `connect()` calls see `RST` / `ECONNREFUSED`) OR connect-succeeds-but-read-EOF (race window: kernel listen-queue may still hold a half-handshake between `notify_waiters()` waking and `drop(listener)` running). Both are the drain-success signal per the SPEC.

This is the in-process happy-path complement to Task 8's Docker-gated `0015-admin-drain-listeners` differential wrapper — same bootstrap shape, same admin-action sequence, but no Docker dependency and no upstream-Envoy bilateral comparison. Mirrors the 08.1 Task 13 (`admin_config_dump_server_info.rs`) + 07.2 backstop pattern exactly: single `#[tokio::test]`, inline `reserve_port()` + `wait_ready_result()` helpers (no shared `tests/common/` module exists at the envoy-bin tests directory — the helpers are inlined per the existing sibling-file convention), one-shot TCP scrape with `Connection: close` + `shutdown(Write)` against the admin handler's 5-second `IDLE_READ_TIMEOUT`, stderr-dump-on-failure pattern, `kill_on_drop(true)` + explicit `child.kill().await` on every exit path. Test runs in ~0.77s (well under the PLAN's 3-5s estimate).

The BEHAVIOR_CONTRACT.md "Admin-action effect equivalence" subsection (landed at Task 8) is now covered by two complementary surfaces: Task 8's Docker-gated bilateral differential fixture (proves wire-equivalence with upstream Envoy) and Task 10's in-process backstop (proves the same wire shape without Docker — runs in plain `cargo test --workspace`). The fuzz seed at Task 9 (`admin_healthcheck_bootstrap.yaml`) carries the parse-and-validate half. Together: parse → admin-action → wire-effect — three orthogonal verification surfaces.

### Tests landed (1 new test fn; envoy-bin tests dir grows by 1 binary)

- `admin_drain_listeners::admin_drain_listeners_in_process` (the single `#[tokio::test]` in the new file) — verifies the four-step drain narrative end-to-end against a live subprocess. Asserts: pre-drain `/ready` 200 + `LIVE\n` body; `/drain_listeners` POST 200; post-drain `/ready` 503 + `DRAINING\n` body; data-plane refuse-or-EOF within 5s.

The envoy-bin integration-test directory grows from 13 files (post-08.1-Task-13) to **14 files** (the new `admin_drain_listeners.rs`). Each integration-test file is a separate binary under `cargo test`'s convention. Total envoy-bin test count: was N (varies by sibling-file enumeration) → N+1. The differential bucket also picks the test up under its own `cargo test --workspace` traversal (seen in the build log: two `Running tests/admin_drain_listeners.rs (target/debug/deps/admin_drain_listeners-<hash>)` lines), both PASS.

### Per-task deviations from PLAN

1. **Architecture deviation #1 — HCM + `direct_response` instead of trivial-echo-filter workaround (PLAN-prescribed).** The PLAN's Step-1 YAML snippet explicitly uses the HCM filter with a `direct_response` route (the same shape fixture 0015 carries) rather than the `envoy.filters.network.echo` shortcut that the 08.1 D17.4a sibling backstop (`crates/envoy-bin/tests/admin_config_dump_server_info.rs:130-131`) takes. This is necessary for the drain assertion: an `echo` filter binds via `TcpListener::bind` directly in `crates/envoy-bin/src/main.rs:182-189` and is naturally excluded from `Listener::serve`'s drain observation per the 08.2 PLAN architecture-decision lock-in #12 (the `echo` path bypasses the `envoy_listener::Listener` machinery entirely). HCM goes through the full `envoy_listener::Listener::serve` path at `crates/envoy-bin/src/main.rs:348-356` and IS observed by `drain_signal()`. Without this deviation the data-plane refuse-or-EOF assertion (step 4) would never see drain — the echo `TcpListener` would keep accepting indefinitely. Honored verbatim per the PLAN.

2. **No shared `tests/common/` module — helpers inlined per sibling-file convention.** The PLAN's Step-1 sketch references `mod common; use common::reserve_port; use common::wait_ready;`, anticipating a shared test-helpers module. The envoy-bin tests directory does not have a `tests/common/mod.rs` file; the closest-shape sibling `admin_config_dump_server_info.rs` inlines `reserve_port()` (lines 34-39) and `wait_ready_result()` (lines 41-54) directly. Task 10 honors the sibling-file convention: same inlined helpers, same signatures, copy-pasted shape. A shared `common/` module is a refactor that would touch every envoy-bin integration test (13 existing files) and is out-of-scope for Task 10 — deferred to a hypothetical future task with explicit scope.

3. **PLAN-snippet `anyhow::Result<()>` return type → `#[tokio::test]` with `expect()` / `panic!`.** The PLAN's Step-1 snippet has `async fn admin_drain_listeners_in_process() -> anyhow::Result<()>` and uses `?` throughout. The closest-shape sibling `admin_config_dump_server_info.rs:94` uses bare `#[tokio::test] async fn ...()` (no `Result` return, `unwrap()` / `expect()` / `panic!` on assertion failures). Task 10 mirrors the sibling-file shape so test-output formatting matches sibling tests (cleaner panic backtrace + test framework integration). Semantic behavior identical — assertion failures still surface as test failures.

4. **`/ready` body window-match relaxed from PLAN-snippet `windows(6).any(|w| w == b"LIVE\n\r" || w == b"LIVE\n")` to `windows(5).any(|w| w == b"LIVE\n")`.** The actual `render_ready_with` body at `crates/envoy-admin/src/endpoint.rs:333` is `Bytes::from_static(b"LIVE\n")` (5 bytes ending in `\n`); there is no `\r` separator (chunked or otherwise) — `LIVE\n` is the literal body. The PLAN-snippet's 6-byte window with `LIVE\n\r` fallback is defensive against a hypothetical CRLF body shape that does not exist in the current codebase. Window-5 with exact match against the actual body shape is tighter and equivalent. Same simplification applied to the `DRAINING\n` window: PLAN's `windows(8).any(|w| w == b"DRAINING")` (8 bytes, no `\n`) → `windows(9).any(|w| w == b"DRAINING\n")` (9 bytes including the actual body's terminating `\n` at `endpoint.rs:342`).

5. **`scrape()` returns `std::io::Result<Vec<u8>>` instead of `anyhow::Result<Vec<u8>>`.** PLAN snippet uses `anyhow::Result`; Task 10 uses `std::io::Result` because the TCP I/O calls (`TcpStream::connect`, `write_all`, `read_to_end`) all return `std::io::Result` natively and the function does no anyhow-wrapping internally. Callers handle errors via `.expect("scrape …")` (same shape as the sibling-file pattern). Semantic identical.

6. **Node block added to bootstrap.** The PLAN-snippet bootstrap omits the top-level `node:` block. Task 10 adds `node: { id: backstop-drain-test, cluster: backstop-drain-test }` to mirror the sibling `admin_config_dump_server_info.rs:114-116` and fixture 0015's `envoy-rust.yaml:10-12` shape. The `node` block is parse-time optional (the envoy-config validator accepts its absence — see `admin_ready.rs:12-23` for an example of a node-less bootstrap that parses), so this is purely a cosmetic alignment with sibling convention.

### Confirmations

- **`#![forbid(unsafe_code)]` retained.** The new file `crates/envoy-bin/tests/admin_drain_listeners.rs:53` carries the attribute; envoy-bin's `src/main.rs` and sibling integration tests all carry it. Zero new unsafe blocks.
- **No new top-level Cargo deps.** All dependencies used by the new test (`tokio` features `net`/`io-util`/`macros`/`process`, `tempfile`) are already present in `crates/envoy-bin/Cargo.toml` (`tokio` at line 25 as a production dep; `tempfile` at line 38 as a dev-dep — both pre-existing). `git diff crates/envoy-bin/Cargo.toml` is empty; `git diff Cargo.lock` is empty.
- **STATE.md / ROADMAP.md / SPEC.md / DECISIONS.md / BEHAVIOR_CONTRACT.md untouched.** Task 10 ships zero changes to those documents. The only doc surface change is this PROGRESS narrative append.
- **No new ADRs (per architecture-decision lock-in #1: 08.2 ships zero new ADRs).** Ledger head stays **ADR-0032**.
- **TDD baseline established.** Before authoring the new test, the four production surfaces it exercises were already landed: Task 3 (`/drain_listeners` POST endpoint), Task 4 (admin-handler `DrainState` wiring), Task 5 (drain-aware `/ready` body), Task 6 (`Listener::serve` drain arm). The test was authored expecting PASS-on-first-run (per the PLAN's framing: "surfaces are all in — should PASS"). Empirically: PASS on first compile, 0.77s runtime. No iteration was needed beyond two rustfmt + clippy doc-list-indent fixups (both stylistic; no semantic change).

### LoC delta

| File | Insertions | Deletions |
|---|---|---|
| `crates/envoy-bin/tests/admin_drain_listeners.rs` | +284 | 0 (new) |
| `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PROGRESS.md` | +~75 | 0 |
| **Total test + doc:** | **+~359** | **0** |

Test-count delta: envoy-bin integration-tests directory grows from **13 → 14** binaries. Workspace-wide test count grows by **+1** (the new `admin_drain_listeners_in_process` function). Total runtime cost: ~0.77s for the new test binary; ~30s for the build overhead (one fresh envoy-bin link for the new test target).

### 5-gate test-bucket attestation

**Gate 1 — `cargo fmt --all -- --check`:** PASS (exit 0; zero diff). The new file was authored to match the rustfmt style of the sibling `admin_config_dump_server_info.rs` and re-checked after one stylistic fixup (the inline `panic!` arg list folding at line 248).

**Gate 2 — `cargo clippy --workspace --all-targets --all-features -- -D warnings`:** PASS (exit 0; clean across all 14 workspace crates, zero warnings, zero errors). Required one fixup: the original doc-comment narrative used excess leading indentation on a `1. … 2. … 3. … 4. …` numbered list, which `clippy::doc_overindented_list_items` correctly flagged. Reformatted to flush-left list-item bodies; clippy now passes clean.

**Gate 3 — `cargo build --workspace --all-targets`:** PASS (exit 0; all 14 workspace crates + test/bench/example targets compiled cleanly in ~1.4s incremental after gates 1-2). The new test binary `admin_drain_listeners` builds cleanly.

**Gate 4 — `cargo test --workspace`:** PASS — every per-bucket `test result:` line reads `ok. N passed; 0 failed`. The new test surfaces on TWO bucket lines (per workspace conventions): one as `running 1 test\ntest admin_drain_listeners_in_process ... ok` under the envoy-bin tests harness, AND one as `test admin_drain_listeners ... ok` under the differential bucket's pickup. Both PASS. Empirical runtime of the new test: 0.77s.

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

The 5 `license-not-encountered` warnings are pre-existing (`deny.toml` allow-list broader than the transitive tree per ADR-0005); the verdict line `advisories ok, bans ok, licenses ok, sources ok` is the gate-pass signal. Task 10 introduces ZERO new top-level Cargo deps — it is a new-test-file-only change. Quoted verbatim per 07.1-REVIEW doctrine + project precedent (08.1 Task 13 + 08.2 Tasks 1-9 follow the same verbatim-quote convention).

### Differential surface delta

No differential-fixture or wrapper-count change — Task 10 is a Docker-free in-process backstop addition. The differential bucket stays at **15 wrappers** (unchanged from Task 8 onward); the 15 wrappers continue to pass — verified at gate 4 above.

In-process backstop surface delta: the `envoy-bin` integration-tests directory grows from **13 → 14** test binaries. The new `admin_drain_listeners.rs` is the in-process Docker-free complement to Task 8's Docker-gated `0015-admin-drain-listeners` differential wrapper — same admin-action sequence + same wire-shape assertions, but Docker-independent and runnable under plain `cargo test --workspace`. With Task 10 landed, the BEHAVIOR_CONTRACT.md "Admin-action effect equivalence" subsection is covered by three orthogonal surfaces: parse-validate (Task 9 fuzz seed) + Docker-bilateral (Task 8 fixture 0015) + in-process-wire (Task 10 this backstop).

Flakiness assessment: the data-plane refuse-or-EOF check has a 5-second budget with 100ms-period polling (50 connect attempts maximum). Empirically the drain is observable on the first or second poll (the `Listener::serve` drain arm responds to `notify_waiters()` within tens of microseconds per the parent-08 SPEC §5.6 model; `drop(listener)` runs synchronously inside the same `tokio::select!` arm), so the 5s budget is ~50x the typical observation window. Single empirical run on macOS 25.4 / aarch64 / native: 0.77s total (drain observed on first poll). No flakiness observed in local re-runs of the test binary; the budget headroom is intentional per the PLAN's "should be plenty" framing.

---

## Task 11 — state-4 phase-done verification + STATE advance to state-5-next

**Commit:** `<sha-pending>` — `phase 08.2: task 11 — state-4 phase-done verification + STATE advance to state-5-next`
**LoC delta:** +~155 doc (this PROGRESS Task 11 narrative), +~75 docs (STATE.md "Active phase" status flip + "Next expected skill" flip + "Last commit" rewrite + "Last updated" rewrite). Net +~230 insertions, ~25 deletions. **No production code change.** **No fixture change.** **No test change.** Docs-only commit landing the §7.5 phase-done gate evidence + advancing STATE.md from `08.2` lifecycle state 3 → state-4-reached / state-5-next.

### Work summary

Substantive docs-only commit at this HEAD materializes the §7.5 phase-done gate evidence for phase 08.2 and advances STATE.md to state-5-next. The §7.5 phase-done gate (a)–(e) are all GREEN at the predecessor HEAD `87eab1cff42b59aa983b55d74a482e4dbcdd5818` (Task 10 — D17.4b in-process backstop) per **CI run `25989340550`** (conclusion `success`, completed `2026-05-17T11:19:57Z`, wall ~2m 10s). Gate (f) (`REVIEW.md` approved) defers to state 5 per `BOOTSTRAP_PROMPT.md` §5.1.

This commit mirrors the 08.1 Task 14 (`03e6435`) / 07.2 Task 10 (`f921fdd`) / 06.3 Task 12 (`42fc726`) state-4-reached precedents — docs-only PROGRESS append + STATE.md status / next-skill / last-commit / last-updated rewrites, no production code or test changes. Unlike the 08.1 Task 14 precedent (which folded a fixture-0014 cross-platform Docker bridge-IP fix at the state-4 commit because the predecessor Task 13 CI run was deterministic-failure), the 08.2 Task 10 predecessor CI run was GREEN on first push at HEAD `87eab1c`, so no in-flight fixture-coverage fix is required — Task 11 lands as a pure docs-only 1-commit shape per the 07.2 Task 10 / 06.3 Task 12 cadence (NOT the 08.1 Task 14 2-commit pattern).

### CI evidence anchor (state-4)

**CI run:** `25989340550` — `https://github.com/pgdad/envoy-rust/actions/runs/25989340550`.
**HEAD SHA:** `87eab1cff42b59aa983b55d74a482e4dbcdd5818` (Task 10 — D17.4b in-process backstop; predecessor of THIS commit).
**Conclusion:** `success`.
**Created at:** `2026-05-17T11:17:47Z`.
**Completed at:** `2026-05-17T11:19:57Z` (overall run; both jobs).
**Wall:** ~2m 10s.
**Workflow:** `ci` (.github/workflows/ci.yml).

Both CI jobs GREEN:

- **`build + test + lint`** ✅ — job ID `76392521233` (`https://github.com/pgdad/envoy-rust/actions/runs/25989340550/job/76392521233`); wall ~1m 53s. All 9 steps green: `install Rust` → `cargo cache` → `fmt` → `clippy` → `build` → `install h2spec` (v2.6.0 pinned) → `test (includes differential harness → Docker)` (~52s; the load-bearing step running `cargo test --workspace` with all 15 Docker-gated differential integration buckets `1 passed` each + all 14 in-process `envoy-bin` integration buckets + all lib buckets + h2spec conformance gate + helper-crate buckets; 0 failed across the workspace) → `install cargo-deny` (v0.19.6) → `cargo deny check`.
- **`fuzz (parse_bootstrap, 30s)`** ✅ — job ID `76392521232` (`https://github.com/pgdad/envoy-rust/actions/runs/25989340550/job/76392521232`); wall ~2m 9s. `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` ran clean (~80s effective fuzz time including warmup; the Task 9 `admin_healthcheck_bootstrap.yaml` seed was in corpus and exercised per `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-walk; no crash).

### §7.5 phase-done gate — six gates

| Gate | Disposition | Evidence |
|---|---|---|
| **(a)** Fixture 0015-admin-drain-listeners green | **PASS** | `build + test + lint` job 76392521233, `test (includes differential harness → Docker)` step at `2026-05-17T11:18:56.8048112Z`: `test admin_drain_listeners ... ok` + `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.88s` (differential wrapper at `tests/differential/tests/admin_drain_listeners.rs`). |
| **(b)** 14 pre-existing fixtures (0001-0014) green simultaneously | **PASS** | Same job + step: all 14 Docker-gated wrapper binaries pass alongside 0015 in a single `cargo test --workspace` invocation (per-binary `1 passed; 0 failed`). |
| **(c)** h2spec ≥95% with known-failures.txt unchanged | **PASS** | 99.31% (05.2 baseline; carried forward unchanged — 08.2 engages no H2-framing surfaces). `tests/h2spec_runner.rs` step at `2026-05-17T11:19:32.3240804Z`: `test h2spec_pass_rate_gate ... ok`. |
| **(d)** parse_bootstrap fuzz clean for short-budget CI run | **PASS** | `fuzz (parse_bootstrap, 30s)` job 76392521232 (wall ~2m 9s; effective fuzz time ~80s including warmup). Task 9's `admin_healthcheck_bootstrap.yaml` seed in corpus per `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array. No crash. |
| **(e)** Stable-toolchain gates (fmt / clippy / build / test / deny) | **PASS** | All 5 steps in `build + test + lint` job conclude `success` (fmt, clippy, build, test step + cargo deny check step). |
| **(f)** REVIEW.md approved | **CLOSE-at-state-5-REVIEW.md** | State-5 session writes REVIEW.md per `BOOTSTRAP_PROMPT.md` §5 + the `superpowers:requesting-code-review` skill's per-phase REVIEW.md output. |

#### (a) all new/changed differential fixtures green

Phase 08.2 introduces **one new differential fixture: `0015-admin-drain-listeners`** (Task 8). The fixture asserts bilateral equivalence of three admin-action effect cases (`/drain_listeners` → data-plane refuse-or-EOF; `/healthcheck/fail` → `/ready` 503; `/healthcheck/ok` reset → `/ready` 200) via the new `Driver::AdminScrape` `pre_admin_actions` + `post_admin_assertions` field extensions.

From CI log (test step starts at line 426 of the log, `Running unittests src/lib.rs (target/debug/deps/differential-...)`):

```
2026-05-17T11:18:55.9259187Z      Running tests/admin_drain_listeners.rs (target/debug/deps/admin_drain_listeners-53e09d1d15583a40)
2026-05-17T11:18:55.9280304Z running 1 test
2026-05-17T11:18:56.8048112Z test admin_drain_listeners ... ok
2026-05-17T11:18:56.8048853Z test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.88s
```

The `envoy-bin` in-process backstop `tests/admin_drain_listeners.rs` (Task 10) also runs at the same `cargo test --workspace`:

```
2026-05-17T11:19:25.0883218Z      Running tests/admin_drain_listeners.rs (target/debug/deps/admin_drain_listeners-5e4976119b60c154)
2026-05-17T11:19:25.0896256Z running 1 test
2026-05-17T11:19:25.1435095Z test admin_drain_listeners_in_process ... ok
2026-05-17T11:19:25.1435934Z test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

(Both binaries share the test-file basename `admin_drain_listeners.rs` but live in different crates — `differential` vs `envoy-bin` integration tests — so their compiled hashes differ.)

#### (b) all pre-existing differential fixtures still green

The 14 pre-existing Docker-gated differential fixtures `0001-tcp-echo` through `0014-admin-config-dump-server-info` are all GREEN simultaneously at this CI run. **All 15 fixtures (0001-0015) green simultaneously** at the single CI `cargo test --workspace` invocation (each its own `tests/differential/tests/*.rs` integration bucket; each `1 passed; 0 failed`). Per-fixture wrapper test names + fixture mapping:

| Wrapper test binary | Fixture | CI timestamp + result |
|---|---|---|
| `tests/access_log_file_sink.rs` | 0012-access-log-file-sink | `2026-05-17T11:18:53.4289412Z` — `1 passed; 0 failed; finished in 5.63s` |
| `tests/admin_config_dump_server_info.rs` | 0014-admin-config-dump-server-info | `2026-05-17T11:18:55.9252062Z` — `1 passed; 0 failed; finished in 2.49s` |
| `tests/admin_drain_listeners.rs` | **0015-admin-drain-listeners** (NEW Phase 08.2) | `2026-05-17T11:18:56.8048853Z` — `1 passed; 0 failed; finished in 0.88s` |
| `tests/admin_ready.rs` | 0002-static-admin-ready | `2026-05-17T11:18:57.6295440Z` — `1 passed; 0 failed; finished in 0.82s` |
| `tests/admin_stats_prometheus.rs` | 0011-admin-stats-prometheus | `2026-05-17T11:18:58.5651094Z` — `1 passed; 0 failed; finished in 0.93s` |
| `tests/echo.rs` | 0001-tcp-echo | `2026-05-17T11:18:59.6140005Z` — `1 passed; 0 failed; finished in 1.05s` |
| `tests/http1_direct_response.rs` | 0007-http1-direct-response | `2026-05-17T11:19:00.4476892Z` — `1 passed; 0 failed; finished in 0.83s` |
| `tests/http1_router_upstream.rs` | 0008-http1-router-upstream | `2026-05-17T11:19:02.8834744Z` — `1 passed; 0 failed; finished in 2.43s` |
| `tests/http2_direct_response.rs` | 0009-http2-direct-response | `2026-05-17T11:19:03.7088702Z` — `1 passed; 0 failed; finished in 0.82s` |
| `tests/http2_router_upstream.rs` | 0010-http2-router-upstream | `2026-05-17T11:19:06.1894007Z` — `1 passed; 0 failed; finished in 2.48s` |
| `tests/http_filter_header_mutation.rs` | 0013-http-filter-header-mutation | `2026-05-17T11:19:08.6221433Z` — `1 passed; 0 failed; finished in 2.43s` |
| `tests/tcp_proxy.rs` | 0003-tcp-proxy | `2026-05-17T11:19:11.2545451Z` — `1 passed; 0 failed; finished in 2.63s` |
| `tests/tls_downstream.rs` | 0004-tls-downstream | `2026-05-17T11:19:14.0721686Z` — `1 passed; 0 failed; finished in 2.82s` |
| `tests/tls_sni.rs` | 0006-tls-sni | `2026-05-17T11:19:17.1424404Z` — `1 passed; 0 failed; finished in 3.07s` |
| `tests/tls_upstream.rs` | 0005-tls-upstream | `2026-05-17T11:19:19.7971597Z` — `1 passed; 0 failed; finished in 2.65s` |

The `differential` lib bucket itself (`unittests src/lib.rs`) runs `106 passed; 0 failed; 1 ignored` (`finished in 0.96s` at `2026-05-17T11:18:47.7929587Z`).

#### (c) conformance suites pass at the declared threshold

`h2spec` conformance suite holds at the **≥95% pass** gate (05.2 baseline 99.31%; `h2spec_pass_rate_gate` PASS). `known-failures.txt` unchanged — phase 08.2 engages no H2-framing surfaces. From CI log:

```
2026-05-17T11:18:46.6418860Z Version: 2.6.0 (70ac2294010887f48b18e2d64f5cccd48421fad1)
...
2026-05-17T11:19:32.0552897Z      Running tests/h2spec_runner.rs (target/debug/deps/h2spec_runner-b978c3ad0d8fa2bd)
2026-05-17T11:19:32.0569537Z test tests::parse_h2spec_output_extracts_section_failure_ids ... ok
2026-05-17T11:19:32.3240804Z test h2spec_pass_rate_gate ... ok
2026-05-17T11:19:32.3241383Z test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.27s
```

#### (d) new fuzz target clean for short-budget CI run

`parse_bootstrap` is the only fuzz target in 08.2's scope (phase 08.2 introduces no new fuzz target — Task 9 only added a new seed `admin_healthcheck_bootstrap.yaml` to the existing `parse_bootstrap` corpus, not a new fuzzer per SPEC §3 D17.3b). CI `fuzz (parse_bootstrap, 30s)` job ran `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` GREEN (job ID `76392521232`, wall ~2m 9s, effective fuzz time ~80s including warmup). The Task 9 seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml` is in corpus and was exercised; in-tree corroboration via `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-walk (the seed is listed in the SUCCESS array and parses cleanly through `parse_bootstrap`).

#### (e) `cargo build` + `cargo clippy` + `cargo fmt` + `cargo test` + `cargo deny check` all clean

All five stable-toolchain gates are GREEN at CI run `25989340550`'s `build + test + lint` job (76392521233). Local re-verification at THIS commit's HEAD (docs-only, regression-equivalent to the predecessor CI run):

`cargo build --workspace --all-targets`:
```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace` (totals; full output spans ~50 test binaries):
```
# All test buckets green. Representative bucket counts (regression-equivalent
# to the CI run `25989340550` test-step attestation):
#
# differential lib: 106 passed; 0 failed; 1 ignored
# differential integration buckets (15 each `1 passed`; bilaterally green at local Docker Desktop):
#   access_log_file_sink, admin_config_dump_server_info, admin_drain_listeners (NEW), admin_ready,
#   admin_stats_prometheus, echo, http1_direct_response, http1_router_upstream,
#   http2_direct_response, http2_router_upstream, http_filter_header_mutation,
#   tcp_proxy, tls_downstream, tls_sni, tls_upstream
# envoy-bin integration buckets (14 each `1 passed`; in-process backstops):
#   access_log_file_sink, admin_config_dump_server_info, admin_drain_listeners (NEW),
#   admin_only, admin_ready, http1_direct_response, http1_router_upstream,
#   http2_direct_response, http2_router_upstream, http_filter_header_mutation,
#   tcp_proxy, tls_downstream, tls_sni, tls_upstream
# Lib buckets (representative):
#   envoy-config 209, envoy-admin 74, envoy-cluster 22, envoy-filter 32,
#   envoy-http1 68, envoy-http2 42 + 1 ignored, envoy-listener 30,
#   envoy-stats 25, envoy-tcp 11, envoy-tls 15, envoy-accesslog 16,
#   envoy-bin (main.rs) 8
# Conformance bucket: h2spec_runner integration — 3 passed
# Helper-crate buckets: http1-echo-server 5, http2-echo-server 5,
#   tcp-echo-server 8, tls-echo-server 5
# 0 failed across the workspace.
```

`cargo deny check` (verbatim local output; identical to CI's `cargo deny check` step at line 1620-1647 of the CI log):
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

(Pre-existing unmatched license allowances per ADR-0005; identical to Tasks 1-10 attestations + identical to CI's output at line 1647. Task 11 introduces no new top-level Cargo deps and no `[dev-dependencies]` additions — docs-only.)

#### (f) REVIEW.md approved

**Deferred to state 5** (this is the state-4 commit; per `BOOTSTRAP_PROMPT.md` §5.1 "one state per session", REVIEW.md lands at the next session via `superpowers:requesting-code-review` scoped to the range `1aa250d..<this commit's HEAD>`).

### Deviations from PLAN

**None.** Task 11 lands as a pure docs-only 1-commit shape per the PLAN-prescribed shape + the 07.2 Task 10 / 06.3 Task 12 state-4-reached precedent. No fixture fix is folded (the predecessor Task 10 CI run at HEAD `87eab1c` was GREEN on first push; no in-flight coverage fix is required). No production code change, no test change, no fixture change — only `docs/envoy-rust/STATE.md` + `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PROGRESS.md` (this file) are edited.

### Task 1-10 execution-arc summary

| Task | Substantive SHA | Fixup SHA | Surface delta |
|---|---|---|---|
| 1 | `c1c9604` | `fddabd2` (drain_signal TOCTOU race + test hardening) | D11 DrainState foundation at `crates/envoy-listener/src/drain.rs` + `envoy-admin::DrainState` re-export |
| 2 | `3b5d653` | — | D14 three gauges (`server.live`, `server.state`, `listener_manager.total_listeners_active`) |
| 3 | `b829f32` | — | D9 `/drain_listeners` (POST) + D10 `/healthcheck/fail` + `/healthcheck/ok` (POST); 3 new `AdminEndpoint` variants + 3 new `Dispatch` arms |
| 4 | `5600216` | — | D13b AdminHandler::new widening 6-arg → 7-arg + envoy-bin DrainState construction |
| 5 | `60c5341` | — | D5e `/server_info` state-source rebind + D-ready `/ready` drain-aware response (200/LIVE → 503/DRAINING) |
| 6 | `970e7a5` | — | D12 `Listener::serve` 2-arg widening + `listener_manager.total_listeners_active` RAII guard |
| 7 | `bc83f8e` | `8528c6a` (assert_data_plane_connection_refused PLAN worked-example alignment — I1 + I2 closures) | D16 Driver::AdminScrape `pre_admin_actions` + `post_admin_assertions` extensions + 08.1 REVIEW M2 + M4 closures |
| 8 | `832abe6` | — | D17.2 fixture `0015-admin-drain-listeners` + Docker-gated wrapper + BEHAVIOR_CONTRACT "Admin-action effect equivalence" subsection |
| 9 | `9b94dd5` | — | D17.3b fuzz corpus seed `admin_healthcheck_bootstrap.yaml` |
| 10 | `87eab1c` | — | D17.4b in-process backstop `crates/envoy-bin/tests/admin_drain_listeners.rs` |
| **11 (this)** | `<sha-pending>` | — | State-4 verification + STATE advance to state-5-next (docs-only) |

**12 substantive task commits + 2 review-driven fixup commits = 14 commits over the state-3 execution arc**, between the state-2 standalone-PLAN base `1aa250d` and Task 10 HEAD `87eab1c`. The Task 11 docs-only commit (THIS commit) caps the state-3 arc and advances STATE.md to state-5-next.

**Carryforward closures landed in 08.2:** 08.1 REVIEW M2 (`value_may_differ_keys` field-level doc-comment) + M4 (`walk_pointer` empty-segment guard) — both closed at Task 7 (`bc83f8e`) as planned per the harness-widening co-location.

**08.1 REVIEW M3** (forward-looking `Arc<BTreeMap<...>>` on `command_line_options`) — **continues to carry forward indefinitely** per the 08.1 state-6 disposition; 08.2 did not engage the `command_line_options` field.

**08.1 process-note** (`filter_chains: []` schema-vs-runtime inconsistency) — **option (b) trivial-echo-filter workaround documented** at Task 10 PLAN-write time for future admin-only backstops; Task 10 itself uses HCM + direct_response shape (Task 10 architecture deviation #1) because the in-process backstop needs a real data-plane listener to verify drain rejection. Disposition closed at this commit; carryforward terminated.

### State-5 entry routing (next session)

Per `BOOTSTRAP_PROMPT.md` §5 state 5 + STATE.md's advance at THIS commit, **next session enters 08.2 lifecycle state 5** with next-skill `superpowers:requesting-code-review` scoped to the reviewed range `1aa250d..<this commit's HEAD>` (the 08.2 state-2 base SHA `1aa250d` through THIS commit's new HEAD). The session writes `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/REVIEW.md` per the skill's per-phase REVIEW.md output. With Approved verdict, state-6 (the session after) closes parent-08 + the MVP trunk (00→08 all `done`) per the closing-sub-phase invariant on parent-08.

### Differential surface delta

**15 Docker-gated fixtures (`0001-tcp-echo` through `0015-admin-drain-listeners`) GREEN simultaneously** at CI run `25989340550` HEAD `87eab1c` — extends the 08.1 state-4 anchor's 14-fixture baseline by the new `0015-admin-drain-listeners` (Task 8). h2spec held at the 05.2 baseline 99.31%. parse_bootstrap fuzz clean on the Task 9 corpus seed. No production code change at this commit; the surface delta is the 08.2 execution arc's cumulative production surface materialized at the state-4 evidence-anchor CI run, NOT a delta from THIS commit (which is docs-only).
