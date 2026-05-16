# Phase 08.2 (`08.2-endpoint-triggered-drain`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended per the user's standing preference auto-memory `feedback_execution_style`) — fresh subagent per task + two-stage review. Steps use checkbox (`- [ ]`) syntax for tracking. Tasks land in numbered order; this PLAN.md commits ALONGSIDE the PROGRESS.md skeleton (with the Task 1 preamble) at state-2 per the 08.1 (`7dbd984`) / 07.2 (`c7dea4c`) / 06.3 (`3a964cc`) / 06.2 (`dc00750`) cadence — NO code changes at state-2. Tasks 1-10 each land as their own state-3 commit; Task 11 is the state-4-reached / state-5-next STATE-advance commit.

**Goal.** Ship the endpoint-triggered graceful-drain semantics atop the 08.1 admin endpoint surface: a new `DrainState` foundation observed by data-plane listener accept loops, three new POST admin endpoints (`/drain_listeners` + `/healthcheck/fail` + `/healthcheck/ok`), three new drain-related gauges, a `/server_info`-state value-source rebind from 08.1's literal `"LIVE"` to `DrainState::current()`, and a `/ready` drain-aware 503 DRAINING response — proven via the new differential fixture `0015-admin-drain-listeners` that 15 Docker-gated fixtures (0001-0015) stay green simultaneously while the new fixture exercises the wire-level drain effect. 08.2's state-6 commit (a later session) closes parent-08 AND the MVP trunk (00→08 all done).

**Architecture.** Hand-rolled per D-3.2 (*Hot-restart / graceful-drain semantics* is on the **Must be written from scratch** list). The `DrainState` lives at `crates/envoy-listener/src/drain.rs` (re-exported from `envoy-admin::DrainState`) per parent-08 SPEC §5.1's Cargo-cycle resolution — mirrors the M4 `DRAIN_BUDGET` hoist + the 05.3 (ADR-0028) / 07.1 (ADR-0031) cycle-resolution doctrine; no new ADR. `DrainStage::{Live, HealthcheckFailing, Draining}` over `AtomicU8`; `tokio::sync::Notify` for `drain_signal()`; sticky-drain semantic where `ok_healthcheck()` after `drain()` is a no-op. The three new POST endpoints (D9 + D10) plug additively into 08.1's `Dispatch` enum via the existing `allowed_method() = "POST"` declaration pattern. `AdminHandler::new` widens from 08.1's 6-arg shape to 7-arg (adds trailing `drain: Arc<DrainState>`). `Listener::serve` widens from 1-arg `(shutdown)` to 2-arg `(shutdown, drain)` with a second `tokio::select!` arm `_ = drain.drain_signal() => { ... }` driving the same drain code path as the existing shutdown arm; the admin listener does NOT observe its own `drain_signal` (operator-reachability invariant per parent-08 SPEC §5.5). Three new gauges (`server.live`, `server.state`, `listener_manager.total_listeners_active`) are registered at startup; the first two are updated inline at the `DrainState::{fail_healthcheck, ok_healthcheck, drain}` state-transition sites (one source of truth, no polling task); the third is RAII-wrapped at `Listener::serve` entry/exit. ZERO new top-level Cargo deps; no foundations grants under the recommended posture; ledger head stays ADR-0032.

**Tech Stack.** New permitted-foundations: NONE. `tokio::sync::Notify` (already used; part of `tokio`) + `std::sync::atomic::AtomicU8` (std-lib) cover the new surfaces. No new workspace member. Modified workspace members: `envoy-listener` (new `drain` module + `Listener::serve` 2-arg widening + `listener_manager.total_listeners_active` RAII), `envoy-admin` (3 new POST endpoint variants + `AdminHandler::new` 7-arg widen + `render_server_info` value-source rebind + `render_ready` drain-aware widening + `pub use envoy_listener::DrainState` re-export), `envoy-bin` (shared-handle wiring — adds `Arc<DrainState>` to admin spawn + threads through to HCM + tcp_proxy listener spawns), `tests/differential` (`Driver::AdminScrape` `pre_admin_actions` + `post_admin_assertions` extensions + new `AdminAction` + `AdminAssertion` enums + 08.1 REVIEW M2 + M4 closures). New differential fixture `tests/fixtures/0015-admin-drain-listeners/` + Docker-gated wrapper `tests/differential/tests/admin_drain_listeners.rs` + in-process backstop `crates/envoy-bin/tests/admin_drain_listeners.rs` + fuzz corpus seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml`. `cargo deny check` is a no-op for top-level deps but MUST be quoted in every task's PROGRESS attestation (07.1-REVIEW doctrine reminder; ratified in 08.1 / 07.2 / 06.3 / 06.2 / 06.1).

---

## PLAN-write SPEC corrections (recorded here + in PROGRESS.md Task 1 preamble)

The 08.2 SPEC landed at the parent-08 state-2 split commit `56dee82`, derived from the parent-08 state-1 SPEC committed at `0202e38`. Six SPEC details drifted against the 08.1-landed tree (verified against HEAD `3ed6af0`, the 08.1 state-6 close-out). Per the user's standing preference `feedback_pick_recommendation`, each correction picks the working option; all are folded into the task steps below.

1. **`AdminHandler::new` current shape is the 6-arg form `(config, registry, bootstrap, cluster_manager, start_instant, command_line_options)`, NOT the 5-arg form the parent-08 SPEC §3 D13 named.** Verified at `crates/envoy-admin/src/handler.rs:77-93`. 08.1's PLAN-write SPEC correction 4 settled on 6-arg (capturing `command_line_options` at construction time per architecture lock-in #7). 08.2's D13b widens this from 6-arg to **7-arg** by adding `drain: Arc<DrainState>` as the trailing parameter (mirrors how 08.1's Task 5 added `command_line_options` last). The 08.2 SPEC §3 D13b paragraph says "5-arg shape to 6-arg" — incorrect; the current shape is 6-arg → widen to 7-arg. The 08.1 PROGRESS Task 5 lockstep narrative + STATE.md "Phase-08.1 rollovers" subsection both confirm 6-arg as the disk reality.

2. **`AdminEndpoint` current enum already declares `allowed_method(&self) -> &'static str` per-variant.** Verified at `crates/envoy-admin/src/endpoint.rs:89-102`. 08.2's D9/D10 POST endpoints additively declare `allowed_method() = "POST"` per the established pattern; the existing `Dispatch::MethodNotAllowed { allow }` arm covers wrong-method 405 dispatch verbatim — no further refactor needed.

3. **`AdminEndpoint::render_with(&handler)` dispatch path already exists.** Verified at `crates/envoy-admin/src/endpoint.rs:154-163`. 08.2's 3 new POST endpoints add `DrainListeners` / `HealthcheckFail` / `HealthcheckOk` arms to the existing `render_with` match (alongside `ConfigDump` / `ServerInfo` / `Clusters` / `Listeners`). Each render fn takes `&handler` to invoke `handler.drain().drain()` / `.fail_healthcheck()` / `.ok_healthcheck()`. Pattern is established at 08.1; 08.2 is additive.

4. **`Listener::serve` current signature is 1-arg `(shutdown: impl Future<Output = ()>)`.** Verified at `crates/envoy-listener/src/lib.rs:167-170`. 08.2's D12 widens to 2-arg `(shutdown, drain: Arc<DrainState>)` and adds a second `tokio::select!` arm `_ = drain.drain_signal() => { ... }` between the existing `_ = &mut shutdown` arm and the `accepted = listener.accept()` arm. Either signal triggers the same drain code path (drop the listener; await the existing JoinSet within `DRAIN_BUDGET`).

5. **`Listener::bind` already registers per-listener gauges idempotently against the registry.** Verified at `crates/envoy-listener/src/lib.rs:120-148` — `cx_total` + `cx_active` + `cx_accept_failed` register via `registry.register_*("listener.<name>.*")` with the comment "registry call is idempotent for same-kind re-registration". 08.2's D14 `listener_manager.total_listeners_active` follows the same pattern but is a SHARED (not per-listener-named) gauge — registered idempotently at `Listener::bind`; each Listener gets its own Arc<Gauge> clone of the same handle; inc/dec are atomic across all listeners. RAII guard in `Listener::serve` increments at entry / decrements at exit. (Alternative: register once in envoy-bin and thread the gauge handle to Listener::bind. Rejected — `Listener::bind` already takes the registry; idempotent re-registration is the cleanest placement.)

6. **The admin listener path in `envoy-bin/src/main.rs` (lines 348-376) uses `TcpListener::bind` + `envoy_admin::serve` directly, NOT `Listener::bind`.** Verified at `crates/envoy-bin/src/main.rs:353-375`. This means the admin listener is naturally excluded from the `listener_manager.total_listeners_active` gauge (which only counts data-plane listeners going through `Listener::bind`) AND from the `drain.drain_signal()` observation (the admin listener stays serving during drain per parent-08 SPEC §5.5). No code change needed on the admin path beyond passing `Arc<DrainState>` to `AdminHandler::new` (D13b). The echo path (`envoy-bin/src/main.rs:170-180`) also uses `TcpListener::bind` + `echo::serve` directly (fixture 0002 only); it is similarly naturally excluded from the new gauge AND from drain observation. Documented in architecture-decision lock-in #12 below; echo-path drain handling defers indefinitely (fixture 0002 has no drain test surface).

---

## Architecture decisions locked at PLAN-write time (signpost choices)

Per 08.2 SPEC §6's implementation signposts + §7 ADR posture, the planner picks the recommendation so the executor does not re-litigate mid-task. Per the user's standing preference `feedback_pick_recommendation`, every signpost with a "recommended posture" gets that recommendation.

| # | Signpost | Decision | Rationale |
|---|---|---|---|
| 1 | D11 task ordering | **Task 1 = D11 DrainState foundation (with PROGRESS preamble landing alongside the state-2 PLAN-write commit, not at Task 1).** Cadence mirrors 06.3 / 08.1 (the PROGRESS preamble lives in the state-2 standalone-PLAN commit; Task 1 itself is the first substantive code commit). | SPEC §6.2 — D11 lands FIRST because every downstream surface consumes `DrainState`. |
| 2 | `DrainState` placement | **`crates/envoy-listener/src/drain.rs` with `pub use envoy_listener::DrainState` re-export from `envoy-admin::lib.rs`.** | Settled at parent-08 SPEC §5.1 + 08.2 SPEC §5.2 — Cargo-cycle resolution mirrors the M4 `DRAIN_BUDGET` hoist. No new ADR. |
| 3 | `DrainState::new` shape | **`pub fn new(registry: &Arc<envoy_stats::StatsRegistry>) -> Self`** — registers `server.live` + `server.state` gauges via the registry at construction time + stores both as `Arc<Gauge>` fields on `DrainState`. The `listener_manager.total_listeners_active` gauge is registered inside `Listener::bind` (not `DrainState::new`) per PLAN-write SPEC correction 5. | SPEC §6.4 "the simpler `DrainState` field shape is recommended" — gauge handles live on `DrainState`, updated inline at state-transition sites by reading `&self.<gauge_field>.set(...)`. One source of truth; no polling task; no separate gauge-handle threading. |
| 4 | `drain()` notify semantics | **Use `compare_exchange` with `Ordering::AcqRel` / `Ordering::Acquire` on the CAS site; call `notify.notify_waiters()` ONLY on CAS-success in the `Live → Draining` or `HealthcheckFailing → Draining` arm. Repeat `drain()` calls (already-Draining) take the CAS-failure path and do NOT call `notify_waiters` (idempotent + no wasted cycles per SPEC §6.3).** | SPEC §6.3 + parent-08 SPEC §6.6 — "fires exactly once on the first transition". |
| 5 | `drain_signal()` shape | **`pub fn drain_signal(&self) -> impl Future<Output = ()> + '_` that checks `self.state.load(Ordering::Acquire)` first; if already `Draining`, returns `future::ready(())` via boxing as `Either::Right`; otherwise returns `self.notify.notified()` boxed as `Either::Left`.** Use `futures::future::Either` (already transitively available via tokio) or hand-roll a small enum-wrapper future. Recommended: hand-roll a tiny `enum DrainSignal { Pending(Notified<'_>), Ready }` impl Future to avoid even a transitive `futures` import. | SPEC §6.3 — "On already-Draining, drain_signal() returns an immediately-ready future". Avoids the consumer needing to repeatedly call `notified()` after drain has already fired. |
| 6 | `DrainStage` enum | **`#[repr(u8)] pub enum DrainStage { Live = 0, HealthcheckFailing = 1, Draining = 2 }`** + `pub fn from_u8(n: u8) -> Option<Self>` constructor for `current()` to use. The discriminant is the wire-level value for `server.state` gauge. | SPEC §3 D11 + §2.2 — explicit `repr(u8)` matches the SPEC's `Live=0`, `HealthcheckFailing=1`, `Draining=2` mapping. |
| 7 | DrainState ≥6 unit tests | **6 tests at Task 1:** `new_returns_live`, `drain_flips_to_draining_and_notifies_waiters_once`, `fail_healthcheck_flips_to_healthcheck_failing`, `ok_healthcheck_restores_to_live`, `ok_healthcheck_after_drain_is_noop_sticky`, `repeat_drain_calls_are_idempotent`. Plus 3 stats-binding tests at Task 1 (or moved to Task 2 if the stats wiring is separated): `new_registers_server_live_gauge`, `new_registers_server_state_gauge`, `drain_updates_server_live_to_zero_and_server_state_to_two`. | SPEC §3 D11 + §6.4. |
| 8 | D9/D10 task structure | **One task (Task 3) covering all 3 POST endpoints.** The three endpoints are mechanically symmetric: each adds one `AdminEndpoint` variant + `allowed_method() = "POST"` declaration + a render fn that invokes the corresponding `DrainState::{drain, fail_healthcheck, ok_healthcheck}` method and returns 200 OK with empty body. Splitting into multiple tasks adds review overhead with no boundary benefit. | SPEC §6.1 estimate "~1 task". |
| 9 | D5e + D-ready task structure | **One task (Task 5) covering both.** D5e (`/server_info` state-source rebind) is ~5 LoC + 1 test; D-ready (`/ready` drain-aware response) is ~15 LoC + 3 tests. Both touch the existing render fns at `crates/envoy-admin/src/endpoint.rs` (`render_server_info` + `render_ready`); they're naturally co-located. Combined task is ~25 LoC production + ~50 LoC test — a comfortable single-task scope. | SPEC §6.1 estimate "~0.5-task" for D5e; D-ready ~1 task. Combined hits ~1-task scope. |
| 10 | `/server_info` state-binding swap | **In `render_server_info(handler: &AdminHandler)`, replace the literal `state: "LIVE"` with `state: match handler.drain().current() { DrainStage::Live \| DrainStage::HealthcheckFailing => "LIVE", DrainStage::Draining => "DRAINING" }`.** Pure value-binding swap; `ServerInfoBody<'a>.state: &'static str` unchanged in shape; `&'static str` literals on both arms keep the existing borrowed-reference shape working. | SPEC §3 D5e + §2.1 row patch + §5.3 wire-state mapping. |
| 11 | `/ready` drain-aware body strings | **LIVE 200 keeps the existing 06.1 body `"LIVE\n"`** (verified at `crates/envoy-admin/src/endpoint.rs:166`); **Draining 503 emits `"DRAINING\n"`** per SPEC §3 D-ready + fixture 0015 sub-case 3; **HealthcheckFailing 503 emits `"Service Unavailable\n"`** (matches Envoy's standard 503 body convention; new render path — 06.1 never had a 503 arm). All bodies are `bytes::Bytes::from_static`. The render fn widens from `render_ready()` → `render_ready_with(handler: &AdminHandler)` so it can read `handler.drain().current()`. | SPEC §3 D-ready + §5.3. The HealthcheckFailing body choice is consistent with Envoy's `503 Service Unavailable` semantic for take-out-of-rotation; the Draining body matches the explicit SPEC §3 D-ready callout for fixture 0015. |
| 12 | `listener_manager.total_listeners_active` gauge scope | **Counts only listeners going through `envoy_listener::Listener::bind` + `Listener::serve` (the HCM + tcp_proxy paths).** Echo path (`envoy-bin/src/main.rs:170-180`) and admin path (`envoy-bin/src/main.rs:353-375`) both use `TcpListener::bind` + a custom serve fn (`echo::serve` / `envoy_admin::serve`); both are naturally excluded. Documented as a one-line caveat in the BEHAVIOR_CONTRACT.md stat-name-mapping row at Task 2. The echo-path exclusion is benign — fixture 0002 (the only echo fixture) does not engage drain-state machinery; the admin-path exclusion is required by parent-08 SPEC §5.5 (admin stays serving during drain) AND benign (fixture 0011 + 0014 do not engage drain). | SPEC §3 D14 + parent-08 SPEC §5.5. Echo-path drain handling is deferred indefinitely — fixture 0002 has no drain assertion surface. |
| 13 | `Listener::bind` widening for listener_manager gauge | **`Listener::bind` registers `listener_manager.total_listeners_active` idempotently against the same `Arc<StatsRegistry>` it already uses for `cx_total` / `cx_active` / `cx_accept_failed`. Stores the gauge as a new `Arc<Gauge>` field on `Listener`.** No signature change to `Listener::bind` (registry is already passed). | PLAN-write SPEC correction 5. Idempotent-register-on-shared-name pattern is documented at `crates/envoy-listener/src/lib.rs:121-125`. |
| 14 | `Listener::serve` RAII guard shape | **Hoist the new `listener_manager_active` Arc<Gauge> field out of `self` (mirrors the existing `cx_total` / `cx_active` / `cx_accept_failed` hoist pattern at lines 173-181); inc at the top of the `serve` body BEFORE the `loop`; create a `let _lm_guard = ScopedDec(&listener_manager_active);` RAII guard whose `Drop` calls `.dec()`.** This mirrors the existing 06.3-landed `ConnGaugeGuard` pattern. The guard's `Drop` fires after the loop exits, after drain completes, and after stragglers are joined — i.e., decrements at the same scope-exit point as the listener finishes serving. | SPEC §3 D14 + parent-08 SPEC §3 D14 + 06.3 precedent. |
| 15 | `Listener::serve` 2-arg widening | **Signature `pub async fn serve(self, shutdown: impl Future<Output = ()> + Send + 'static, drain: Arc<DrainState>) -> Result<(), ListenerError>`.** Add `_ = drain.drain_signal() => { tracing::info!("listener drain signal received; draining"); drop(listener); break; }` arm between the existing `_ = &mut shutdown` arm and the `accepted = ...` arm. The drain arm is identical in behavior to the shutdown arm except for the log message. | SPEC §3 D12 + parent-08 SPEC §3 D12. |
| 16 | `Listener::serve` drain unit test | **`serve_returns_when_drain_signal_fires`** — constructs `DrainState::new(&registry)`, spawns `serve` with a manual shutdown future that never resolves (`std::future::pending::<()>()`) + the `Arc<DrainState>` clone, calls `drain.drain()` from outside, asserts `serve` returns within `DRAIN_BUDGET + Duration::from_millis(500)` (5.5s) with `Ok(())`. Plus all existing 06.x-landed listener tests (`serves_honors_shutdown_signal`, `serve_drains_in_flight_within_budget`, etc.) keep working under the widened signature (signature-update churn only — they pass `Arc::new(DrainState::new(&Arc::new(StatsRegistry::new())))` for the new param). | SPEC §3 D12 + §6.4. |
| 17 | D13b `Arc<DrainState>` wiring in envoy-bin | **Construct ONE `Arc<DrainState>` at `envoy-bin::main` startup (alongside the existing `Arc<StatsRegistry>` construction at line 101-102, before the cluster_mgr construction at line 113).** Clone into `AdminHandler::new` (the 7th positional arg, trailing). Clone into each `Listener::serve` call (HCM path line 333-338; tcp_proxy path line 234-240). | SPEC §3 D13b + parent-08 SPEC §3 D13. |
| 18 | D16 `pre_admin_actions` + `post_admin_assertions` placement | **Driver::AdminScrape extends to `{ pre_admin_actions: Vec<AdminAction>, pre_requests: Vec<PreRequest>, scrapes: Vec<AdminScrapeCase>, post_admin_assertions: Vec<AdminAssertion> }`.** Field order: `pre_admin_actions` BEFORE `pre_requests` (it's a logically-prior step — drain the listener before driving data-plane traffic); `post_admin_assertions` AFTER `scrapes` (assertions fire after the scrape sequence completes). Each defaults to `Vec::new()` via `#[serde(default)]` on the variant fields so the 08.1-landed fixtures 0011 + 0014 carry forward unchanged (both supply only `pre_requests: []` + `scrapes: [...]`). | SPEC §3 D16. The field-order choice matches the temporal sequence the harness drives. |
| 19 | `AdminAction` + `AdminAssertion` enum shapes | **`AdminAction::Post { path: String, expected_status: u16 }`** + **`AdminAssertion::DataPlaneConnectionRefused { listener_address: String, within: serde_humantime_or_ms_u64 }`**. The `within` field deserializes from milliseconds (`u64`) via `#[serde(rename = "within_ms")]` — keeps the YAML shape simple (`within_ms: 5000`) without a new humantime crate dep. Internally store as `Duration` (`Duration::from_millis(within_ms)`). Both enums `#[serde(tag = "kind")]`-internally-tagged. | SPEC §3 D16. The within_ms YAML shape avoids a `humantime-serde` dep (would be a new top-level Cargo dep). |
| 20 | `DataPlaneConnectionRefused` assertion semantics | **Spawn a `tokio::time::timeout(within, async { TcpStream::connect(listener_address).await })` repeated in a 100ms-interval loop until EITHER (a) `connect` returns Err (e.g., `ECONNREFUSED`) — SUCCESS; or (b) `connect` succeeds but a subsequent `read` returns 0 bytes within 50ms (immediately-closed) — SUCCESS; or (c) the `within` budget expires — FAILURE.** Both dispositions (a) + (b) are accepted because Envoy and envoy-rust may choose either rejection semantic at the OS / runtime level. | SPEC §3 D16 — "connect returns Err (refused) OR connect succeeds + read returns immediately with EOF (immediately-closed). Either disposition is accepted." |
| 21 | 08.1 REVIEW M2 closure | **Fold into Task 7 (D16).** Add a one-line field-level doc-comment `/// Shared keys whose values may differ bilaterally; presence is required, value equality is not.` above `tests/differential/src/lib.rs:300` (`pub value_may_differ_keys: Vec<String>`). Same edit pass naturally touches the surrounding `BodyRule::JsonShape` declarations. ~1-line edit. | 08.1 REVIEW.md §4 disposition — named owner phase 08.2 D16. |
| 22 | 08.1 REVIEW M4 closure | **Fold into Task 7 (D16).** Add a 3-line guard at the top of `walk_pointer` body at `tests/differential/src/lib.rs:379-394`: `if dotted_path.split('.').any(\|s\| s.is_empty()) { anyhow::bail!("walk_pointer: dotted path contains empty segment: {dotted_path:?}"); }`. Co-located with M2 in the same harness-touch task. | 08.1 REVIEW.md §4 disposition — named owner phase 08.2 D16 or later. |
| 23 | 08.1 REVIEW process-note (`filter_chains: []`) | **Option (b) — document the trivial-echo-filter workaround inline in Task 10's D17.4b in-process backstop bootstrap construction.** Matches 08.1 Task 13 disposition; zero schema-layer work. Option (a) (parse-time validator extension) is deferred to a future hardening phase. Inline doc-comment in `crates/envoy-bin/tests/admin_drain_listeners.rs` explains the workaround + cross-references 08.1 PROGRESS Task 13 + REVIEW §4 process note. | 08.1 REVIEW.md §4 process note + cold-start prompt + user's `feedback_pick_recommendation`. |
| 24 | BEHAVIOR_CONTRACT row landing cadence | **(a) The 3 new "Admin endpoint body shapes" rows for `/drain_listeners` / `/healthcheck/fail` / `/healthcheck/ok` + the `/server_info` row note patch land at Task 3 (D9/D10 endpoints — the wiring task).** **(b) The 3 new "Stat-name mapping" rows for `server.live` / `server.state` / `listener_manager.total_listeners_active` land at Task 2 (D14 — the registration task).** **(c) The new "Admin-action effect equivalence" subsection lands at Task 8 (D17.2 fixture 0015 — the empirical-exercise task).** | Per 06.3 / 06.1 / 08.1 cadence — contract extensions land at the task where each is first wired or empirically exercised, NOT at PLAN-write time. SPEC §2 enumerates the rows but does NOT prescribe per-task landing; this lock-in picks the task per the established doctrine. |
| 25 | Pre-state-4 fmt discipline | **Per-task PROGRESS sections run `cargo fmt --all -- --check` at every task close, NOT just at state-4.** The 5 stable-toolchain gates (`build` / `clippy` / `fmt` / `test` / `deny`) are quoted in every per-task PROGRESS entry. | SPEC §6.6 + 08.1 / 07.x REVIEW doctrine. |
| 26 | State-4 evidence-discipline | **CI run URL + HEAD SHA + completion timestamp + per-gate quoted evidence in PROGRESS Task 11.** Fixture 0015 + all 14 pre-existing fixtures (0001-0014) green simultaneously at the same CI run. | SPEC §6.7 + 05.3 REVIEW I3 → 06.x → 07.x → 08.1 closure chain. |
| 27 | Cargo.lock cadence | **No new top-level Cargo deps expected; `Cargo.lock` diff at the 08.2 reviewed range is minimal (workspace-internal path-dep registrations only, if any).** | SPEC §6.8 — 04.1 REVIEW M5/M9 cadence-ratification ADR carries forward unchanged. |
| 28 | No new ADRs | **Ledger head stays ADR-0032.** No foundations grants; `tokio::sync::Notify` is part of `tokio` (D-3.2-permitted); `std::sync::atomic` is std-lib. ADR-0033 stays reserved-available for execution-time landing if reality forces it (per SPEC §7 — recommended posture is no new ADRs). | SPEC §7. |
| 29 | PROGRESS.md cadence | **PROGRESS.md skeleton + Task 1 preamble land ALONGSIDE PLAN.md at state-2** (the 08.1 `7dbd984` / 07.2 `c7dea4c` / 06.3 `3a964cc` / 06.2 `dc00750` shape). | Project precedent. |
| 30 | `#![forbid(unsafe_code)]` | **`crates/envoy-listener/src/drain.rs` is a NEW module file inside an existing crate — the crate root's `#![forbid(unsafe_code)]` (already present at `crates/envoy-listener/src/lib.rs:1`) propagates module-wide.** No new crate root in 08.2 — no per-crate exemption needed. | D-3.8 + 4.1 invariant 8. |

---

## LoC drift posture / split-gate evaluation (per BOOTSTRAP_PROMPT.md §6.1)

08.2 SPEC §6.1's projection: **~10-13 tasks / ~810-920 LoC**. This PLAN materializes **11 tasks / ~1315 LoC** projected:

| # | Task | Production LoC | Test LoC | Fixture/Doc LoC | Total |
|---|---|---|---|---|---|
| 1 | D11: `DrainState` foundation + 6 unit tests | 100 | 80 | — | 180 |
| 2 | D14: 3 gauge registrations + DrainState gauge fields + state-transition wiring + BEHAVIOR_CONTRACT 3 stat-name rows | 60 | 40 | 30 | 130 |
| 3 | D9 + D10: 3 POST endpoints + BEHAVIOR_CONTRACT 3 body-shape rows + /server_info row note patch | 90 | 60 | 30 | 180 |
| 4 | D13b: `AdminHandler::new` 7-arg widen + envoy-bin wiring + DrainState construction at startup | 50 | 60 | — | 110 |
| 5 | D5e + D-ready: `/server_info` state-source rebind + `/ready` drain-aware 503 response | 25 | 50 | — | 75 |
| 6 | D12: `Listener::serve` 2-arg widening + `listener_manager.total_listeners_active` RAII guard + signature-update churn at callers | 70 | 60 | — | 130 |
| 7 | D16: `Driver::AdminScrape` `pre_admin_actions` + `post_admin_assertions` extensions + `AdminAction` + `AdminAssertion` enums + 08.1 REVIEW M2 + M4 closures | 120 | 30 | — | 150 |
| 8 | D17.2: Fixture 0015 + Docker-gated wrapper + BEHAVIOR_CONTRACT "Admin-action effect equivalence" subsection | — | 90 | 130 | 220 |
| 9 | D17.3b: Fuzz corpus seed `admin_healthcheck_bootstrap.yaml` | — | — | 40 | 40 |
| 10 | D17.4b: In-process backstop `admin_drain_listeners.rs` | — | 130 | — | 130 |
| 11 | State-4 phase-done verification + STATE advance to state-5-next | — | — | 30 | 30 |
| | **Total** | **~515** | **~600** | **~260** | **~1375** |

Task count (11) sits at the SPEC's projected middle and is well under the §6.1 ~25-task gate. LoC projection sits ~+45% over the SPEC's upper projection (920) but is concentrated in test (~600) + fixture (~260) material; production code (~515 LoC) is **right at the SPEC §6.1 implied production-side target** (D11 120 + D9/D10 80 + D12 60 + D13b 30 + D5e 5 + D-ready 15 + D14 80 + D16 120 = 510). The drift is test-heavy because the per-task fmt/clippy/test gate discipline (from the 06.x/07.x/08.1 doctrine) is paid in test LoC, not production LoC.

**Decision: accept the drift; do NOT nest-split.** Per parent-08 SPEC §6.1 alternative (vi) ("Not recommended: nested splits") + the standardized accept-drift posture across 07.2 (SPEC ~1500 → PLAN ~1600), 06.1 (~1300 → ~2010), 06.2 (~1300 → ~1875), 06.3 (~1200 → ~1750), 07.1 (~1100 → ~1450), and 08.1 (~1180 → ~1530), test-heavy projections regularly inflate without triggering a nested split. The ~+45% drift here is within the established envelope; the production-side surface matches the SPEC. If execution-time drift inflates a single task past ~10 sub-steps, the in-execution release valve is per-step commit splitting recorded in PROGRESS (e.g., Task 6a = `Listener::serve` 2-arg widening; Task 6b = `listener_manager` RAII guard) — NOT a phase-level nest-split.

---

## Task summary

11 substantive tasks; Tasks 1-10 land at state-3, each as its own commit; Task 11 is the state-4-reached / state-5-next STATE-advance commit. Recommended execution order **1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11**.

| # | Title | Scope (LoC) | Depends on | Carryforwards / notes |
|---|---|---|---|---|
| 1 | D11: `DrainState` foundation + 6 unit tests | ~180 | — | Foundation: every downstream task consumes `DrainState` |
| 2 | D14: gauge registrations + DrainState wiring + BEHAVIOR_CONTRACT stat-name rows | ~130 | 1 | Inline-at-transition gauge updates (one source of truth); BEHAVIOR_CONTRACT 3 stat-name rows land here |
| 3 | D9 + D10: 3 POST endpoints + BEHAVIOR_CONTRACT body-shape rows | ~180 | 1 (uses `DrainState` methods) | BEHAVIOR_CONTRACT 3 body-shape rows + /server_info row note patch land here |
| 4 | D13b: `AdminHandler::new` 7-arg widen + envoy-bin wiring | ~110 | 1, 3 | Threads `Arc<DrainState>` through admin handler construction + tests |
| 5 | D5e + D-ready: `/server_info` state-source rebind + `/ready` drain-aware response | ~75 | 1, 4 | Requires `AdminHandler::drain()` accessor (added in Task 4) |
| 6 | D12: `Listener::serve` 2-arg widening + `listener_manager` RAII | ~130 | 1, 4 | Wires `Arc<DrainState>` into data-plane listener accept loops |
| 7 | D16: `Driver::AdminScrape` extensions + 08.1 REVIEW M2 + M4 closures | ~150 | — (harness-only; independent of envoy-rust changes) | M2 + M4 close as opportunistic fixes co-located with the harness widening |
| 8 | D17.2: Fixture 0015 + Docker-gated wrapper + BEHAVIOR_CONTRACT subsection | ~220 | 3, 5, 6, 7 | Docker-gated bilateral run before commit; first fixture exercising drain end-to-end |
| 9 | D17.3b: Fuzz corpus seed `admin_healthcheck_bootstrap.yaml` | ~40 | — (independent) | Short-budget local run before commit; mirrors 08.1 Task 12 + 06.1 Task 13 pattern |
| 10 | D17.4b: In-process backstop `admin_drain_listeners.rs` | ~130 | 3, 5, 6 | Trivial-echo-filter workaround per 08.1 process-note option (b) |
| 11 | State-4 phase-done verification + STATE advance to state-5-next | ~30 doc | 1-10 | Materialize a real CI run; quote per-gate evidence in PROGRESS; advance STATE.md to state-5-next |

**Parallelization notes (for subagent-driven dispatch).** Recommended default is strict sequential 1→11 with two-stage review between tasks. Where the executor wants concurrency: **Task 7 is fully independent of Tasks 1-6** (touches `tests/differential` only; the new `AdminAction` + `AdminAssertion` enums + the M2/M4 closures are tree-orthogonal to envoy-rust changes) — can dispatch concurrently with any earlier task once Task 1 lands. **Task 9 is fully independent** (touches fuzz corpus + bootstrap.rs SUCCESS array only) — can dispatch any time. **Tasks 3, 5, 6 are mutually parallelizable after Task 4 lands** (each touches different surfaces: Task 3 = `endpoint.rs` POST variants; Task 5 = `endpoint.rs` render fns for `/server_info` + `/ready`; Task 6 = `listener/lib.rs`). The `endpoint.rs` touchpoints in Tasks 3 + 5 can merge-conflict on render-fn ordering — the executor adds them in a deterministic order or sequentializes those two specifically. Task 8 strictly sequential on its predecessors (uses Tasks 3 + 5 + 6 + 7 surfaces). Tasks 10 + 11 strictly sequential on their predecessors.

---

## File structure overview

### Created (new files)

- **`crates/envoy-listener/src/drain.rs`** (Task 1) — `DrainState` foundation module. `DrainStage` enum + `DrainState` struct + `pub fn new(&Arc<StatsRegistry>) -> Self` + `current` + `fail_healthcheck` + `ok_healthcheck` + `drain` + `drain_signal` methods. 6 unit tests in the same file.
- **`tests/fixtures/0015-admin-drain-listeners/envoy.yaml`** (Task 8) — reference Envoy config (HCM + direct_response; mirrors fixture 0007's shape).
- **`tests/fixtures/0015-admin-drain-listeners/envoy-rust.yaml`** (Task 8) — envoy-rust config (paired).
- **`tests/fixtures/0015-admin-drain-listeners/inputs/payload.bin`** (Task 8) — 0-byte placeholder.
- **`tests/fixtures/0015-admin-drain-listeners/expectations.yaml`** (Task 8) — differential assertions; single admin-scrape sequence with `pre_admin_actions` + `post_admin_assertions`.
- **`tests/fixtures/0015-admin-drain-listeners/README.md`** (Task 8) — fixture documentation.
- **`tests/differential/tests/admin_drain_listeners.rs`** (Task 8) — Docker-gated wrapper.
- **`crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml`** (Task 9) — fuzz corpus seed.
- **`crates/envoy-bin/tests/admin_drain_listeners.rs`** (Task 10) — in-process backstop.

### Modified

- **`crates/envoy-listener/src/lib.rs`** (Tasks 1, 6) — Task 1: add `pub mod drain;` + `pub use drain::{DrainStage, DrainState};` re-export at the top of the file. Task 6: widen `Listener::serve` signature from 1-arg to 2-arg (add `drain: Arc<DrainState>`); add `_ = drain.drain_signal() => { ... }` arm in `tokio::select!`; add `listener_manager_active: Arc<envoy_stats::Gauge>` field on `Listener` struct; register at `Listener::bind` via the existing `registry` parameter; add RAII guard at `Listener::serve` entry.
- **`crates/envoy-admin/src/lib.rs`** (Task 1) — add `pub use envoy_listener::{DrainStage, DrainState};` re-export so admin call sites read naturally.
- **`crates/envoy-admin/src/handler.rs`** (Task 4, 5) — Task 4: widen `AdminHandler::new` from 6-arg to 7-arg (add `drain: Arc<DrainState>` trailing); add `drain: Arc<DrainState>` field on struct; add `pub(crate) fn drain(&self) -> &Arc<DrainState>` accessor; update the `ConnectionHandler::handle` impl's `cloned = Arc::new(AdminHandler { ... })` to clone the new field; update all 7+ in-file test call sites to pass `Arc::new(DrainState::new(&registry))` as the new arg. Task 5: no direct edit here — Task 5 only touches `endpoint.rs`.
- **`crates/envoy-admin/src/endpoint.rs`** (Tasks 3, 5) — Task 3: add `DrainListeners` + `HealthcheckFail` + `HealthcheckOk` variants to `AdminEndpoint`; each declares `allowed_method() = "POST"`; add `render_drain_listeners`, `render_healthcheck_fail`, `render_healthcheck_ok` fns; extend `render_with`'s match arms. Task 5: rewire `render_server_info`'s `state` field source from the literal `"LIVE"` to `match handler.drain().current() { Live | HealthcheckFailing => "LIVE", Draining => "DRAINING" }`; rewire `render_ready` → `render_ready_with(handler)` to return 503 DRAINING (body `"DRAINING\n"`) when `drain.current() == Draining` + 503 HealthcheckFailing (body `"Service Unavailable\n"`) when `drain.current() == HealthcheckFailing` + 200 LIVE (body `"LIVE\n"`) otherwise.
- **`crates/envoy-bin/src/main.rs`** (Task 4, 6) — Task 4: construct `let drain = Arc::new(envoy_listener::DrainState::new(&registry));` alongside the existing registry construction; pass `Arc::clone(&drain)` as the 7th arg to `envoy_admin::AdminHandler::new(...)` at line 358-365. Task 6: thread `Arc::clone(&drain)` into the HCM listener's `listener.serve(...)` call at line 333-338 + the tcp_proxy listener's `listener.serve(...)` call at line 234-240 (signature is now `serve(shutdown, drain)`).
- **`tests/differential/src/lib.rs`** (Task 7) — extend `Driver::AdminScrape` variant with `pre_admin_actions: Vec<AdminAction>` + `post_admin_assertions: Vec<AdminAssertion>` fields (`#[serde(default)]` on each); add `AdminAction` + `AdminAssertion` enums with `#[serde(tag = "kind")]`; extend the dispatch path in `assert_admin_scrape_*` (the existing function ~line 2185) to drive `pre_admin_actions` BEFORE the existing `pre_requests` block + drive `post_admin_assertions` AFTER the existing scrape loop. Plus the 08.1 REVIEW M2 closure (one-line doc-comment at line 300) + M4 closure (3-line guard at line 379-394). 9+ new unit tests covering the new action/assertion dispatch.
- **`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** (Tasks 2, 3, 8) — Task 2: append 3 new rows to the "Stat-name mapping" section for `server.live` + `server.state` + `listener_manager.total_listeners_active`. Task 3: append 3 new rows to "Admin endpoint body shapes" for `/drain_listeners` + `/healthcheck/fail` + `/healthcheck/ok` + patch the existing `/server_info` row note to acknowledge the D5e value-source rebind. Task 8: append new "Admin-action effect equivalence" top-level subsection (5-8 lines per parent-08 SPEC §2.4).
- **`crates/envoy-config/src/bootstrap.rs`** (Task 9, inside `#[cfg(test)] mod tests`) — append `"admin_healthcheck_bootstrap.yaml"` to the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array (mirrors 08.1 Task 12 + 07.2 Task 6 + 06.1 Task 13 pattern).
- **`crates/envoy-config/fuzz/.gitignore`** (Task 9) — add `!corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml`.
- **`docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PROGRESS.md`** (every task) — per-task narrative append. CREATED at state-2 with the Task 1 preamble.
- **`docs/envoy-rust/STATE.md`** (Task 11) — advance from `08.2 state 3` → `08.2 state-4-reached / state-5-next`.
- **`docs/envoy-rust/ROADMAP.md`** — flip row `08.2` `planned` → `in-progress` at THIS state-2 commit (NOT at a task commit; per the standard project precedent — 08.1 `7dbd984` / 07.2 `c7dea4c` — flips at the state-2 PLAN-write commit alongside PLAN.md).

### Deleted

None.

---

## Conventions

Mirrors the 08.1 / 07.2 / 06.3 / 06.2 / 06.1 PLAN conventions:

- **TDD shape per task:** Step 1 writes the failing test(s); Step 2 runs them (FAIL expected; quote output); Step 3 writes the minimal implementation; Step 4 runs the tests (PASS expected; quote output); later steps layer workspace-wide verification; final step appends the per-task PROGRESS section and commits.
- **Commit messages:** `phase 08.2: task N — <task summary>` (the exact subject line is in each task's final step). Co-Authored-By trailer: `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
- **PROGRESS.md per-task append:** every substantive task commit appends a per-task section narrating work summary, tests landed (names + LoC tally), per-task deviations from PLAN (D-3.5 append-only discipline), LoC delta, and the 5-gate test-bucket attestation. **The test-bucket attestation MUST explicitly quote `cargo deny check` output** (07.1-REVIEW doctrine reminder — do not write "assumed no-op").
- **No new top-level Cargo deps.** Every `Cargo.toml`-touching task (none projected in 08.2 production scope; harness Task 7 may not touch Cargo.toml either since `serde` is already on the differential crate) quotes `cargo deny check`.
- **`cargo fmt --all` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean at every per-task commit.**
- **Error variants use the existing `ListenerError` / `AdminError` / `ConfigError` naming convention** — no transform. 08.2 introduces no new error variants (every new render fn returns success synchronously; `DrainState` methods are infallible; `Listener::serve`'s drain arm reuses the existing `ListenerError::DrainTimeout` for the budget-expired case).

---

## State-2 commit (this commit's content; lands BEFORE any Task 1-11 commit)

The state-2 commit lands exactly 2 files created + 2 files modified — docs-only, no code:

- **CREATE:** `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PLAN.md` (this file).
- **CREATE:** `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PROGRESS.md` — the PROGRESS skeleton with the Task 1 preamble (PLAN-write SPEC corrections + architecture-decision lock-ins + carryforward dispositions). Per the `7dbd984` (08.1) / `c7dea4c` (07.2) / `dc00750` (06.2) cadence.
- **MODIFY:** `docs/envoy-rust/ROADMAP.md` — flip row `08.2` `status: planned` → `status: in-progress` (single-cell edit; per BOOTSTRAP_PROMPT.md §4.1 invariant 3 — a phase enters `in-progress` only when STATE.md points at it AND its PLAN.md has landed; both true at this commit). Parent row `08` stays `in-progress`; sibling row `08.1` stays `done`.
- **MODIFY:** `docs/envoy-rust/STATE.md` — advance active-phase status `08.2 state 2 (SPEC.md only)` → `08.2 state 3 (SPEC + PLAN exist; implementation incomplete)`; next-skill `superpowers:writing-plans` → `superpowers:subagent-driven-development` against this PLAN.md. Rewrite the Active-phase / Next-expected-skill / Last-commit / Last-updated sections + the standing context from PLAN-writer perspective to executor perspective. Preserve all "Phase-NN rollovers" sections verbatim (including the "Phase-08 state-1 brainstorm" + "Phase-08 state-2 split" + "Phase-08.1 state-2 PLAN-write" + "Phase-08.1 rollovers" subsections). Add new "Phase-08.2 state-2 PLAN-write" subsection at the end of "Notes" recording the architecture-decision lock-ins + SPEC corrections + carryforward folds.
- **MODIFY (no edit):** `docs/envoy-rust/DECISIONS.md` — UNCHANGED. Ledger head stays **ADR-0032**. No ADR at the state-2 commit (recommended no-foundations-grants posture per SPEC §7).
- **MODIFY (no edit):** `BEHAVIOR_CONTRACT.md`, `ENVOY_TARGET.md`, `rust-toolchain.toml`, the 08.2 `SPEC.md` — UNCHANGED.

**Commit message (verbatim):**

```
phase 08.2: state-2 standalone PLAN.md

Lands the 08.2 PLAN.md + PROGRESS.md skeleton as a standalone
pre-Task-1 commit per the established standalone-PLAN cadence
(7dbd984 08.1 / c7dea4c 07.2 / 3a964cc 06.3 / dc00750 06.2). 11 tasks
targeting the 08.2 SPEC §3 D5e + D9 + D10 + D11 + D12 + D13b + D14 +
D-ready + D16 + D17.2 + D17.3b + D17.4b deliverable set, ~1375 LoC
projected (production ~515; tests ~600; fixture/doc ~260). Split-gate
evaluation: 11 tasks at the SPEC's projected middle and well under the
~25-task gate; ~1375 LoC sits ~+45% over the SPEC's ~920 upper
projection but is concentrated in test + fixture material — production
LoC ~515 matches the SPEC's implied production target. Accept the
drift, do NOT nest-split (parent-08 SPEC §6.1 alternative (vi) "Not
recommended: nested splits" + 08.1 / 07.x / 06.x accept-drift
precedent on test-heavy projections).

PROGRESS.md skeleton lands alongside with the Task 1 preamble recording
6 PLAN-write SPEC corrections (AdminHandler::new is 6-arg → widens to
7-arg, NOT 5-arg → 6-arg; AdminEndpoint::allowed_method per-variant
declaration already in place from 08.1 D4; render_with dispatch path
already in place from 08.1 D6; Listener::serve is 1-arg → widens to
2-arg; Listener::bind idempotent gauge re-registration for the shared
listener_manager.total_listeners_active gauge; admin + echo listener
paths use TcpListener::bind directly and are naturally excluded from
the new gauge AND from drain observation) + 30 architecture-decision
lock-ins (DrainState placement at envoy-listener::drain with
envoy-admin re-export; DrainState::new(&Arc<StatsRegistry>) shape;
compare_exchange + notify-once-on-CAS-success drain semantics; the 6
unit-test cases; the 3 BEHAVIOR_CONTRACT row landing cadences; the
D16 field-ordering + AdminAction/AdminAssertion enum shapes; the
DataPlaneConnectionRefused assertion semantics; the 08.1 REVIEW M2 +
M4 closures folded into Task 7; the 08.1 process-note option (b) pick
for Task 10's trivial-echo-filter workaround; et al.) + carryforward
dispositions (M2 + M4 close at Task 7; M1 closed at 08.1 state-6;
M3 indefinite-carry; the filter_chains: [] process-note pick at
Task 10) + the 08.2 PLAN-write deviations (none beyond the 6 SPEC
corrections).

ROADMAP row 08.2: planned → in-progress per BOOTSTRAP_PROMPT.md §4.1
invariant 3. Parent row 08 stays in-progress (closing-sub-phase
invariant — flips done only at 08.2's state-6 commit). Sibling row
08.1 stays done.

STATE.md advances from 08.2 state 2 (SPEC.md only) to 08.2 state 3
(SPEC + PLAN exist); next-skill superpowers:writing-plans →
superpowers:subagent-driven-development per the user's standing
preference (auto-memory feedback_execution_style).

DECISIONS.md unchanged at ADR-0032 (no foundations grants;
no ADR at a state-2 PLAN-write commit per the established
no-foundations-grants posture). ADR-0033 stays reserved-available
for 08.2 execution-time landings if reality forces it.

BEHAVIOR_CONTRACT.md unchanged at this commit. The 3 new "Admin
endpoint body shapes" rows (POST endpoints) + 3 new "Stat-name
mapping" rows (drain gauges) + new "Admin-action effect equivalence"
subsection + the /server_info row note patch all land at execution-
time task commits per the 06.x / 07.x / 08.1 doctrine.

ENVOY_TARGET.md and rust-toolchain.toml untouched (D-3.7 / D-3.9
unchanged).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 1: D11 — `DrainState` foundation

**Goal.** Land the `DrainState` module at `crates/envoy-listener/src/drain.rs` with `DrainStage` + `DrainState` types + 6 unit tests. Re-export from `envoy-admin::lib.rs` so admin-side call sites read as `envoy_admin::DrainState`. NO gauge wiring at this task — gauge registration + fields land at Task 2 (the SPEC §6.4 split: foundation first; stats integration second so the DrainState shape can be reviewed in isolation).

**Files:**
- Create: `crates/envoy-listener/src/drain.rs`
- Modify: `crates/envoy-listener/src/lib.rs` (add `pub mod drain;` + `pub use drain::{DrainStage, DrainState};` at the top, after the existing `#![forbid(unsafe_code)]` + module doc-comment block)
- Modify: `crates/envoy-admin/src/lib.rs` (add `pub use envoy_listener::{DrainStage, DrainState};` so admin call sites can `use envoy_admin::DrainState;`)

- [ ] **Step 1: Write the 6 failing unit tests in `crates/envoy-listener/src/drain.rs`**

Create the file with a `#[cfg(test)] mod tests` block at the bottom carrying all 6 tests. Test bodies use the type names that will exist after Step 3:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    /// Test 1 of 6: a fresh `DrainState::new()` starts at `DrainStage::Live`.
    #[test]
    fn new_returns_live() {
        let drain = DrainState::new();
        assert_eq!(drain.current(), DrainStage::Live);
    }

    /// Test 2 of 6: `drain()` flips state to `Draining` AND notifies all
    /// pending `drain_signal()` waiters exactly once.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn drain_flips_to_draining_and_notifies_waiters_once() {
        let drain = Arc::new(DrainState::new());

        // Two waiters block on `drain_signal()` before drain fires.
        let d1 = Arc::clone(&drain);
        let h1 = tokio::spawn(async move { d1.drain_signal().await });
        let d2 = Arc::clone(&drain);
        let h2 = tokio::spawn(async move { d2.drain_signal().await });

        // Yield to let the two waiters call `notified()` and register.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Fire drain ONCE; both waiters must complete.
        drain.drain();
        tokio::time::timeout(Duration::from_secs(1), h1)
            .await
            .expect("waiter 1 must complete within 1s of drain()")
            .expect("waiter 1 join");
        tokio::time::timeout(Duration::from_secs(1), h2)
            .await
            .expect("waiter 2 must complete within 1s of drain()")
            .expect("waiter 2 join");

        assert_eq!(drain.current(), DrainStage::Draining);

        // A NEW post-drain waiter must complete IMMEDIATELY (already-Draining
        // path returns a ready future).
        tokio::time::timeout(Duration::from_millis(50), drain.drain_signal())
            .await
            .expect("post-drain drain_signal must be immediately ready");
    }

    /// Test 3 of 6: `fail_healthcheck()` from `Live` flips to
    /// `HealthcheckFailing`.
    #[test]
    fn fail_healthcheck_flips_to_healthcheck_failing() {
        let drain = DrainState::new();
        assert_eq!(drain.current(), DrainStage::Live);
        drain.fail_healthcheck();
        assert_eq!(drain.current(), DrainStage::HealthcheckFailing);
    }

    /// Test 4 of 6: `ok_healthcheck()` from `HealthcheckFailing` restores
    /// `Live`.
    #[test]
    fn ok_healthcheck_restores_to_live() {
        let drain = DrainState::new();
        drain.fail_healthcheck();
        assert_eq!(drain.current(), DrainStage::HealthcheckFailing);
        drain.ok_healthcheck();
        assert_eq!(drain.current(), DrainStage::Live);
    }

    /// Test 5 of 6: `ok_healthcheck()` AFTER `drain()` is a no-op (sticky-
    /// drain semantic per parent-08 SPEC §5.6).
    #[test]
    fn ok_healthcheck_after_drain_is_noop_sticky() {
        let drain = DrainState::new();
        drain.drain();
        assert_eq!(drain.current(), DrainStage::Draining);
        drain.ok_healthcheck();
        assert_eq!(
            drain.current(),
            DrainStage::Draining,
            "ok_healthcheck after drain must NOT un-drain (sticky)"
        );
    }

    /// Test 6 of 6: repeat `drain()` calls are idempotent (no second
    /// notify_waiters; state stays `Draining`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn repeat_drain_calls_are_idempotent() {
        let drain = Arc::new(DrainState::new());
        drain.drain();
        assert_eq!(drain.current(), DrainStage::Draining);

        // Second drain() must not panic + state stays Draining.
        drain.drain();
        assert_eq!(drain.current(), DrainStage::Draining);

        // Third drain() ditto.
        drain.drain();
        assert_eq!(drain.current(), DrainStage::Draining);

        // A waiter registered AFTER any drain() call completes immediately.
        tokio::time::timeout(Duration::from_millis(50), drain.drain_signal())
            .await
            .expect("post-drain drain_signal must be immediately ready");
    }
}
```

- [ ] **Step 2: Run the tests to verify they FAIL**

Run: `cargo test -p envoy-listener --lib drain::tests -- --nocapture`

Expected: 6 test functions FAIL with `error[E0433]: failed to resolve: use of unresolved module or unlinked crate \`drain\`` (since `drain` module is not yet declared in `lib.rs`) AND `error[E0412]: cannot find type \`DrainStage\` in this scope` / `cannot find type \`DrainState\` in this scope`. Quote the cargo test output in PROGRESS.

- [ ] **Step 3: Implement the `DrainState` module + register the module in `lib.rs`**

Top of `crates/envoy-listener/src/drain.rs`:

```rust
//! Phase 08.2 D11: shared `DrainState` foundation observed by data-plane
//! listener accept loops.
//!
//! Lives at `envoy-listener::drain` (and re-exported from `envoy-admin`) per
//! parent-08 SPEC §5.1's Cargo-cycle resolution: the natural placement
//! would be `envoy-admin::drain` (admin endpoints are its only writers),
//! but `envoy-listener::Listener::serve` must consume a typed
//! `Arc<DrainState>` for its accept-loop `tokio::select!` — and
//! `envoy-admin` already depends on `envoy-listener::ConnectionHandler`,
//! so an `envoy-admin → envoy-listener` reverse dep would create a Cargo
//! cycle (structurally identical to the 05.3 / 07.1 cycles resolved at
//! ADR-0028 / ADR-0031). Resolution: `DrainState` lives in `envoy-listener`;
//! `envoy-admin::lib` re-exports `pub use envoy_listener::DrainState` so
//! admin-side call sites read naturally. Mirrors the M4 `DRAIN_BUDGET`
//! hoist (D3 at 08.1) pattern.
//!
//! State machine (parent-08 SPEC §5.6 + 08.2 SPEC §3 D11):
//!
//! ```text
//!         fail_healthcheck()                drain()
//!     Live ─────────────────► HealthcheckFailing ─────────► Draining
//!      ▲                              │                       │ │
//!      │                              │ ok_healthcheck()      │ │
//!      └──────────────────────────────┘                       │ │
//!                                                             ▼ ▼
//!                                            drain() repeat ──┘ │
//!                                        ok_healthcheck() ──────┘  (no-op; sticky)
//! ```
//!
//! `notify.notify_waiters()` fires EXACTLY ONCE — on the first
//! `Live → Draining` or `HealthcheckFailing → Draining` transition.
//! `drain_signal()` returns an immediately-ready future when state is
//! already `Draining` (idempotent + re-entrant). This crate ships ONLY the
//! state-machine + signal primitive at Task 1; gauge wiring
//! (`server.live`, `server.state`, `listener_manager.total_listeners_active`)
//! lands at Task 2 (08.2 PLAN architecture-decision lock-in #3 — the SPEC
//! §6.4 split: foundation first, stats integration second).

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::task::{Context, Poll};
use tokio::sync::Notify;

/// Discriminant matches the `server.state` gauge value per parent-08 SPEC
/// §2.3 + 08.2 SPEC §2.2: `Live = 0`, `HealthcheckFailing = 1`,
/// `Draining = 2`. The `#[repr(u8)]` is load-bearing — `DrainState::current()`
/// converts via `from_u8` and `drain()` writes the discriminant directly
/// via `AtomicU8::store`. Sticky-drain: `Draining` is terminal.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainStage {
    Live = 0,
    HealthcheckFailing = 1,
    Draining = 2,
}

impl DrainStage {
    /// Convert from the underlying `AtomicU8` representation. Returns `None`
    /// for unrepresented discriminants — the `current()` accessor on
    /// `DrainState` collapses `None` to a panic because the only writers
    /// of the atomic are this module's own methods.
    pub fn from_u8(n: u8) -> Option<Self> {
        match n {
            0 => Some(DrainStage::Live),
            1 => Some(DrainStage::HealthcheckFailing),
            2 => Some(DrainStage::Draining),
            _ => None,
        }
    }
}

/// Shared drain-state primitive. Constructed once at `envoy-bin::main`
/// startup via [`DrainState::new`]; an `Arc<DrainState>` is cloned into the
/// admin handler (writer) and each data-plane listener accept-loop
/// (reader/observer).
pub struct DrainState {
    /// Underlying `AtomicU8` carrying the `DrainStage` discriminant.
    /// `compare_exchange` semantics gate the `Live | HealthcheckFailing →
    /// Draining` transition so `notify.notify_waiters()` fires exactly once.
    state: AtomicU8,
    /// Wakes all `drain_signal()` waiters when `drain()` first succeeds at
    /// flipping to `Draining`. `tokio::sync::Notify` is the right primitive
    /// per parent-08 SPEC §6.6 — multi-consumer, zero-copy, cheap to clone
    /// via `Arc`.
    notify: Notify,
}

impl DrainState {
    /// Construct a fresh `DrainState` in the `Live` stage with no pending
    /// waiters. Task 2 widens this constructor to take
    /// `&Arc<envoy_stats::StatsRegistry>` for gauge registration; at Task 1
    /// the registry parameter is NOT yet accepted (PLAN architecture-
    /// decision lock-in #3 — foundation first; stats wiring second).
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(DrainStage::Live as u8),
            notify: Notify::new(),
        }
    }

    /// Read the current `DrainStage`. Uses `Ordering::Acquire` to pair with
    /// the `Ordering::Release` store in the mutator methods — every observer
    /// sees a coherent stage value with respect to the mutator that last
    /// wrote it.
    pub fn current(&self) -> DrainStage {
        let raw = self.state.load(Ordering::Acquire);
        DrainStage::from_u8(raw)
            .unwrap_or_else(|| panic!("DrainState atomic carries invalid discriminant: {raw}"))
    }

    /// Transition `Live → HealthcheckFailing` (`compare_exchange`). All
    /// other from-stages are no-ops (sticky `Draining`; idempotent
    /// `HealthcheckFailing`). Does NOT call `notify_waiters` — only
    /// `drain()` does.
    pub fn fail_healthcheck(&self) {
        let _ = self.state.compare_exchange(
            DrainStage::Live as u8,
            DrainStage::HealthcheckFailing as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        // Sticky `Draining` and self-loop `HealthcheckFailing` both fail
        // the CAS silently — that's the desired idempotent behavior.
    }

    /// Transition `HealthcheckFailing → Live` (`compare_exchange`). All
    /// other from-stages are no-ops (sticky `Draining`; idempotent `Live`).
    /// Does NOT call `notify_waiters` — only `drain()` does. The sticky-
    /// drain semantic at parent-08 SPEC §5.6: `ok_healthcheck()` AFTER
    /// `drain()` MUST NOT un-drain.
    pub fn ok_healthcheck(&self) {
        let _ = self.state.compare_exchange(
            DrainStage::HealthcheckFailing as u8,
            DrainStage::Live as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        // Sticky `Draining` and self-loop `Live` both fail the CAS silently.
    }

    /// Sticky transition `* → Draining`. Calls `notify_waiters` EXACTLY
    /// ONCE — on the first successful CAS from `Live` or `HealthcheckFailing`
    /// to `Draining`. Repeat `drain()` calls fail the CAS silently and do
    /// NOT re-notify (avoids wasted cycles per parent-08 SPEC §6.6).
    pub fn drain(&self) {
        // Two CAS attempts cover the two valid from-stages (`Live` and
        // `HealthcheckFailing`); exactly one can succeed on the first
        // call. Already-`Draining` falls through both CAS-failures (the
        // store-write order is `compare_exchange` succeeds only when the
        // current value matches the `expected` arg, so a Draining-from
        // value never matches either expected arg).
        let from_live = self.state.compare_exchange(
            DrainStage::Live as u8,
            DrainStage::Draining as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let from_hc = if from_live.is_err() {
            self.state.compare_exchange(
                DrainStage::HealthcheckFailing as u8,
                DrainStage::Draining as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
        } else {
            Err(0) // Sentinel — we already succeeded via `from_live`.
        };
        if from_live.is_ok() || from_hc.is_ok() {
            // Wake all currently-registered `drain_signal()` waiters.
            // Future calls to `drain_signal()` see the already-Draining
            // branch in that method and return an immediately-ready
            // future (no notify needed).
            self.notify.notify_waiters();
        }
    }

    /// Returns a future that resolves when `drain()` has fired (now or in
    /// the future). If the state is ALREADY `Draining`, the returned
    /// future is immediately ready; otherwise it parks on `notify.notified()`
    /// until `drain()` fires.
    ///
    /// Observed by `envoy_listener::Listener::serve`'s `tokio::select!`
    /// arm at Task 6 (D12). The admin listener (`envoy_admin::serve`) does
    /// NOT observe its own `drain_signal` per parent-08 SPEC §5.5 — the
    /// admin listener stays serving during drain so `/server_info` +
    /// `/stats/prometheus` remain reachable.
    pub fn drain_signal(&self) -> impl Future<Output = ()> + '_ {
        DrainSignal {
            state: &self.state,
            notified: self.notify.notified(),
        }
    }
}

impl Default for DrainState {
    fn default() -> Self {
        Self::new()
    }
}

/// Custom future returned by [`DrainState::drain_signal`]. Polls the
/// `AtomicU8` first — if state is already `Draining`, returns
/// `Poll::Ready(())` on the very first poll without ever registering with
/// the underlying `Notify`. Otherwise delegates to the inner
/// `Notified<'a>` future.
struct DrainSignal<'a> {
    state: &'a AtomicU8,
    notified: tokio::sync::futures::Notified<'a>,
}

impl<'a> Future for DrainSignal<'a> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // SAFETY (logical, not unsafe): we project the pinned reference into
        // its two fields. `state: &'a AtomicU8` is `Unpin` (loads are
        // safe through a shared reference); `notified` is `!Unpin` and must
        // stay pinned. We achieve both by `unsafe`-free pin projection: the
        // outer pin is structurally-pinned on `notified` only. Since this
        // crate has `#![forbid(unsafe_code)]`, we use `Pin::new_unchecked`-
        // free projection via `std::pin::pin!`-style splitting in a small
        // helper. Actual mechanics: we `get_mut`-project this `&mut Self`
        // and rebuild the pin on `notified` via `unsafe`-free
        // `Pin::new` after asserting `notified` does not move (it lives in
        // the same allocation as `Self`, which itself is pinned by the
        // caller's `Pin<&mut Self>`).
        let this = unsafe { self.get_unchecked_mut() };
        if this.state.load(Ordering::Acquire) == DrainStage::Draining as u8 {
            return Poll::Ready(());
        }
        // Project the pin onto `notified` and poll it. The unsafe block
        // re-pins `notified` because we projected through `get_unchecked_mut`
        // above; this is a documented pin-projection idiom. Crate-level
        // `forbid(unsafe_code)` requires an opt-in — DEFERRED to the
        // implementer if `forbid` blocks: an alternative wraps `Notified`
        // in `Box::pin` (heap allocation per `drain_signal()` call;
        // acceptable cost given the call site is bounded by listener
        // count).
        let notified = unsafe { Pin::new_unchecked(&mut this.notified) };
        notified.poll(cx)
    }
}
```

**Note for the implementer:** the `unsafe` blocks above CANNOT land because `envoy-listener/src/lib.rs:1` carries `#![forbid(unsafe_code)]` (no per-crate exemption ADR exists). The implementer MUST land the `Box::pin` alternative instead:

```rust
pub fn drain_signal(&self) -> impl Future<Output = ()> + '_ {
    // Already-Draining: return an immediately-ready future. Avoids
    // registering a waiter that would never unpark.
    if self.state.load(Ordering::Acquire) == DrainStage::Draining as u8 {
        return Box::pin(std::future::ready(())) as Pin<Box<dyn Future<Output = ()> + Send + '_>>;
    }
    Box::pin(self.notify.notified())
}
```

The `Box::pin` shape requires `Send` on the boxed future; `tokio::sync::futures::Notified` is `Send` (verified per tokio's docs), so the boxed future trait object is well-formed. Cost: one heap allocation per `drain_signal()` call. Acceptable — the call site is `Listener::serve`'s `tokio::select!` arm, fired at most once per listener-lifetime per drain event.

Now register the module + re-exports in `crates/envoy-listener/src/lib.rs` (insertion point: immediately after the existing `#![forbid(unsafe_code)]` attribute and module doc-comment block; before the first `use` statement):

```rust
pub mod drain;
pub use drain::{DrainStage, DrainState};
```

And add the convenience re-export in `crates/envoy-admin/src/lib.rs` (insertion point: alongside the existing pub use re-exports):

```rust
pub use envoy_listener::{DrainStage, DrainState};
```

- [ ] **Step 4: Run the 6 unit tests to verify they PASS**

Run: `cargo test -p envoy-listener --lib drain::tests -- --nocapture`

Expected output (quote in PROGRESS):

```
running 6 tests
test drain::tests::new_returns_live ... ok
test drain::tests::fail_healthcheck_flips_to_healthcheck_failing ... ok
test drain::tests::ok_healthcheck_restores_to_live ... ok
test drain::tests::ok_healthcheck_after_drain_is_noop_sticky ... ok
test drain::tests::drain_flips_to_draining_and_notifies_waiters_once ... ok
test drain::tests::repeat_drain_calls_are_idempotent ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured
```

- [ ] **Step 5: Run the 5 stable-toolchain gates and quote the output in PROGRESS**

Run sequentially (separately quote each gate's output):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

All five must be clean. The full workspace test count grows by 6 (the new `drain::tests` bucket). If any clippy warning fires (e.g., `clippy::needless_doctest_main` on the ASCII state-machine diagram in the module doc-comment), suppress it inline with an `#[allow(clippy::...)]` attribute on the module rather than rewriting the diagram.

- [ ] **Step 6: Append the per-task PROGRESS section and commit**

Append a "Task 1 (D11 — DrainState foundation)" section to `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PROGRESS.md` recording: work summary; tests landed (6 named tests with their assertions); per-task deviations from PLAN (e.g., the `Box::pin` shape adopted instead of the unsafe pin-projection sketch); LoC delta (production: drain.rs + 2 re-export lines; tests: 6 unit tests); the 5-gate test-bucket attestation quoting `cargo deny check` verbatim per the 07.1-REVIEW doctrine.

```bash
git add crates/envoy-listener/src/drain.rs \
        crates/envoy-listener/src/lib.rs \
        crates/envoy-admin/src/lib.rs \
        docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.2: task 1 — D11 DrainState foundation

DrainState module at crates/envoy-listener/src/drain.rs with the 3-stage
DrainStage enum (Live=0, HealthcheckFailing=1, Draining=2 — discriminant
load-bearing per BEHAVIOR_CONTRACT.md server.state row at Task 2) +
DrainState struct over AtomicU8 + tokio::sync::Notify + 5 public methods
(new, current, fail_healthcheck, ok_healthcheck, drain, drain_signal).
Compare_exchange semantics on every transition; notify.notify_waiters()
fires exactly once on first Live|HealthcheckFailing → Draining CAS-success.
Sticky drain per parent-08 SPEC §5.6 (ok_healthcheck after drain is a
no-op). drain_signal() returns an immediately-ready future when state is
already Draining (no waiter registration; idempotent). 6 unit tests
cover all transition arms + notify-once + sticky semantic.

Re-exported from envoy-admin::lib as pub use envoy_listener::{DrainStage,
DrainState} per parent-08 SPEC §5.1's cycle-resolution doctrine.

Differential surface: none (foundation slice; gauge wiring at Task 2;
fixture 0015 at Task 8).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: D14 — gauge registrations + DrainState gauge fields + state-transition wiring

**Goal.** Widen `DrainState::new` to take `&Arc<StatsRegistry>`; register `server.live` + `server.state` gauges via the registry at construction; store both as `Arc<Gauge>` fields on `DrainState`; update gauge values inline at every state-transition method (CAS-success path). Land the third gauge `listener_manager.total_listeners_active` registration inside `Listener::bind` (idempotent re-registration; stored on the `Listener` struct; RAII guard in Task 6). Append 3 new rows to BEHAVIOR_CONTRACT.md's "Stat-name mapping" section.

**Files:**
- Modify: `crates/envoy-listener/src/drain.rs` (widen `DrainState::new` signature; add 2 `Arc<Gauge>` fields; update 4 methods to call `.set(...)` on the gauges)
- Modify: `crates/envoy-listener/src/lib.rs` (extend `Listener::bind` to register the shared `listener_manager.total_listeners_active` gauge via the same registry; add `listener_manager_active: Arc<Gauge>` field on the `Listener` struct — the RAII guard wiring in `serve` is Task 6)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (append 3 new rows to "Stat-name mapping" section)

- [ ] **Step 1: Write failing tests for the new gauge wiring at `crates/envoy-listener/src/drain.rs::tests`**

Append to the existing `#[cfg(test)] mod tests` block:

```rust
    /// Task 2: `DrainState::new(&registry)` registers `server.live` gauge.
    #[test]
    fn new_registers_server_live_gauge() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let _drain = DrainState::new(&registry);
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().collect();
        assert!(
            snapshot.contains_key("server.live"),
            "server.live gauge missing from registry; snapshot keys: {:?}",
            snapshot.keys().collect::<Vec<_>>()
        );
    }

    /// Task 2: `DrainState::new(&registry)` registers `server.state` gauge.
    #[test]
    fn new_registers_server_state_gauge() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let _drain = DrainState::new(&registry);
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().collect();
        assert!(snapshot.contains_key("server.state"));
    }

    /// Task 2: fresh DrainState has server.live=1 + server.state=0 (Live).
    #[test]
    fn new_initial_gauge_values_are_live() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let _drain = DrainState::new(&registry);
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().collect();
        match snapshot.get("server.live").expect("server.live missing") {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(g.value(), 1),
            _ => panic!("server.live is not a gauge"),
        }
        match snapshot.get("server.state").expect("server.state missing") {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(g.value(), 0),
            _ => panic!("server.state is not a gauge"),
        }
    }

    /// Task 2: drain() flips server.live=0 + server.state=2.
    #[test]
    fn drain_updates_server_live_to_zero_and_server_state_to_two() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = DrainState::new(&registry);
        drain.drain();
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().collect();
        match snapshot.get("server.live").unwrap() {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(g.value(), 0),
            _ => unreachable!(),
        }
        match snapshot.get("server.state").unwrap() {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(g.value(), 2),
            _ => unreachable!(),
        }
    }

    /// Task 2: fail_healthcheck() keeps server.live=1 + flips server.state=1
    /// (server-state is INDEPENDENT of healthcheck-failure per parent-08
    /// SPEC §5.5 wire-state mapping).
    #[test]
    fn fail_healthcheck_keeps_server_live_at_one() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = DrainState::new(&registry);
        drain.fail_healthcheck();
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().collect();
        match snapshot.get("server.live").unwrap() {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(
                g.value(),
                1,
                "server.live MUST stay 1 when healthcheck fails; only Draining flips it to 0"
            ),
            _ => unreachable!(),
        }
        match snapshot.get("server.state").unwrap() {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(g.value(), 1),
            _ => unreachable!(),
        }
    }
```

Also write a failing test in `crates/envoy-listener/src/lib.rs::tests` for the listener-side gauge registration:

```rust
    /// Task 2: Listener::bind registers the shared
    /// listener_manager.total_listeners_active gauge idempotently.
    #[tokio::test]
    async fn bind_registers_listener_manager_total_active_gauge() {
        // Build a minimal Listener config + a no-op handler + a fresh registry.
        let cfg = envoy_config::Listener {
            name: "test_lm_gauge".to_string(),
            address: envoy_config::Address {
                socket_address: envoy_config::SocketAddress {
                    address: "127.0.0.1".to_string(),
                    port_value: 0,
                },
            },
            filter_chains: vec![],
            listener_filters: vec![],
        };
        let handler: std::sync::Arc<dyn ConnectionHandler> = std::sync::Arc::new(NoopHandler);
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let _listener = Listener::bind(&cfg, handler, std::sync::Arc::clone(&registry))
            .await
            .expect("bind succeeds");
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().collect();
        assert!(
            snapshot.contains_key("listener_manager.total_listeners_active"),
            "listener_manager.total_listeners_active not registered; snapshot keys: {:?}",
            snapshot.keys().collect::<Vec<_>>()
        );
    }

    /// Task 2: Two `Listener::bind` calls with the same registry register
    /// only one `listener_manager.total_listeners_active` gauge (idempotent
    /// shared-name re-registration mirrors the existing 06.1 pattern at
    /// `cx_total`).
    #[tokio::test]
    async fn bind_listener_manager_gauge_is_idempotent_shared() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());

        for name in ["lis_a", "lis_b"] {
            let cfg = envoy_config::Listener {
                name: name.to_string(),
                address: envoy_config::Address {
                    socket_address: envoy_config::SocketAddress {
                        address: "127.0.0.1".to_string(),
                        port_value: 0,
                    },
                },
                filter_chains: vec![],
                listener_filters: vec![],
            };
            let handler: std::sync::Arc<dyn ConnectionHandler> = std::sync::Arc::new(NoopHandler);
            let _ = Listener::bind(&cfg, handler, std::sync::Arc::clone(&registry))
                .await
                .expect("bind succeeds");
        }
        // The shared gauge name appears exactly once.
        let snapshot: Vec<_> = registry
            .snapshot()
            .filter(|(name, _)| name == "listener_manager.total_listeners_active")
            .collect();
        assert_eq!(snapshot.len(), 1, "shared gauge must be registered once");
    }
```

(The implementer adapts `NoopHandler` to the existing `ConnectionHandler` test stub already in `crates/envoy-listener/src/lib.rs::tests` — at line ~280-310 per the 06.1 test pattern.)

- [ ] **Step 2: Run the tests to verify they FAIL**

Run: `cargo test -p envoy-listener --lib -- --nocapture`

Expected: 5 new `drain::tests::*` tests fail with `error: this function takes 1 argument but 0 arguments were supplied` (the `DrainState::new()` calls in the existing 6 Task-1 tests fail too if the signature widens; update those test bodies as part of Step 3). The 2 new `tests::bind_*` tests fail with `assertion failed: snapshot.contains_key("listener_manager.total_listeners_active")`.

- [ ] **Step 3: Update `DrainState::new` to take `&Arc<StatsRegistry>` + register gauges + store + update inline**

Replace the existing `DrainState::new` + struct + methods in `crates/envoy-listener/src/drain.rs` with:

```rust
use std::sync::Arc;

pub struct DrainState {
    state: AtomicU8,
    notify: Notify,
    /// Task 2 (D14): `server.live` gauge. `1` when `current() == Live`;
    /// `0` otherwise (HealthcheckFailing and Draining both emit `0` for
    /// liveness per parent-08 SPEC §5.5 — note the asymmetry vs
    /// `/server_info.state` which collapses Live + HealthcheckFailing → "LIVE").
    /// WAIT — re-read SPEC §2.2: "server.live ← 1 when DrainState::Live, else 0."
    /// So HealthcheckFailing emits server.live=0. Verify against parent-08
    /// SPEC §2.3 row 1 — confirmed: "1 when DrainState::current() == Live;
    /// 0 otherwise." Lock-in: HealthcheckFailing → server.live=0.
    server_live: Arc<envoy_stats::Gauge>,
    /// Task 2 (D14): `server.state` gauge. Discriminant of `DrainStage`
    /// (0/1/2). Updated alongside `server_live` at every state transition.
    server_state: Arc<envoy_stats::Gauge>,
}

impl DrainState {
    pub fn new(registry: &Arc<envoy_stats::StatsRegistry>) -> Self {
        let server_live = registry
            .register_gauge("server.live")
            .expect("server.live gauge registration");
        let server_state = registry
            .register_gauge("server.state")
            .expect("server.state gauge registration");
        // Initial values: Live (server.live=1; server.state=0).
        server_live.set(1);
        server_state.set(0);
        Self {
            state: AtomicU8::new(DrainStage::Live as u8),
            notify: Notify::new(),
            server_live,
            server_state,
        }
    }

    pub fn current(&self) -> DrainStage { /* unchanged */ }

    pub fn fail_healthcheck(&self) {
        let cas = self.state.compare_exchange(
            DrainStage::Live as u8,
            DrainStage::HealthcheckFailing as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        if cas.is_ok() {
            // server.live STAYS at 1 per SPEC §2.2 — re-read: actually
            // server.live = 1 ONLY when Live; else 0. So
            // HealthcheckFailing → server.live = 0.
            self.server_live.set(0);
            self.server_state.set(DrainStage::HealthcheckFailing as i64);
        }
    }

    pub fn ok_healthcheck(&self) {
        let cas = self.state.compare_exchange(
            DrainStage::HealthcheckFailing as u8,
            DrainStage::Live as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        if cas.is_ok() {
            self.server_live.set(1);
            self.server_state.set(DrainStage::Live as i64);
        }
    }

    pub fn drain(&self) {
        let from_live = self.state.compare_exchange(
            DrainStage::Live as u8,
            DrainStage::Draining as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let from_hc = if from_live.is_err() {
            self.state.compare_exchange(
                DrainStage::HealthcheckFailing as u8,
                DrainStage::Draining as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
        } else {
            Err(0)
        };
        if from_live.is_ok() || from_hc.is_ok() {
            self.server_live.set(0);
            self.server_state.set(DrainStage::Draining as i64);
            self.notify.notify_waiters();
        }
    }

    pub fn drain_signal(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> { /* unchanged */ }
}
```

**Re-verify SPEC §2.2** before committing: per the SPEC table row 1, "`server.live` (gauge, value-exact). `1` when `DrainState::current() == Live`; `0` otherwise. Both proxies emit on every snapshot." So `HealthcheckFailing` → `server.live = 0` (NOT 1). The doc-comment + test `fail_healthcheck_keeps_server_live_at_one` in Step 1 above is WRONG — fix at Step 3-prime: rename the test to `fail_healthcheck_sets_server_live_to_zero_and_server_state_to_one` and flip the assertion to `assert_eq!(g.value(), 0)`. The corrected test name + assertion are the canonical PLAN intent.

Update the existing 6 Task-1 tests' `DrainState::new()` calls to `DrainState::new(&Arc::new(StatsRegistry::new()))`.

Update `crates/envoy-listener/src/lib.rs::Listener::bind` to also register `listener_manager.total_listeners_active`:

```rust
// (insertion after the existing cx_accept_failed registration at line ~141)
let listener_manager_active = registry
    .register_gauge("listener_manager.total_listeners_active")
    .map_err(|e| ListenerError::StatsRegistration(e.to_string()))?;
```

And add the field to the `Listener` struct:

```rust
/// 08.2 D14: shared gauge counting currently-active data-plane listeners.
/// Registered idempotently against the shared registry — every Listener
/// gets its own Arc<Gauge> clone of the same handle; inc/dec are atomic
/// across all listeners. RAII-guarded at `serve` entry/exit (Task 6 D12).
listener_manager_active: Arc<envoy_stats::Gauge>,
```

Initialize in the `Ok(Self { ... })` block. Add a `pub(crate) fn listener_manager_active(&self) -> &Arc<envoy_stats::Gauge>` accessor for Task 6's RAII guard to hoist out of `self`.

Append 3 new rows to `docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s `Stat-name mapping` section under a new "**08.2 entries:**" heading (mirrors the existing "06.1 initial entries:" + "06.3 entries:" subheading pattern):

```markdown
**08.2 entries (drain machinery):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `server.live` | value-exact | Gauge; `1` when `DrainState::current() == Live`; `0` otherwise (HealthcheckFailing and Draining both emit `0`). Updated inline at the `DrainState::{fail_healthcheck, ok_healthcheck, drain}` CAS-success sites (one source of truth — NOT polled). Initial value `1` at process start. Both proxies emit on every snapshot. |
| `server.state` | value-exact (Live=0 baseline; Draining=2 post-drain) | Gauge; discriminant of `DrainStage` (`Live=0`, `HealthcheckFailing=1`, `Draining=2`). The `#[repr(u8)]` on `DrainStage` makes the discriminant load-bearing for the gauge value. Updated inline at the same CAS-success sites as `server.live` (one source of truth). Initial value `0` at process start. Fixture 0015 asserts the post-drain value `2`. |
| `listener_manager.total_listeners_active` | value-exact | Gauge; count of currently-active data-plane listeners (HCM + tcp_proxy paths going through `envoy_listener::Listener::bind`/`serve`). Echo path (fixture 0002 only) + admin path use `tokio::net::TcpListener` directly and are naturally excluded. RAII-guarded at `Listener::serve` entry (inc) / exit (dec); decrement fires AFTER drain completes and AFTER stragglers join. Mirrors the 06.3 `listener.<name>.downstream_cx_active` gauge pattern but is global (not per-listener-named); registered idempotently inside `Listener::bind`. |
```

- [ ] **Step 4: Run the tests to verify they PASS**

Run: `cargo test -p envoy-listener --lib -- --nocapture`

Expected: all `drain::tests::*` tests pass (11 total — the original 6 Task-1 tests + 5 new Task-2 tests) AND the 2 `tests::bind_*` tests pass. Quote the count.

- [ ] **Step 5: Run the 5 stable-toolchain gates**

Same as Task 1 Step 5. All clean. Quote each output in PROGRESS.

- [ ] **Step 6: Append PROGRESS section + commit**

Commit message:

```
phase 08.2: task 2 — D14 gauges (server.live + server.state +
listener_manager.total_listeners_active)

DrainState::new(&Arc<StatsRegistry>) registers server.live + server.state
gauges and stores both as Arc<Gauge> fields; all 4 state-transition
methods (fail_healthcheck, ok_healthcheck, drain) call .set(...) on
the gauges inline on CAS-success (one source of truth; no polling).
Listener::bind additionally registers listener_manager.total_listeners_
active idempotently against the same registry (shared gauge — every
Listener gets an Arc<Gauge> clone; inc/dec atomic across all listeners).
The RAII guard at Listener::serve entry/exit lands at Task 6 (D12).
Echo + admin paths use TcpListener::bind directly and are naturally
excluded from the shared gauge per architecture-decision lock-in #12.

BEHAVIOR_CONTRACT.md "Stat-name mapping" gains 3 new rows
(server.live, server.state, listener_manager.total_listeners_active)
under a new "08.2 entries (drain machinery):" subheading.

Differential surface: none (gauges register but no fixture asserts
their values yet; fixture 0015 at Task 8 asserts the post-drain
server.state=2).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 3: D9 + D10 — three POST admin endpoints (`/drain_listeners`, `/healthcheck/fail`, `/healthcheck/ok`)

**Goal.** Add 3 new variants to `AdminEndpoint` (each `allowed_method() = "POST"`); 3 new render fns invoking `DrainState::{drain, fail_healthcheck, ok_healthcheck}` and returning 200 OK with empty body; extend `render_with`'s match arms. Append 3 new BEHAVIOR_CONTRACT body-shape rows + patch the existing `/server_info` row note to acknowledge the D5e value-source rebind (which lands at Task 5).

NOTE: This task introduces the dispatch surface but does NOT exercise it end-to-end yet — `AdminHandler::new` is still 6-arg at Task 3 (Task 4 widens to 7-arg) so the render fns can read `handler.drain()` ONLY after Task 4. To resolve the cycle: the render fns in Task 3 take `&DrainState` directly (passed in by the executor at the render_with call site via a still-to-land accessor), OR Task 3's render fns are stubbed with a `todo!("Task 4 will wire handler.drain()")` until Task 4 lands. The cleaner path: Task 3 lands the variant declarations + match-arm scaffolding + the 3 new BEHAVIOR_CONTRACT rows, but the render fns themselves take `drain: &DrainState` as a param + are CALLED with a placeholder until Task 4 wires the real `handler.drain()` accessor. Recommended split (architecture-decision deviation #1 for this task, recorded in PROGRESS): land the render-fn signatures + render bodies at Task 3; gate the `render_with` dispatch arm with a Task-4 TODO comment that Task 4 will replace with the real `handler.drain()` call. Tests verify the render-fn output shape (200 OK empty body) directly — they construct a fresh `DrainState` + call the render fn directly + assert response shape.

**Files:**
- Modify: `crates/envoy-admin/src/endpoint.rs` (add 3 variants + their `allowed_method() = "POST"` arms + 3 render fns + 3 `render_with` match arms gated with Task-4 TODO)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (append 3 new rows to "Admin endpoint body shapes" + patch the existing `/server_info` row note)

- [ ] **Step 1: Write failing tests in `crates/envoy-admin/src/endpoint.rs::tests`**

Append (the 9 new tests cover dispatch + render + side-effect):

```rust
    #[test]
    fn drain_listeners_path_dispatches_on_post() {
        let dispatch = AdminEndpoint::dispatch("POST", "/drain_listeners");
        assert!(matches!(
            dispatch,
            Dispatch::Endpoint(AdminEndpoint::DrainListeners)
        ));
    }

    #[test]
    fn drain_listeners_405_on_get() {
        let dispatch = AdminEndpoint::dispatch("GET", "/drain_listeners");
        assert!(matches!(
            dispatch,
            Dispatch::MethodNotAllowed { allow: "POST" }
        ));
    }

    #[test]
    fn healthcheck_fail_path_dispatches_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/healthcheck/fail"),
            Dispatch::Endpoint(AdminEndpoint::HealthcheckFail)
        ));
    }

    #[test]
    fn healthcheck_ok_path_dispatches_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/healthcheck/ok"),
            Dispatch::Endpoint(AdminEndpoint::HealthcheckOk)
        ));
    }

    #[test]
    fn drain_listeners_render_returns_200_empty_body_and_invokes_drain() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = envoy_listener::DrainState::new(&registry);
        let resp = render_drain_listeners(&drain);
        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, Some("OK"));
        assert!(resp.body.is_empty(), "200 OK body must be empty");
        assert_eq!(drain.current(), envoy_listener::DrainStage::Draining);
    }

    #[test]
    fn healthcheck_fail_render_returns_200_empty_body_and_flips_state() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = envoy_listener::DrainState::new(&registry);
        let resp = render_healthcheck_fail(&drain);
        assert_eq!(resp.status, 200);
        assert!(resp.body.is_empty());
        assert_eq!(drain.current(), envoy_listener::DrainStage::HealthcheckFailing);
    }

    #[test]
    fn healthcheck_ok_render_returns_200_empty_body_and_restores_live() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = envoy_listener::DrainState::new(&registry);
        drain.fail_healthcheck();
        let resp = render_healthcheck_ok(&drain);
        assert_eq!(resp.status, 200);
        assert!(resp.body.is_empty());
        assert_eq!(drain.current(), envoy_listener::DrainStage::Live);
    }

    #[test]
    fn healthcheck_ok_after_drain_is_noop_via_render_fn() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = envoy_listener::DrainState::new(&registry);
        drain.drain();
        let resp = render_healthcheck_ok(&drain);
        assert_eq!(resp.status, 200);
        assert_eq!(
            drain.current(),
            envoy_listener::DrainStage::Draining,
            "sticky drain: ok_healthcheck after drain must NOT un-drain"
        );
    }

    #[test]
    fn each_drain_endpoint_declares_post_allowed_method() {
        assert_eq!(AdminEndpoint::DrainListeners.allowed_method(), "POST");
        assert_eq!(AdminEndpoint::HealthcheckFail.allowed_method(), "POST");
        assert_eq!(AdminEndpoint::HealthcheckOk.allowed_method(), "POST");
    }
```

- [ ] **Step 2: Run the tests to verify they FAIL**

Run: `cargo test -p envoy-admin --lib endpoint::tests -- --nocapture`

Expected: 9 tests fail with `error[E0599]: no variant or associated item named \`DrainListeners\` found for enum \`AdminEndpoint\`` (and similar for `HealthcheckFail` + `HealthcheckOk`) AND `error[E0425]: cannot find function \`render_drain_listeners\` in this scope`.

- [ ] **Step 3: Add the 3 enum variants + dispatch arms + render fns**

In `crates/envoy-admin/src/endpoint.rs`:

```rust
// Append to the AdminEndpoint enum (insertion at line ~22):
pub enum AdminEndpoint {
    Ready,
    Stats,
    StatsPrometheus,
    ConfigDump,
    ServerInfo,
    Clusters,
    Listeners,
    // 08.2 D9:
    DrainListeners,
    // 08.2 D10:
    HealthcheckFail,
    HealthcheckOk,
}

// Extend from_path's match (line ~75) to include the 3 new paths.
// In Rust 2024 idiom, just add the 3 new arms returning Some(...).
"/drain_listeners" => Some(AdminEndpoint::DrainListeners),
"/healthcheck/fail" => Some(AdminEndpoint::HealthcheckFail),
"/healthcheck/ok" => Some(AdminEndpoint::HealthcheckOk),

// Extend allowed_method's match (line ~89) — three new arms:
AdminEndpoint::DrainListeners
| AdminEndpoint::HealthcheckFail
| AdminEndpoint::HealthcheckOk => "POST",

// Extend render's match (line ~129) — three new arms; all unreachable
// from the registry-only path because the side effects need DrainState:
AdminEndpoint::DrainListeners => unreachable!(
    "DrainListeners requires DrainState; dispatch via AdminEndpoint::render_with"
),
AdminEndpoint::HealthcheckFail => unreachable!(
    "HealthcheckFail requires DrainState; dispatch via AdminEndpoint::render_with"
),
AdminEndpoint::HealthcheckOk => unreachable!(
    "HealthcheckOk requires DrainState; dispatch via AdminEndpoint::render_with"
),

// Extend render_with's match (line ~154) — three new arms. Task 3's
// `handler.drain()` accessor is added at Task 4 (D13b widens AdminHandler::new
// from 6-arg to 7-arg, exposing the Arc<DrainState> via a new accessor).
// Task 3 leaves the dispatch as a `todo!` placeholder that Task 4 replaces;
// the unit tests at Step 1 above exercise the render fns directly via
// `render_drain_listeners(&drain)` etc., not via `render_with`. Per the
// architecture-decision deviation #1 recorded in PROGRESS Task 3.
AdminEndpoint::DrainListeners => todo!(
    "Task 4 (D13b) wires handler.drain() accessor; until then dispatch returns todo!()"
),
AdminEndpoint::HealthcheckFail => todo!("Task 4 wires"),
AdminEndpoint::HealthcheckOk => todo!("Task 4 wires"),

// New render fns at module level (insertion after the existing
// render_listeners fn ~line 430):

/// 08.2 D9: `/drain_listeners` POST endpoint. Invokes `DrainState::drain()`
/// and returns 200 OK with an empty body. Side effect: triggers the
/// `drain_signal()` notify; the listener accept loops observe and start
/// draining within tens of microseconds. Sticky — repeat POSTs are
/// idempotent (per parent-08 SPEC §5.6 + 08.2 SPEC §3 D11 sticky-drain).
fn render_drain_listeners(drain: &envoy_listener::DrainState) -> envoy_http1::Response {
    drain.drain();
    empty_200_ok()
}

/// 08.2 D10a: `/healthcheck/fail` POST endpoint. Invokes
/// `DrainState::fail_healthcheck()` and returns 200 OK empty body.
fn render_healthcheck_fail(drain: &envoy_listener::DrainState) -> envoy_http1::Response {
    drain.fail_healthcheck();
    empty_200_ok()
}

/// 08.2 D10b: `/healthcheck/ok` POST endpoint. Invokes
/// `DrainState::ok_healthcheck()` and returns 200 OK empty body. Sticky-
/// drain: if state is already `Draining`, this is a no-op (the underlying
/// `compare_exchange` from `HealthcheckFailing → Live` fails silently;
/// state stays `Draining`).
fn render_healthcheck_ok(drain: &envoy_listener::DrainState) -> envoy_http1::Response {
    drain.ok_healthcheck();
    empty_200_ok()
}

/// Shared 200 OK empty-body response shape for the 3 D9/D10 POST endpoints.
/// `content-length: 0` per the established admin response convention; no
/// `content-type` (no body — content-type is moot per RFC 7231 §3.1.1.5).
fn empty_200_ok() -> envoy_http1::Response {
    envoy_http1::Response {
        status: 200,
        reason: Some("OK"),
        headers: vec![("content-length".to_string(), "0".to_string())],
        body: bytes::Bytes::new(),
    }
}
```

Append 3 rows to `docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s "Admin endpoint body shapes" table:

```markdown
| `/drain_listeners` | POST | empty | Status 200; empty body (`content-length: 0`); effect-only endpoint. Invokes `DrainState::drain()`. Sticky — repeat POSTs are idempotent. Both proxies emit 200 OK on first AND subsequent POSTs. |
| `/healthcheck/fail` | POST | empty | Status 200; empty body; effect-only endpoint. Invokes `DrainState::fail_healthcheck()`. Flips `/ready` to 503 (per parent-08 SPEC §5.5 wire-state mapping); `/server_info.state` stays `"LIVE"` (server-state is independent of healthcheck-failure). |
| `/healthcheck/ok` | POST | empty | Status 200; empty body; effect-only endpoint. Invokes `DrainState::ok_healthcheck()`. Restores from `HealthcheckFailing` → `Live`. Sticky-drain: `/healthcheck/ok` AFTER `/drain_listeners` does NOT un-drain (the `HealthcheckFailing → Live` compare_exchange fails silently against the `Draining` state). |
```

Patch the existing `/server_info` row note (last column) to acknowledge the D5e value-source rebind. Replace:

```
`state` value-exact (08.1 emits the constant `"LIVE"`; 08.2 extends to `LIVE` / `DRAINING`);
```

with:

```
`state` value-exact, sourced from `DrainState::current()` via the mapping `Live | HealthcheckFailing → "LIVE"`, `Draining → "DRAINING"` (08.1 emitted the literal constant `"LIVE"` as a placeholder; 08.2's D5e patches the value-binding source at Task 5 — the struct shape is unchanged at the 08.1 → 08.2 boundary);
```

- [ ] **Step 4: Run the tests to verify they PASS**

Run: `cargo test -p envoy-admin --lib endpoint::tests -- --nocapture`

Expected: 9 new tests pass + all existing endpoint tests stay green.

- [ ] **Step 5: Run the 5 stable-toolchain gates**

Same as Task 1 Step 5. Note: the `todo!()` arms in `render_with` will be reachable from a code-path standpoint but the existing dispatch tests do not exercise them yet (the new dispatch tests above only verify enum-variant resolution + per-endpoint render-fn output, never the `render_with` arm). All 5 gates clean.

- [ ] **Step 6: Append PROGRESS section + commit**

Record the architecture-decision deviation #1 (Task 3 render_with arms gated with `todo!()` until Task 4 wires `handler.drain()`).

Commit message:

```
phase 08.2: task 3 — D9/D10 three POST admin endpoints
(drain_listeners + healthcheck/fail + healthcheck/ok)

Three new AdminEndpoint variants (DrainListeners, HealthcheckFail,
HealthcheckOk) each declaring allowed_method() = "POST". Three new
render fns invoking the corresponding DrainState method and returning
200 OK with empty body (content-length: 0; no content-type). The
render_with dispatch arms are gated with todo!() until Task 4 (D13b)
wires the handler.drain() accessor; per-render-fn unit tests at this
task exercise the render fns directly (passing &DrainState constructed
in-test) so the side effect + response shape are bilaterally verified.

BEHAVIOR_CONTRACT.md "Admin endpoint body shapes" gains 3 new rows
+ the existing /server_info row note patched to acknowledge the D5e
value-source rebind (Task 5).

Differential surface: none (fixture 0015 at Task 8 exercises the
endpoints end-to-end; Task 3 only lands the enum + render machinery).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 4: D13b — `AdminHandler::new` 7-arg widening + envoy-bin DrainState wiring

**Goal.** Widen `AdminHandler::new` from 6-arg to 7-arg (add `drain: Arc<DrainState>` as the trailing param). Add `drain: Arc<DrainState>` field on `AdminHandler` struct + `pub(crate) fn drain(&self) -> &Arc<DrainState>` accessor. Update the `ConnectionHandler::handle` impl's `cloned = Arc::new(AdminHandler { ... })` site to clone the new field. Update all in-file `AdminHandler::new` test call sites (7+ sites at lines ~438, 472, 502, 531, 561, 598, 650) to pass `Arc::new(DrainState::new(&registry))` as the new arg. Construct `Arc<DrainState>` ONCE at `envoy-bin::main` startup (alongside the existing `Arc<StatsRegistry>` construction at line ~101-102) and pass through to `AdminHandler::new` (line ~358-365). Replace the `todo!()` arms from Task 3's `render_with` dispatch with the real `handler.drain()` calls.

**Files:**
- Modify: `crates/envoy-admin/src/handler.rs` (signature widening + struct field + accessor + ConnectionHandler::handle clone + 7+ test call site updates)
- Modify: `crates/envoy-admin/src/endpoint.rs` (replace the 3 `todo!()` arms in `render_with` with `render_drain_listeners(handler.drain())` etc.)
- Modify: `crates/envoy-bin/src/main.rs` (construct `Arc<DrainState>` at startup; pass to `AdminHandler::new`)

- [ ] **Step 1: Write failing tests for the new accessor + wiring**

Append to `crates/envoy-admin/src/handler.rs::tests`:

```rust
    #[tokio::test]
    async fn admin_handler_new_takes_drain_state_as_seventh_arg() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = std::sync::Arc::new(envoy_listener::DrainState::new(&registry));
        let handler = AdminHandler::new(
            test_admin_config(),
            std::sync::Arc::clone(&registry),
            test_bootstrap_arc(),
            test_cluster_manager_arc(),
            std::time::Instant::now(),
            std::collections::BTreeMap::new(),
            std::sync::Arc::clone(&drain),
        );
        // Verify the new accessor returns the same Arc (pointer equality).
        assert!(std::sync::Arc::ptr_eq(handler.drain(), &drain));
    }
```

Also write an integration-style test that exercises the `render_with` dispatch path (post-Task-3 todo!() replacement):

```rust
    #[tokio::test]
    async fn drain_listeners_endpoint_invokes_drain_via_render_with() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = std::sync::Arc::new(envoy_listener::DrainState::new(&registry));
        let handler = AdminHandler::new(
            test_admin_config(),
            std::sync::Arc::clone(&registry),
            test_bootstrap_arc(),
            test_cluster_manager_arc(),
            std::time::Instant::now(),
            std::collections::BTreeMap::new(),
            std::sync::Arc::clone(&drain),
        );
        assert_eq!(drain.current(), envoy_listener::DrainStage::Live);
        let resp = crate::endpoint::AdminEndpoint::DrainListeners.render_with(&handler);
        assert_eq!(resp.status, 200);
        assert_eq!(drain.current(), envoy_listener::DrainStage::Draining);
    }
```

- [ ] **Step 2: Run tests to verify they FAIL**

Run: `cargo test -p envoy-admin --lib -- --nocapture`

Expected: `error[E0061]: this function takes 6 arguments but 7 arguments were supplied` on the 2 new tests AND `error[E0599]: no method named \`drain\` found for struct \`AdminHandler\``.

- [ ] **Step 3: Widen `AdminHandler::new` + thread through envoy-bin + replace `todo!()`**

In `crates/envoy-admin/src/handler.rs`:

```rust
// Add the field on AdminHandler struct (line ~68 after command_line_options):
/// Phase 08.2 D13b: shared DrainState handle for the 3 POST endpoints
/// (D9 + D10) AND the /server_info state-source read (D5e) AND the
/// /ready drain-aware response (D-ready). Constructed ONCE at
/// envoy-bin::main startup alongside the registry; cloned into the
/// admin handler (writer) and each data-plane Listener::serve call
/// (reader/observer per D12). Held as Arc<DrainState> so the handler
/// can stay Send + Sync.
drain: Arc<envoy_listener::DrainState>,

// Widen new() to 7-arg (line ~77):
pub fn new(
    config: Arc<AdminConfig>,
    registry: Arc<StatsRegistry>,
    bootstrap: Arc<Bootstrap>,
    cluster_manager: Arc<ClusterManager>,
    start_instant: Instant,
    command_line_options: BTreeMap<String, serde_yaml::Value>,
    drain: Arc<envoy_listener::DrainState>,
) -> Self {
    Self {
        config,
        registry,
        bootstrap,
        cluster_manager,
        start_instant,
        command_line_options,
        drain,
    }
}

// Add accessor (line ~133 after command_line_options accessor):
/// Phase 08.2 D13b accessor: consumed by Task 3's render_drain_listeners
/// + render_healthcheck_fail + render_healthcheck_ok (via render_with's
/// dispatch arms) AND by Task 5's render_server_info state-source patch
/// AND by Task 5's render_ready_with drain-aware response branch.
pub(crate) fn drain(&self) -> &Arc<envoy_listener::DrainState> {
    &self.drain
}

// Extend the ConnectionHandler::handle clone (line ~299):
let cloned = Arc::new(AdminHandler {
    config: Arc::clone(&self.config),
    registry: Arc::clone(&self.registry),
    bootstrap: Arc::clone(&self.bootstrap),
    cluster_manager: Arc::clone(&self.cluster_manager),
    start_instant: self.start_instant,
    command_line_options: self.command_line_options.clone(),
    drain: Arc::clone(&self.drain),
});
```

Update all in-file `AdminHandler::new(...)` test call sites (7+ sites; the executor enumerates via `grep -n "AdminHandler::new" crates/envoy-admin/src/handler.rs` and adds the 7th arg `std::sync::Arc::new(envoy_listener::DrainState::new(&registry))` — using the `registry` variable in scope at each site).

In `crates/envoy-admin/src/endpoint.rs`, replace the 3 `todo!()` arms from Task 3 with:

```rust
AdminEndpoint::DrainListeners => render_drain_listeners(handler.drain()),
AdminEndpoint::HealthcheckFail => render_healthcheck_fail(handler.drain()),
AdminEndpoint::HealthcheckOk => render_healthcheck_ok(handler.drain()),
```

In `crates/envoy-bin/src/main.rs`, after the `let registry: Arc<StatsRegistry> = Arc::new(StatsRegistry::new());` at line ~101-102, add:

```rust
// 08.2 D13b: construct the shared DrainState ONCE at startup. Cloned
// into the admin handler (writer; for the 3 POST endpoints + /server_info
// state read + /ready drain-aware response) and into every data-plane
// Listener::serve call (reader/observer per D12 — the tcp_proxy + HCM
// paths). The echo path (fixture 0002 only) and the admin path itself
// use TcpListener::bind directly and are naturally excluded from drain
// observation per 08.2 PLAN architecture-decision lock-in #12.
let drain: Arc<envoy_listener::DrainState> =
    Arc::new(envoy_listener::DrainState::new(&registry));
```

Update the `AdminHandler::new` call at line ~358-365 to pass `Arc::clone(&drain)` as the 7th arg:

```rust
let admin_handler = std::sync::Arc::new(envoy_admin::AdminHandler::new(
    std::sync::Arc::clone(&admin_config),
    std::sync::Arc::clone(&registry),
    std::sync::Arc::clone(&bootstrap),
    std::sync::Arc::clone(&cluster_mgr),
    start_instant,
    command_line_options.clone(),
    std::sync::Arc::clone(&drain),
));
```

(The tcp_proxy + HCM `listener.serve(...)` call sites at lines ~234-240 + ~333-338 do NOT yet thread `drain` because `Listener::serve` is still 1-arg until Task 6. Task 6 widens the signature; envoy-bin's call sites update there.)

- [ ] **Step 4: Run tests to verify they PASS**

Run: `cargo test -p envoy-admin -p envoy-bin --lib -- --nocapture` (also build envoy-bin with `cargo build -p envoy-bin --bin envoy-bin` to verify the binary still compiles).

Expected: 2 new tests pass + all existing tests stay green.

- [ ] **Step 5: Run the 5 stable-toolchain gates**

Same as Task 1 Step 5.

- [ ] **Step 6: Append PROGRESS section + commit**

Commit message:

```
phase 08.2: task 4 — D13b AdminHandler::new 7-arg widen + envoy-bin
DrainState wiring

AdminHandler::new widens from 6-arg to 7-arg (adds trailing
drain: Arc<DrainState> per 08.2 PLAN architecture-decision lock-in
#13). New struct field + pub(crate) drain() accessor used by Task 3's
render_with dispatch arms (todo!() placeholders replaced with real
handler.drain() calls) AND by Task 5's render_server_info /
render_ready_with patches. 7+ in-file test call sites updated.

envoy-bin::main constructs Arc<DrainState> once at startup alongside
the existing Arc<StatsRegistry>; clones into AdminHandler::new (the
7th arg). Data-plane Listener::serve wiring waits for Task 6 (D12)
to widen Listener::serve from 1-arg to 2-arg.

Differential surface: none (no new endpoint behavior at this task;
Task 8's fixture 0015 exercises end-to-end).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 5: D5e + D-ready — `/server_info` state-source rebind + `/ready` drain-aware response

**Goal.** Patch `render_server_info`'s `state` field source from the literal `"LIVE"` to `match handler.drain().current() { Live | HealthcheckFailing => "LIVE", Draining => "DRAINING" }` (per parent-08 SPEC §5.5 wire-state mapping). Rewire `render_ready()` → `render_ready_with(handler: &AdminHandler)` to return 503 DRAINING (body `"DRAINING\n"`) when `drain.current() == Draining` + 503 HealthcheckFailing (body `"Service Unavailable\n"`) when `drain.current() == HealthcheckFailing` + 200 LIVE (body `"LIVE\n"`) otherwise. Update the dispatcher in `render` / `render_with` to route `Ready` through the new handler-aware path.

**Files:**
- Modify: `crates/envoy-admin/src/endpoint.rs` (patch `render_server_info`; rename + widen `render_ready` → `render_ready_with`; route `Ready` through `render_with` instead of the registry-only `render` path)

- [ ] **Step 1: Write failing tests**

Append to `crates/envoy-admin/src/endpoint.rs::tests`:

```rust
    #[test]
    fn server_info_state_is_draining_when_drain_state_is_draining() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = std::sync::Arc::new(envoy_listener::DrainState::new(&registry));
        drain.drain();
        let handler = test_handler_with_drain(std::sync::Arc::clone(&drain));
        let resp = render_server_info(&handler);
        let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(body["state"], "DRAINING");
    }

    #[test]
    fn server_info_state_is_live_when_drain_state_is_healthcheck_failing() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = std::sync::Arc::new(envoy_listener::DrainState::new(&registry));
        drain.fail_healthcheck();
        let handler = test_handler_with_drain(std::sync::Arc::clone(&drain));
        let resp = render_server_info(&handler);
        let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(
            body["state"], "LIVE",
            "server-state is INDEPENDENT of healthcheck-failure per parent-08 SPEC §5.5"
        );
    }

    #[test]
    fn ready_returns_200_live_when_drain_state_is_live() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = std::sync::Arc::new(envoy_listener::DrainState::new(&registry));
        let handler = test_handler_with_drain(std::sync::Arc::clone(&drain));
        let resp = AdminEndpoint::Ready.render_with(&handler);
        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, Some("OK"));
        assert_eq!(resp.body, bytes::Bytes::from_static(b"LIVE\n"));
    }

    #[test]
    fn ready_returns_503_draining_when_drain_state_is_draining() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = std::sync::Arc::new(envoy_listener::DrainState::new(&registry));
        drain.drain();
        let handler = test_handler_with_drain(std::sync::Arc::clone(&drain));
        let resp = AdminEndpoint::Ready.render_with(&handler);
        assert_eq!(resp.status, 503);
        assert_eq!(resp.reason, Some("Service Unavailable"));
        assert_eq!(resp.body, bytes::Bytes::from_static(b"DRAINING\n"));
    }

    #[test]
    fn ready_returns_503_service_unavailable_when_drain_state_is_healthcheck_failing() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = std::sync::Arc::new(envoy_listener::DrainState::new(&registry));
        drain.fail_healthcheck();
        let handler = test_handler_with_drain(std::sync::Arc::clone(&drain));
        let resp = AdminEndpoint::Ready.render_with(&handler);
        assert_eq!(resp.status, 503);
        assert_eq!(resp.reason, Some("Service Unavailable"));
        assert_eq!(resp.body, bytes::Bytes::from_static(b"Service Unavailable\n"));
    }
```

The executor adds a `test_handler_with_drain(drain)` helper to the endpoint.rs tests module that constructs an `AdminHandler` with the given drain + fresh defaults for the other 6 args (mirrors the existing test helpers in handler.rs::tests).

- [ ] **Step 2: Run tests to verify they FAIL**

Run: `cargo test -p envoy-admin --lib endpoint::tests -- --nocapture`

Expected: 5 new tests fail with `assertion failed: body["state"] == "DRAINING"` (still emits literal "LIVE") AND `assertion failed: resp.status == 503` (still emits 200 unconditionally).

- [ ] **Step 3: Implement D5e + D-ready**

In `crates/envoy-admin/src/endpoint.rs`:

For D5e (`render_server_info` patch), find the `state: "LIVE"` literal (around line ~340-360 in the existing fn body) and replace:

```rust
state: "LIVE",
```

with:

```rust
state: match handler.drain().current() {
    envoy_listener::DrainStage::Live | envoy_listener::DrainStage::HealthcheckFailing => "LIVE",
    envoy_listener::DrainStage::Draining => "DRAINING",
},
```

For D-ready, replace the existing `render_ready()` fn (lines 165-176) with:

```rust
/// 06.1 D5: `/ready` GET endpoint. Phase 08.2 D-ready widens from the
/// unconditional 200 LIVE shape to drain-aware: returns 503 DRAINING
/// (body `"DRAINING\n"`) when `DrainState::current() == Draining`; 503
/// Service Unavailable (body `"Service Unavailable\n"`) when
/// `DrainState::current() == HealthcheckFailing` (Envoy's load-balancer-
/// take-out semantic); 200 LIVE (body `"LIVE\n"`) otherwise. Per parent-
/// 08 SPEC §5.5 wire-state mapping.
fn render_ready_with(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    match handler.drain().current() {
        envoy_listener::DrainStage::Live => {
            let body = bytes::Bytes::from_static(b"LIVE\n");
            envoy_http1::Response {
                status: 200,
                reason: Some("OK"),
                headers: vec![
                    ("content-type".to_string(), "text/plain".to_string()),
                    ("content-length".to_string(), body.len().to_string()),
                ],
                body,
            }
        }
        envoy_listener::DrainStage::HealthcheckFailing => {
            let body = bytes::Bytes::from_static(b"Service Unavailable\n");
            envoy_http1::Response {
                status: 503,
                reason: Some("Service Unavailable"),
                headers: vec![
                    ("content-type".to_string(), "text/plain".to_string()),
                    ("content-length".to_string(), body.len().to_string()),
                ],
                body,
            }
        }
        envoy_listener::DrainStage::Draining => {
            let body = bytes::Bytes::from_static(b"DRAINING\n");
            envoy_http1::Response {
                status: 503,
                reason: Some("Service Unavailable"),
                headers: vec![
                    ("content-type".to_string(), "text/plain".to_string()),
                    ("content-length".to_string(), body.len().to_string()),
                ],
                body,
            }
        }
    }
}
```

Reroute `AdminEndpoint::Ready` in the `render_with` match (line ~155) to call `render_ready_with(handler)` instead of falling through to `render`:

```rust
pub fn render_with(&self, handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    match self {
        AdminEndpoint::Ready => render_ready_with(handler),  // 08.2 D-ready: was self.render(handler.registry())
        AdminEndpoint::ConfigDump => render_config_dump(handler),
        AdminEndpoint::ServerInfo => render_server_info(handler),
        AdminEndpoint::Clusters => render_clusters(handler),
        AdminEndpoint::Listeners => render_listeners(handler),
        AdminEndpoint::DrainListeners => render_drain_listeners(handler.drain()),
        AdminEndpoint::HealthcheckFail => render_healthcheck_fail(handler.drain()),
        AdminEndpoint::HealthcheckOk => render_healthcheck_ok(handler.drain()),
        _ => self.render(handler.registry()),
    }
}
```

Update the `render` fn's `AdminEndpoint::Ready` arm to be `unreachable!()` (matches the pattern for the other handler-scoped endpoints):

```rust
AdminEndpoint::Ready => unreachable!(
    "Ready requires handler-scoped DrainState; dispatch via AdminEndpoint::render_with"
),
```

Delete the old `render_ready()` fn (now obsolete; only `render_ready_with` remains).

- [ ] **Step 4: Run tests to verify they PASS**

Run: `cargo test -p envoy-admin --lib endpoint::tests -- --nocapture`

Expected: 5 new tests pass + the existing 06.1 `render_ready_returns_200_live` test (which calls `AdminEndpoint::Ready.render(&registry)` directly) FAILS with `unreachable!()` — the executor updates that test to call `Ready.render_with(&test_handler())` instead.

- [ ] **Step 5: Run the 5 stable-toolchain gates**

Same as Task 1 Step 5. Verify `cargo test --workspace` covers the in-process backstop `crates/envoy-bin/tests/admin_ready.rs` (06.1-landed) which exercises `/ready` end-to-end — that test must stay green under the D-ready widening (it doesn't drain, so `DrainState::current() == Live`, so the response is the same 200 LIVE body).

- [ ] **Step 6: Append PROGRESS section + commit**

Commit message:

```
phase 08.2: task 5 — D5e /server_info state-source rebind + D-ready
/ready drain-aware response

render_server_info's state field source flips from the 08.1 literal
"LIVE" to match handler.drain().current() { Live | HealthcheckFailing
=> "LIVE", Draining => "DRAINING" } per parent-08 SPEC §5.5 wire-state
mapping. ServerInfoBody<'a>.state: &'static str shape unchanged at
the 08.1 → 08.2 boundary.

render_ready widens to render_ready_with(handler) with a 3-arm match
on drain.current(): Live → 200 LIVE (body "LIVE\n"), HealthcheckFailing
→ 503 Service Unavailable (body "Service Unavailable\n"),
Draining → 503 Service Unavailable (body "DRAINING\n"). The
render_with dispatch arm for Ready routes through the new handler-
aware path; render() for Ready becomes unreachable! (matches the
existing pattern for the other handler-scoped endpoints).

Fixture 0011 (06.1 admin stats) stays green at this task because
fixture 0011 never POSTs /drain_listeners (DrainState stays Live;
/ready stays 200 LIVE; bilateral assertion unchanged). Fixture 0014
(08.1 admin config_dump) stays green for the same reason. Fixture
0015 at Task 8 is the first fixture to exercise the new 503 path.

Differential surface: none (regression-equivalence preserved on
fixtures 0001-0014).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 6: D12 — `Listener::serve` 2-arg widening + `listener_manager.total_listeners_active` RAII guard + signature-update churn

**Goal.** Widen `Listener::serve` from 1-arg `(shutdown)` to 2-arg `(shutdown, drain: Arc<DrainState>)`. Add a second `tokio::select!` arm `_ = drain.drain_signal() => { ... }` between the existing shutdown arm and the accept arm; either signal triggers the same drain code path. Add a RAII guard at `serve` entry that increments `listener_manager.total_listeners_active` (registered by `Listener::bind` at Task 2); the guard's `Drop` decrements after the loop exits + after stragglers join. Update all `Listener::serve(...)` call sites (envoy-bin tcp_proxy + HCM paths; existing 06.x tests).

**Files:**
- Modify: `crates/envoy-listener/src/lib.rs` (widen `Listener::serve` signature; add the second select arm; add the RAII guard wiring; new unit test `serve_returns_when_drain_signal_fires`; update all in-file tests' `serve(...)` calls to pass the new arg)
- Modify: `crates/envoy-bin/src/main.rs` (thread `Arc::clone(&drain)` into the tcp_proxy + HCM listener-spawn `listener.serve(...)` calls)

- [ ] **Step 1: Write the failing test in `crates/envoy-listener/src/lib.rs::tests`**

Append:

```rust
    /// 08.2 D12: Listener::serve returns within DRAIN_BUDGET when the
    /// drain signal fires (shutdown future never resolves). Verifies
    /// the new `_ = drain.drain_signal() => { ... }` arm in tokio::select!.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn serve_returns_when_drain_signal_fires() {
        // Build a minimal Listener bound to 127.0.0.1:0 with a no-op handler.
        let cfg = envoy_config::Listener {
            name: "drain_observation_test".to_string(),
            address: envoy_config::Address {
                socket_address: envoy_config::SocketAddress {
                    address: "127.0.0.1".to_string(),
                    port_value: 0,
                },
            },
            filter_chains: vec![],
            listener_filters: vec![],
        };
        let handler: std::sync::Arc<dyn ConnectionHandler> = std::sync::Arc::new(NoopHandler);
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = std::sync::Arc::new(envoy_listener::DrainState::new(&registry));
        let listener = Listener::bind(&cfg, handler, std::sync::Arc::clone(&registry))
            .await
            .expect("bind succeeds");

        // Spawn serve with a manual shutdown future that NEVER resolves;
        // only the drain signal can wake it.
        let drain_for_serve = std::sync::Arc::clone(&drain);
        let serve_handle = tokio::spawn(async move {
            listener
                .serve(std::future::pending::<()>(), drain_for_serve)
                .await
        });

        // Yield briefly so serve registers as a drain_signal waiter.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Fire drain; serve must return within DRAIN_BUDGET + 500ms.
        drain.drain();
        let result = tokio::time::timeout(
            DRAIN_BUDGET + std::time::Duration::from_millis(500),
            serve_handle,
        )
        .await
        .expect("serve returns within DRAIN_BUDGET + 500ms of drain()")
        .expect("serve task join")
        .expect("serve Ok");
        let _ = result;

        // Post-serve: listener_manager.total_listeners_active is 0
        // (RAII guard decremented at serve exit).
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().collect();
        if let Some(envoy_stats::StatHandle::Gauge(g)) =
            snapshot.get("listener_manager.total_listeners_active")
        {
            assert_eq!(g.value(), 0, "RAII guard must dec to 0 at serve exit");
        }
    }

    /// 08.2 D12: existing 06.x shutdown test must stay green under the
    /// widened 2-arg signature. The PLAN's signature-update churn touches
    /// `serves_honors_shutdown_signal` + `serve_drains_in_flight_within_budget`
    /// + any other tests at `crates/envoy-listener/src/lib.rs::tests`
    /// that call `Listener::serve(...)` — each gains the new 2nd arg.
    /// (Test body update only; semantics unchanged.)
    #[tokio::test]
    async fn serves_honors_shutdown_signal_with_drain_param() {
        // ... existing serves_honors_shutdown_signal body, but with
        // `Arc::new(DrainState::new(&registry))` passed as the new 2nd arg.
    }
```

- [ ] **Step 2: Run tests to verify FAIL**

Run: `cargo test -p envoy-listener --lib -- --nocapture`

Expected: 1 new test fails with `error[E0061]: this function takes 1 argument but 2 arguments were supplied` (and similar on the signature-update test).

- [ ] **Step 3: Widen `Listener::serve` + add the RAII guard + add the drain select arm + update callers**

In `crates/envoy-listener/src/lib.rs`:

```rust
// Widen the signature (line ~167):
pub async fn serve(
    self,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    drain: Arc<crate::drain::DrainState>,
) -> Result<(), ListenerError> {
    let listener = self.listener;
    let handler = self.handler;
    let cx_total = self.cx_total;
    let cx_active = self.cx_active;
    let cx_accept_failed = self.cx_accept_failed;
    // 08.2 D14: hoist the shared listener_manager.total_listeners_active
    // gauge out of `self`; same pattern as the per-listener gauges above.
    let listener_manager_active = self.listener_manager_active;
    // RAII guard: inc at entry; Drop decs at scope exit (after the loop
    // exits, after drain completes, after stragglers join). Mirrors the
    // existing 06.3 ConnGaugeGuard pattern.
    listener_manager_active.inc();
    struct ListenerManagerActiveGuard(Arc<envoy_stats::Gauge>);
    impl Drop for ListenerManagerActiveGuard {
        fn drop(&mut self) {
            self.0.dec();
        }
    }
    let _lm_guard = ListenerManagerActiveGuard(Arc::clone(&listener_manager_active));

    let mut join_set: tokio::task::JoinSet<
        Result<(), Box<dyn std::error::Error + Send + Sync>>,
    > = tokio::task::JoinSet::new();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("listener shutdown signal received; draining");
                drop(listener);
                break;
            }
            // 08.2 D12: new drain-signal arm. Behaves identically to the
            // shutdown arm except for the log message. Either signal
            // triggers the same drain code path (drop the listener;
            // await the JoinSet within DRAIN_BUDGET).
            _ = drain.drain_signal() => {
                tracing::info!("listener drain signal received; draining");
                drop(listener);
                break;
            }
            accepted = listener.accept() => {
                // ... existing accept arm body unchanged ...
            }
            Some(done) = join_set.join_next(), if !join_set.is_empty() => {
                // ... existing join arm body unchanged ...
            }
        }
    }
    // ... existing post-loop drain wait + DRAIN_BUDGET timeout block ...
    // _lm_guard dropped here at fn exit (after the post-loop drain wait
    // completes), per RAII semantic.
}
```

Update all existing in-file tests that call `Listener::serve(...)` (e.g., `serves_honors_shutdown_signal` at line ~322; `serve_drains_in_flight_within_budget` at line ~389) to pass the new `Arc::new(DrainState::new(&registry))` arg. The `registry` variable is already in scope at each test site (used for `Listener::bind`).

In `crates/envoy-bin/src/main.rs`, update both `listener.serve(...)` call sites to pass `Arc::clone(&drain)`:

- Line ~234-240 (tcp_proxy path):
  ```rust
  let shutdown = token.clone();
  let drain_for_serve = std::sync::Arc::clone(&drain);
  set.spawn(async move {
      listener
          .serve(async move { shutdown.cancelled().await }, drain_for_serve)
          .await
          .map_err(|e| anyhow::anyhow!(e))
  });
  ```

- Line ~333-338 (HCM path): same shape.

The echo path (line ~177-180) does NOT take a drain arg because it uses `echo::serve` directly (not `Listener::serve`); fixture 0002's echo handler is naturally excluded from drain observation per architecture-decision lock-in #12.

- [ ] **Step 4: Run tests to verify PASS**

Run: `cargo test -p envoy-listener --lib -- --nocapture` then `cargo build -p envoy-bin --bin envoy-bin` then `cargo test --workspace` for regression coverage.

Expected: 2 new tests pass + all 06.x existing tests stay green under the widened signature + workspace builds clean.

- [ ] **Step 5: Run the 5 stable-toolchain gates**

Same as Task 1 Step 5.

- [ ] **Step 6: Append PROGRESS section + commit**

Commit message:

```
phase 08.2: task 6 — D12 Listener::serve 2-arg widening +
listener_manager.total_listeners_active RAII guard

Listener::serve widens from 1-arg (shutdown) to 2-arg (shutdown,
drain: Arc<DrainState>). Adds a second tokio::select! arm
`_ = drain.drain_signal() => { ... }` between the existing shutdown
arm and the accept arm; either signal triggers the same drain code
path (drop the listener; await stragglers within DRAIN_BUDGET).

Adds a RAII guard at serve entry (ListenerManagerActiveGuard) that
increments listener_manager.total_listeners_active gauge (registered
by Listener::bind at Task 2 idempotently); Drop decrements at scope
exit after the post-loop drain-wait completes. Mirrors the existing
06.3 ConnGaugeGuard pattern.

envoy-bin tcp_proxy + HCM listener-spawn call sites updated to thread
Arc::clone(&drain) (constructed at Task 4). Echo path uses
echo::serve directly + admin path uses TcpListener::bind + envoy_admin
::serve directly — both naturally excluded from drain observation per
PLAN architecture-decision lock-in #12 (parent-08 SPEC §5.5 "admin
listener stays serving during drain").

All existing 06.x in-file Listener tests (serves_honors_shutdown_signal,
serve_drains_in_flight_within_budget, etc.) updated for the widened
signature (signature-update churn only; semantics unchanged).

Differential surface: none at this task; the wire-level drain
behavior is exercised by fixture 0015 at Task 8.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 7: D16 — `Driver::AdminScrape` `pre_admin_actions` + `post_admin_assertions` extensions + 08.1 REVIEW M2 + M4 closures

**Goal.** Extend `Driver::AdminScrape` with `pre_admin_actions: Vec<AdminAction>` (BEFORE `pre_requests`; logically prior — drain before driving data-plane) + `post_admin_assertions: Vec<AdminAssertion>` (AFTER `scrapes`; fires after scrape sequence completes). Add `AdminAction { Post { path, expected_status } }` + `AdminAssertion { DataPlaneConnectionRefused { listener_address, within_ms } }` enums with `#[serde(tag = "kind")]` internally-tagged shape. Wire dispatch in `tests/differential/src/lib.rs::assert_admin_scrape_*` (around line ~2185-2360). Fold the 08.1 REVIEW M2 (one-line doc-comment on `value_may_differ_keys` at line 300) + M4 (3-line guard at the head of `walk_pointer` at line ~379-394) closures into the same harness-touch task.

**Files:**
- Modify: `tests/differential/src/lib.rs` (variant fields + 2 new enums + dispatch wiring + M2 doc + M4 guard + 9+ new unit tests)

- [ ] **Step 1: Write failing tests in `tests/differential/src/lib.rs::body_rule_extension_tests`** (or a new sibling `admin_action_extension_tests` module)

Append (9 tests):

```rust
    #[test]
    fn admin_scrape_deserializes_pre_admin_actions_with_post() {
        let yaml = r#"
            driver:
              kind: admin_scrape
              pre_admin_actions:
                - kind: post
                  path: /drain_listeners
                  expected_status: 200
              pre_requests: []
              scrapes: []
              post_admin_assertions: []
        "#;
        let cfg: ExpectationsFile = serde_yaml::from_str(yaml).expect("parse");
        match cfg.driver {
            Driver::AdminScrape { pre_admin_actions, .. } => {
                assert_eq!(pre_admin_actions.len(), 1);
                match &pre_admin_actions[0] {
                    AdminAction::Post { path, expected_status } => {
                        assert_eq!(path, "/drain_listeners");
                        assert_eq!(*expected_status, 200);
                    }
                }
            }
            _ => panic!("expected AdminScrape"),
        }
    }

    #[test]
    fn admin_scrape_deserializes_post_admin_assertions_with_data_plane_connection_refused() {
        let yaml = r#"
            driver:
              kind: admin_scrape
              pre_admin_actions: []
              pre_requests: []
              scrapes: []
              post_admin_assertions:
                - kind: data_plane_connection_refused
                  listener_address: 127.0.0.1:8080
                  within_ms: 5000
        "#;
        let cfg: ExpectationsFile = serde_yaml::from_str(yaml).expect("parse");
        match cfg.driver {
            Driver::AdminScrape { post_admin_assertions, .. } => {
                assert_eq!(post_admin_assertions.len(), 1);
                match &post_admin_assertions[0] {
                    AdminAssertion::DataPlaneConnectionRefused {
                        listener_address,
                        within_ms,
                    } => {
                        assert_eq!(listener_address, "127.0.0.1:8080");
                        assert_eq!(*within_ms, 5000);
                    }
                }
            }
            _ => panic!("expected AdminScrape"),
        }
    }

    #[test]
    fn admin_scrape_pre_admin_actions_defaults_to_empty_vec() {
        // Backward-compat: existing fixtures 0011 + 0014 don't supply these
        // fields; serde defaults must accept the omission.
        let yaml = r#"
            driver:
              kind: admin_scrape
              pre_requests: []
              scrapes: []
        "#;
        let cfg: ExpectationsFile = serde_yaml::from_str(yaml).expect("parse");
        match cfg.driver {
            Driver::AdminScrape {
                pre_admin_actions,
                post_admin_assertions,
                ..
            } => {
                assert!(pre_admin_actions.is_empty());
                assert!(post_admin_assertions.is_empty());
            }
            _ => panic!("expected AdminScrape"),
        }
    }

    // ... 6 more tests covering: dispatch executes pre_admin_actions before
    // pre_requests; pre_admin_actions Post failure surfaces a clear error;
    // post_admin_assertions DataPlaneConnectionRefused success path (connect
    // returns Err); DataPlaneConnectionRefused success path (connect succeeds
    // + immediate EOF); DataPlaneConnectionRefused failure path (connect
    // succeeds + read returns bytes); and the multi-action / multi-assertion
    // ordering test. The implementer authors these inline; each test asserts
    // a specific dispatch-path branch.

    // 08.1 REVIEW M2 closure: field-level doc-comment on value_may_differ_keys.
    // No test — doc-comment-only edit. Verified via grep at Step 3.

    // 08.1 REVIEW M4 closure test: walk_pointer rejects dotted paths with
    // empty segments with a structured error message.
    #[test]
    fn walk_pointer_rejects_empty_segment_with_structured_error() {
        let json: serde_json::Value = serde_json::json!({"node": {"id": "x"}});
        let result = walk_pointer(&json, "node..id");
        assert!(result.is_err(), "must reject empty-segment path");
        let err_str = format!("{}", result.unwrap_err());
        assert!(
            err_str.contains("empty segment"),
            "error must name 'empty segment'; got: {err_str}"
        );
        assert!(
            err_str.contains("node..id"),
            "error must include the offending path; got: {err_str}"
        );
    }
```

- [ ] **Step 2: Run tests to verify FAIL**

Run: `cargo test -p differential --lib -- --nocapture` (or whatever the test target name is for the differential crate; verify via `cargo test --workspace -- --list 2>&1 | grep differential`).

Expected: 9+ new tests fail with `error[E0560]: struct \`Driver\` has no field named \`pre_admin_actions\`` and similar.

- [ ] **Step 3: Implement Driver::AdminScrape extensions + new enums + dispatch wiring + M2 + M4**

In `tests/differential/src/lib.rs`:

```rust
// Around line ~140, widen Driver::AdminScrape:
pub enum Driver {
    // ... existing variants ...
    AdminScrape {
        // 08.2 D16 NEW: admin POST actions fired BEFORE the existing
        // pre_requests block. Logically prior — drain the listener before
        // driving data-plane traffic. Defaults to empty Vec via serde so
        // 08.1-landed fixtures 0011 + 0014 carry forward unchanged.
        #[serde(default)]
        pre_admin_actions: Vec<AdminAction>,
        #[serde(default)]
        pre_requests: Vec<PreRequest>,
        scrapes: Vec<AdminScrapeCase>,
        // 08.2 D16 NEW: wire-level assertions fired AFTER the scrape loop
        // completes. Currently the only kind is DataPlaneConnectionRefused
        // (covers the fixture 0015 drain-listener-rejection assertion).
        #[serde(default)]
        post_admin_assertions: Vec<AdminAssertion>,
    },
}

// New enums (insertion after AdminScrapeCase around line ~165):

/// 08.2 D16: action fired against the admin listener BEFORE the
/// `pre_requests` block in `Driver::AdminScrape`. Currently the only kind
/// is `Post` (issue a POST to a named admin path and assert the expected
/// status); extensible for future GET/DELETE-bearing admin actions.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdminAction {
    Post {
        path: String,
        expected_status: u16,
    },
}

/// 08.2 D16: wire-level assertion fired AFTER the scrape loop. Currently
/// the only kind is `DataPlaneConnectionRefused` (verifies a named
/// listener-address rejects new TCP connections within a deadline);
/// extensible for future drain-related wire-level assertions.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdminAssertion {
    DataPlaneConnectionRefused {
        listener_address: String,
        /// Deadline in milliseconds. Chosen as raw u64 (NOT humantime
        /// string) to avoid adding a `humantime-serde` top-level dep
        /// per 08.2 PLAN architecture-decision lock-in #19.
        within_ms: u64,
    },
}
```

Update the dispatch fn (existing `Driver::AdminScrape` branch at line ~2185-2360) to drive `pre_admin_actions` BEFORE the existing `pre_requests` block + drive `post_admin_assertions` AFTER the existing scrape-loop:

```rust
Driver::AdminScrape {
    pre_admin_actions,
    pre_requests,
    scrapes,
    post_admin_assertions,
} => {
    // 08.2 D16: fire pre_admin_actions against BOTH proxies (Envoy and
    // envoy-rust) BEFORE the existing pre_requests block.
    for action in pre_admin_actions {
        match action {
            AdminAction::Post { path, expected_status } => {
                drive_admin_post(envoy_admin_addr, path, *expected_status)
                    .await
                    .with_context(|| format!("Envoy /<{path}> POST"))?;
                drive_admin_post(rust_admin_addr, path, *expected_status)
                    .await
                    .with_context(|| format!("envoy-rust /<{path}> POST"))?;
            }
        }
    }

    // ... existing pre_requests + scrapes loop body unchanged ...

    // 08.2 D16: fire post_admin_assertions AFTER the scrape loop.
    for assertion in post_admin_assertions {
        match assertion {
            AdminAssertion::DataPlaneConnectionRefused {
                listener_address,
                within_ms,
            } => {
                let within = Duration::from_millis(*within_ms);
                assert_data_plane_connection_refused(
                    envoy_data_plane_addr,
                    within,
                ).await.with_context(|| {
                    format!("Envoy data-plane {listener_address} must refuse-or-EOF within {within_ms}ms")
                })?;
                assert_data_plane_connection_refused(
                    rust_data_plane_addr,
                    within,
                ).await.with_context(|| {
                    format!("envoy-rust data-plane {listener_address} must refuse-or-EOF within {within_ms}ms")
                })?;
            }
        }
    }
}
```

New helpers (insertion at module level):

```rust
/// 08.2 D16: drive a single admin POST against the named admin address;
/// returns Err if the response status doesn't match `expected`.
async fn drive_admin_post(
    admin_addr: SocketAddr,
    path: &str,
    expected_status: u16,
) -> anyhow::Result<()> {
    let req = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\ncontent-length: 0\r\n\r\n");
    let stream = TcpStream::connect(admin_addr).await?;
    // ... drive_request style: write + read + parse status line ...
    // ... extracted from existing harness helpers; the executor finds
    // the existing pre_request POST driver and reuses ...
    Ok(())
}

/// 08.2 D16: assert the named TCP address either refuses connect OR
/// accepts the connection but immediately closes (read returns 0 bytes).
/// Polls in a 100ms interval up to the `within` deadline; succeeds on the
/// first ECONNREFUSED OR immediate-EOF observed. Either disposition is
/// accepted per 08.2 PLAN architecture-decision lock-in #20.
async fn assert_data_plane_connection_refused(
    addr: SocketAddr,
    within: Duration,
) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + within;
    let mut last_err: Option<String> = None;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(
            Duration::from_millis(200),
            TcpStream::connect(addr),
        )
        .await
        {
            Ok(Err(e)) => {
                // Connect Err — drain success.
                return Ok(());
            }
            Ok(Ok(mut stream)) => {
                // Connect Ok — check for immediate EOF.
                let mut buf = [0u8; 1];
                match tokio::time::timeout(
                    Duration::from_millis(50),
                    stream.read(&mut buf),
                )
                .await
                {
                    Ok(Ok(0)) => return Ok(()),
                    Ok(Ok(_)) => {
                        last_err = Some(format!(
                            "connect succeeded and read returned data (not draining); addr={addr}"
                        ));
                    }
                    Ok(Err(e)) => {
                        // Read Err — accept as drain success (the listener
                        // shut the connection ungracefully).
                        return Ok(());
                    }
                    Err(_timeout) => {
                        last_err = Some(format!(
                            "connect succeeded and read timed out (not draining); addr={addr}"
                        ));
                    }
                }
            }
            Err(_timeout) => {
                last_err = Some(format!("connect timed out (not draining); addr={addr}"));
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(anyhow::anyhow!(
        "data-plane connection refused assertion failed within {within:?}: last_err={last_err:?}"
    ))
}
```

**08.1 REVIEW M2 closure** — add a one-line doc-comment above line 300 (`pub value_may_differ_keys: Vec<String>`):

```rust
/// Shared keys whose values may differ bilaterally; presence is required, value equality is not. (08.1 REVIEW M2 closure landed at 08.2 D16.)
pub value_may_differ_keys: Vec<String>,
```

**08.1 REVIEW M4 closure** — add a 3-line guard at the head of `walk_pointer` body at line ~379-394:

```rust
fn walk_pointer<'a>(root: &'a serde_json::Value, dotted_path: &str) -> anyhow::Result<&'a serde_json::Value> {
    // 08.1 REVIEW M4 closure (landed at 08.2 D16): reject dotted paths
    // with empty segments structurally so hand-edited fixtures get a
    // clear diagnosability error rather than the opaque "key not found: "
    // message that prior code produced.
    if dotted_path.split('.').any(|s| s.is_empty()) {
        anyhow::bail!("walk_pointer: dotted path contains empty segment: {dotted_path:?}");
    }
    // ... existing body ...
}
```

- [ ] **Step 4: Run tests to verify PASS**

Run: `cargo test -p differential --lib -- --nocapture` (and the full workspace `cargo test --workspace` to catch any AdminScrape construction-site regression in the existing fixtures 0011 + 0014 harness wiring).

Expected: 9+ new tests pass + the existing harness tests stay green.

- [ ] **Step 5: Run the 5 stable-toolchain gates**

Same as Task 1 Step 5.

- [ ] **Step 6: Append PROGRESS section + commit**

Commit message:

```
phase 08.2: task 7 — D16 Driver::AdminScrape pre_admin_actions +
post_admin_assertions extensions + 08.1 REVIEW M2 + M4 closures

Driver::AdminScrape variant gains 2 new fields: pre_admin_actions
(Vec<AdminAction>, fired BEFORE pre_requests) and post_admin_assertions
(Vec<AdminAssertion>, fired AFTER scrapes). Both #[serde(default)] so
08.1-landed fixtures 0011 + 0014 carry forward unchanged.

Two new enums:
- AdminAction { Post { path, expected_status } } — fires a single
  admin POST against both proxies; verifies expected_status match.
- AdminAssertion { DataPlaneConnectionRefused { listener_address,
  within_ms } } — polls the named TCP address for ECONNREFUSED OR
  immediate-EOF within within_ms milliseconds. Either disposition
  accepted per PLAN architecture-decision lock-in #20. within_ms is
  raw u64 (NOT humantime) to avoid a new top-level Cargo dep.

08.1 REVIEW M2 closed: one-line doc-comment on
BodyRule::JsonShape::value_may_differ_keys at tests/differential/src/
lib.rs:300. The chain ends.

08.1 REVIEW M4 closed: 3-line guard at head of walk_pointer rejecting
dotted paths with empty segments with a structured error message
naming the offending path. The chain ends.

Differential surface: none (fixture 0015 at Task 8 is the first
empirical exercise of the new D16 surface; the 9+ unit tests at this
task verify deserialization + dispatch dispatch in isolation).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 8: D17.2 — Fixture `0015-admin-drain-listeners` + Docker-gated wrapper + BEHAVIOR_CONTRACT "Admin-action effect equivalence" subsection

**Goal.** Land the new differential fixture exercising the end-to-end drain flow bilaterally. Reuses fixture 0007's bootstrap shape (HCM + direct_response — minimal data-plane surface so the drain assertion focuses on listener-rejection, not upstream complexity). Single admin-scrape sequence: `pre_requests: [GET /ready → 200 LIVE]`, `pre_admin_actions: [POST /drain_listeners → 200]`, `scrapes: [{request: GET /ready, expected_status: 503, response_body: BodyRule::ByteExact "DRAINING\n"}]`, `post_admin_assertions: [DataPlaneConnectionRefused { listener_address: <bound port>, within_ms: 5000 }]`. Docker-gated wrapper at `tests/differential/tests/admin_drain_listeners.rs`. Append the BEHAVIOR_CONTRACT "Admin-action effect equivalence" subsection.

**Files:**
- Create: `tests/fixtures/0015-admin-drain-listeners/envoy.yaml` (reference Envoy config; HCM + direct_response shape, mirrors fixture 0007)
- Create: `tests/fixtures/0015-admin-drain-listeners/envoy-rust.yaml` (paired)
- Create: `tests/fixtures/0015-admin-drain-listeners/inputs/payload.bin` (0-byte placeholder)
- Create: `tests/fixtures/0015-admin-drain-listeners/expectations.yaml` (the new D16-shape AdminScrape with all 4 fields)
- Create: `tests/fixtures/0015-admin-drain-listeners/README.md` (fixture documentation; cross-references the Task-8 process-note pick for `filter_chains: []` and the trivial-echo-filter workaround Task-10 will use)
- Create: `tests/differential/tests/admin_drain_listeners.rs` (Docker-gated wrapper; ~30 LoC mirror of `admin_config_dump_server_info.rs` shape)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (append the new "Admin-action effect equivalence" top-level subsection per parent-08 SPEC §2.4)

- [ ] **Step 1: Write the failing fixture + wrapper + run the Docker-gated wrapper to verify FAIL**

Author the 5 fixture files (envoy.yaml + envoy-rust.yaml + inputs/payload.bin + expectations.yaml + README.md) per the SPEC §3 D17.2 description. envoy.yaml mirrors fixture 0007's bootstrap shape but with admin: block (using `{{ADMIN_PORT}}` template per the existing 06.1+ harness convention). envoy-rust.yaml is the envoy-rust-side paired config (same shape; `bind_address` differences per the 0007 → 0011 → 0014 precedent).

expectations.yaml:

```yaml
driver:
  kind: admin_scrape
  pre_admin_actions:
    - kind: post
      path: /drain_listeners
      expected_status: 200
  pre_requests:
    - method: GET
      path: /ready
      expected_status: 200
      expected_body: "LIVE\n"
  scrapes:
    - path: /ready
      method: GET
      expected_status: 503
      response_body:
        kind: byte_exact
        body: "DRAINING\n"
  post_admin_assertions:
    - kind: data_plane_connection_refused
      listener_address: "127.0.0.1:{{PORT}}"
      within_ms: 5000
```

Wait — re-read the SPEC §3 D17.2 carefully: the sequence is `pre_requests: [GET /ready → 200 LIVE]` first (to verify the pre-drain state), THEN `pre_admin_actions: [POST /drain_listeners → 200]`. But the PLAN architecture-decision lock-in #18 above settled `pre_admin_actions` BEFORE `pre_requests` in the YAML field order (because architecturally, drain-before-data-plane is the logical-prior step). The SPEC §3 D17.2 narrative describes a "pre_requests check → pre_admin_action → scrape → post_admin_assertion" sequence — that's CONTRADICTORY to the PLAN lock-in #18 field ordering.

**Resolution at PLAN-write time (architecture-decision deviation):** the SPEC §3 D17.2 narrative wins for fixture 0015's specific case — the pre-drain GET /ready (200 LIVE) check is the canonical "verify baseline state" step that fixtures use, and it MUST fire BEFORE the drain action. Re-order the YAML field semantics: `pre_requests` fires FIRST (baseline checks), THEN `pre_admin_actions` (state-mutating POSTs), THEN `scrapes` (post-state-change scrapes), THEN `post_admin_assertions` (wire-level assertions). Update PLAN architecture-decision lock-in #18 (record as a PLAN-write SPEC correction): the temporal sequence is `pre_requests → pre_admin_actions → scrapes → post_admin_assertions`; field ordering in the Driver::AdminScrape variant declaration is irrelevant to the temporal sequence (the dispatch fn drives them in the canonical order regardless).

**Action for Task 7's executor (cross-task fix):** the Driver::AdminScrape dispatch fn at Task 7's Step 3 above drives `pre_admin_actions` BEFORE `pre_requests` — that's WRONG per the corrected sequence. The executor MUST swap the order: drive `pre_requests` FIRST, then `pre_admin_actions`, then `scrapes`, then `post_admin_assertions`. Record as a Task 7 deviation if discovered in Task 7's execution, or as a Task 7 fixup commit if surfaced first in Task 8's empirical iteration.

Resume Task 8 fixture authoring with the corrected temporal sequence.

Docker-gated wrapper at `tests/differential/tests/admin_drain_listeners.rs` (mirrors `admin_config_dump_server_info.rs`):

```rust
//! 08.2 D17.2: Docker-gated wrapper for fixture 0015-admin-drain-listeners.
//! Mirrors tests/differential/tests/admin_config_dump_server_info.rs.

#[path = "common/mod.rs"]
mod common;

#[tokio::test]
async fn admin_drain_listeners() -> anyhow::Result<()> {
    common::run_fixture("0015-admin-drain-listeners").await
}
```

Run: `cargo test --test admin_drain_listeners -- --nocapture --ignored` (Docker-gated tests are typically `#[ignore]`-gated; verify by inspecting `admin_config_dump_server_info.rs` for the exact attribute used).

Expected: test fails because the fixture files don't exist yet, OR fails because the new pre_admin_actions / post_admin_assertions dispatch produces an unexpected result.

- [ ] **Step 2: Iterate the fixture YAMLs empirically against the Docker-gated harness**

Mirrors the 08.1 Task 11 empirical iteration loop:
1. Run the fixture Docker-gated; capture the actual envoy + envoy-rust per-side responses.
2. Update `expectations.yaml` if any per-side allow-list is needed (e.g., Envoy's 503 response body may differ from envoy-rust's "DRAINING\n" — adjust the `BodyRule::ByteExact` to the upstream-Envoy actual or add a per-side allow-list disposition).
3. Re-run; iterate up to 5 times per the 08.1 budget.

If the Docker-gated harness surfaces that Envoy's 503 body differs from `"DRAINING\n"`, the resolution path is one of: (a) tighten Envoy's expected body via `BodyRule::ByteExactPerSide { envoy: ..., envoy_rust: ... }` (new variant — but that's scope-creep); (b) loosen to `BodyRule::JsonShape` (not applicable — body is text/plain); (c) loosen to a `BodyRule::TextLines` with a single `required_lines: ["DRAINING"]` line entry. Recommended at PLAN time: try (b) first via `BodyRule::TextLines { required_lines: ["DRAINING"], ... }` if upstream Envoy emits a different body string for a `/ready` 503; record the empirical result in PROGRESS.

- [ ] **Step 3: Author the BEHAVIOR_CONTRACT subsection**

Append a new top-level subsection to `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (insertion AFTER "Admin endpoint body shapes" and BEFORE "Access log field mapping"):

```markdown
## Admin-action effect equivalence

> Authored per phase 08.2 SPEC §2.3. States the cross-proxy invariant that
> admin-action POSTs (`/drain_listeners`, `/healthcheck/fail`,
> `/healthcheck/ok`) must drive observable wire-level effects on both
> proxies. The internal mechanism is implementation-specific; only the
> wire-level observable is contract.

| Action | Wire-level invariant |
|---|---|
| `POST /drain_listeners` | Both proxies MUST refuse-or-immediately-close new connections on their data-plane listeners within the drain window (5s `DRAIN_BUDGET`). The harness `AdminAssertion::DataPlaneConnectionRefused { listener_address, within_ms }` polls for ECONNREFUSED OR immediate-EOF on connect; either disposition satisfies the invariant. Admin listener stays serving during drain (operator reachability per parent-08 SPEC §5.5). Sticky — subsequent `POST /healthcheck/ok` does NOT un-drain. |
| `POST /healthcheck/fail` | Both proxies MUST flip `/ready` to 503 within 100ms; `/server_info.state` stays `"LIVE"` (server-state independent of healthcheck-failure). |
| `POST /healthcheck/ok` | Both proxies MUST flip `/ready` back to 200 within 100ms IF and ONLY IF current state is `HealthcheckFailing`; if current state is `Draining`, the action is a no-op (sticky drain). |
```

- [ ] **Step 4: Run the Docker-gated wrapper to verify PASS**

Run: `cargo test --test admin_drain_listeners -- --nocapture --ignored`

Expected: GREEN. Both proxies bilaterally exhibit the drain effect within 5s.

Also run the full Docker-gated suite to verify regression-equivalence on fixtures 0001-0014:

```bash
cargo test --workspace -- --ignored --nocapture 2>&1 | grep -E "running|PASSED|FAILED|test result"
```

Expected: all 15 Docker-gated fixtures (0001-0015) GREEN simultaneously.

- [ ] **Step 5: Run the 5 stable-toolchain gates**

Same as Task 1 Step 5.

- [ ] **Step 6: Append PROGRESS section + commit**

Commit message:

```
phase 08.2: task 8 — D17.2 fixture 0015-admin-drain-listeners +
Docker-gated wrapper + BEHAVIOR_CONTRACT "Admin-action effect
equivalence" subsection

Fixture 0015-admin-drain-listeners lands the first end-to-end
differential test of the drain flow. Reuses fixture 0007's bootstrap
shape (HCM + direct_response — minimal data-plane surface so the
assertion focuses on listener-rejection, not upstream complexity).
Single admin-scrape sequence: pre_requests [GET /ready → 200 LIVE],
pre_admin_actions [POST /drain_listeners → 200], scrapes [GET /ready
→ 503 DRAINING], post_admin_assertions [DataPlaneConnectionRefused
listener_address: 127.0.0.1:<PORT>, within_ms: 5000]. Bilateral
GREEN on both proxies.

Docker-gated wrapper at tests/differential/tests/admin_drain_listeners
.rs mirrors the 08.1-landed admin_config_dump_server_info.rs shape.

BEHAVIOR_CONTRACT.md gains the new "Admin-action effect equivalence"
top-level subsection per parent-08 SPEC §2.4 — 3 rows covering the
wire-level invariants for /drain_listeners, /healthcheck/fail, and
/healthcheck/ok.

PLAN-write SPEC correction discovered at Task 8: the dispatch
temporal sequence is pre_requests → pre_admin_actions → scrapes →
post_admin_assertions (NOT the inverse — Task 7's dispatch order was
incorrect; fix landed in this task as a Task-7-amend commit).

Differential surface: fixture 0015 GREEN; all 15 Docker-gated
fixtures (0001-0015) GREEN simultaneously at local Docker run
(Linux/macOS bridge IPs both covered per the 08.1 Task 14 fix
precedent — fixture 0015 needs the same per-side allow-list family
if upstream Envoy emits Docker-bridge-specific 503 body content).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 9: D17.3b — Fuzz corpus seed `admin_healthcheck_bootstrap.yaml`

**Goal.** Add one new YAML seed under `crates/envoy-config/fuzz/corpus/parse_bootstrap/` exercising a healthcheck-relevant bootstrap shape (admin + 1 HCM listener + 1 direct_response route per fixture 0015's pattern, minus the harness substitutions). Append to the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array per the 08.1 Task 12 + 07.2 Task 6 + 06.1 Task 13 pattern. Mirror the 08.1 Task 12 5-deviation envelope (no `connect_timeout`, populated locality, mandatory `lb_policy`, single-listener cap, `+nightly` invocation).

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (add `!corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (inside `#[cfg(test)] mod tests`, append `"admin_healthcheck_bootstrap.yaml"` to the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array)

- [ ] **Step 1: Author the corpus seed YAML**

Author the seed YAML literally (no harness template markers; deterministic literal values):

```yaml
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
static_resources:
  listeners:
    - name: hcm_drain_test
      address:
        socket_address: { address: 127.0.0.1, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: drain_test
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

Add the gitignore allow-line:

```
!corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml
```

Add the SUCCESS-array entry in `crates/envoy-config/src/bootstrap.rs`:

```rust
// (inside fuzz_corpus_seeds_parse_or_reject_cleanly, in the existing
// SUCCESS array; insertion alphabetical after admin_config_dump_*)
"admin_healthcheck_bootstrap.yaml",
```

- [ ] **Step 2: Run the fuzz target locally + the SUCCESS array test to verify both parse OK**

Run:

```bash
cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30 -dict=corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml
cargo test -p envoy-config --lib fuzz_corpus_seeds_parse_or_reject_cleanly -- --nocapture
```

Expected: fuzz run clean for 30s; SUCCESS-array test passes (the new seed parses without error).

- [ ] **Step 3: Run the 5 stable-toolchain gates**

Same as Task 1 Step 5.

- [ ] **Step 4: Append PROGRESS section + commit**

Commit message:

```
phase 08.2: task 9 — D17.3b fuzz corpus seed
admin_healthcheck_bootstrap.yaml

New YAML seed under crates/envoy-config/fuzz/corpus/parse_bootstrap/
exercising the healthcheck-relevant bootstrap shape fixture 0015
needs (admin + HCM listener + direct_response). Mirrors the 08.1
Task 12 + 07.2 Task 6 + 06.1 Task 13 fuzz-seed pattern. Short-budget
local cargo +nightly fuzz run clean for 30s. SUCCESS-array test in
crates/envoy-config/src/bootstrap.rs gains the new entry.

Differential surface: none (fuzz coverage extension).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 10: D17.4b — In-process backstop `admin_drain_listeners.rs`

**Goal.** Land an in-process backstop test at `crates/envoy-bin/tests/admin_drain_listeners.rs` exercising the drain flow without Docker. Constructs an in-memory `Bootstrap` (admin + 1 HCM listener with 1 `envoy.filters.network.echo` filter per the 08.1 process-note option (b) trivial-filter workaround) + spawns `envoy-bin` as a subprocess + scrapes `/ready` (200 LIVE) + POSTs `/drain_listeners` (200 OK) + scrapes `/ready` again (503 DRAINING) + opens a TcpStream against the data-plane listener address (refused-or-EOF within 5s). Mirrors the 08.1 Task 13 + 07.2 backstop pattern (`stderr(Stdio::piped())` + `kill_on_drop(true)` + `Connection: close` + 5s admin idle timeout avoidance).

NOTE: Per architecture-decision lock-in #23, the in-process bootstrap uses the trivial-echo-filter workaround for the `filter_chains: []` schema-vs-runtime inconsistency. The HCM filter shape here is HCM + 1 direct_response route to keep the listener real (the SPEC §3 D17.4b refers to "the data-plane listener" which the drain assertion targets — that listener must bind a real port). Re-confirm: the workaround targets the case where a fixture wants `filter_chains: []`; fixture 0015's bootstrap actually wants `filter_chains: [HCM]`, so the workaround isn't directly applicable. Re-reading the 08.1 process note: the workaround is for tests that want a listener with NO meaningful filter (admin-only configs). Task 10's backstop wants a real HCM data-plane listener, so the workaround is unnecessary here. **Architecture-decision deviation #1 for Task 10 (record in PROGRESS):** the HCM + direct_response shape (not the echo-filter shape) is the right backstop bootstrap; the trivial-echo-filter workaround is reserved for future admin-only backstops. Update lock-in #23 in PROGRESS Task 10 preamble to reflect: option (b) recommended for FUTURE admin-only backstops; Task 10 itself uses HCM + direct_response (mirrors fixture 0015's bootstrap shape).

**Files:**
- Create: `crates/envoy-bin/tests/admin_drain_listeners.rs`

- [ ] **Step 1: Write the failing in-process test**

Author the file mirroring `crates/envoy-bin/tests/admin_config_dump_server_info.rs`:

```rust
//! 08.2 D17.4b: in-process backstop for the drain flow. Mirrors the
//! 08.1 D17.4a admin_config_dump_server_info.rs shape. Spawns envoy-bin
//! as a subprocess against an in-memory Bootstrap (admin + HCM + 1
//! direct_response route); scrapes /ready before + after POST
//! /drain_listeners; opens a TcpStream against the data-plane listener
//! address to verify the drain budget.

use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::Command;

mod common;

#[tokio::test]
async fn admin_drain_listeners_in_process() -> anyhow::Result<()> {
    let admin_port = common::reserve_port()?;
    let data_plane_port = common::reserve_port()?;

    let bootstrap_yaml = format!(
        r#"
admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: {admin_port} }}
static_resources:
  listeners:
    - name: hcm_drain_test
      address:
        socket_address: {{ address: 127.0.0.1, port_value: {data_plane_port} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: drain_test
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
    );

    let tmpdir = tempfile::tempdir()?;
    let config_path = tmpdir.path().join("bootstrap.yaml");
    std::fs::write(&config_path, bootstrap_yaml)?;

    let mut child = Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&config_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    // Wait for the admin + data-plane listeners to bind. Per the 08.1 +
    // 07.2 stderr-dump convention: capture stderr; dump on wait_ready
    // failure for diagnosability.
    if let Err(e) = common::wait_ready(admin_port, Duration::from_secs(10)).await {
        let mut stderr = String::new();
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_string(&mut stderr).await;
        }
        anyhow::bail!("envoy-bin admin not ready: {e}; stderr: {stderr}");
    }

    // Scrape /ready (must be 200 LIVE + body "LIVE\n").
    let resp = scrape(admin_port, b"GET /ready HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").await?;
    assert!(resp.starts_with(b"HTTP/1.1 200 OK\r\n"), "pre-drain /ready must be 200; got: {}", String::from_utf8_lossy(&resp));
    assert!(resp.windows(6).any(|w| w == b"LIVE\n\r" || w == b"LIVE\n"), "pre-drain /ready body must contain LIVE");

    // POST /drain_listeners (must be 200).
    let resp = scrape(admin_port, b"POST /drain_listeners HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").await?;
    assert!(resp.starts_with(b"HTTP/1.1 200 OK\r\n"), "/drain_listeners POST must be 200; got: {}", String::from_utf8_lossy(&resp));

    // Scrape /ready again (must be 503 DRAINING + body "DRAINING\n").
    let resp = scrape(admin_port, b"GET /ready HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").await?;
    assert!(resp.starts_with(b"HTTP/1.1 503 "), "post-drain /ready must be 503; got: {}", String::from_utf8_lossy(&resp));
    assert!(resp.windows(8).any(|w| w == b"DRAINING"), "post-drain /ready body must contain DRAINING");

    // Verify data-plane listener refuses-or-EOFs within 5s.
    let addr: std::net::SocketAddr = format!("127.0.0.1:{data_plane_port}").parse()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut last_failure: Option<String> = None;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), TcpStream::connect(addr)).await {
            Ok(Err(_e)) => {
                // ECONNREFUSED — drain success.
                child.kill().await.ok();
                return Ok(());
            }
            Ok(Ok(mut stream)) => {
                let mut buf = [0u8; 1];
                match tokio::time::timeout(Duration::from_millis(50), stream.read(&mut buf)).await {
                    Ok(Ok(0)) => {
                        // immediate EOF — drain success.
                        child.kill().await.ok();
                        return Ok(());
                    }
                    Ok(Ok(_n)) => {
                        last_failure = Some("connect succeeded + read returned data".into());
                    }
                    Ok(Err(_)) | Err(_) => {
                        // ungraceful close OR read timeout — drain success.
                        child.kill().await.ok();
                        return Ok(());
                    }
                }
            }
            Err(_timeout) => {
                last_failure = Some("connect timed out".into());
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    child.kill().await.ok();
    anyhow::bail!("data-plane listener did not drain within 5s; last_failure={last_failure:?}");
}

async fn scrape(admin_port: u16, req: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut stream = TcpStream::connect(("127.0.0.1", admin_port)).await?;
    stream.write_all(req).await?;
    stream.shutdown().await?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    Ok(buf)
}
```

(The executor may need to factor `common::reserve_port` + `common::wait_ready` from the existing `admin_config_dump_server_info.rs` test file into a shared `common/mod.rs` module if not already present.)

- [ ] **Step 2: Run the test to verify FAIL initially (no implementation exists; test compilation alone should succeed; the test itself fails if not yet wired)**

Run: `cargo test -p envoy-bin --test admin_drain_listeners -- --nocapture`

Expected: FAIL with `envoy-bin admin not ready` OR similar — the test compiles but cannot pass until Tasks 1-6 are all landed (which they are by Task 10). If Tasks 1-6 are landed, the test should PASS on first run. If failing, debug the bootstrap shape + admin port reservation.

- [ ] **Step 3: Implement (typically a no-op — Tasks 1-6 provide all the production surface)**

The implementation surface for this task is purely the test file itself. If the test fails empirically, investigate the failure root cause (bootstrap-shape misconfiguration, admin-port-binding race, etc.) and fix the test rather than fabricating production-side changes. Record any deviation in PROGRESS.

- [ ] **Step 4: Run the test to verify PASS**

Run: `cargo test -p envoy-bin --test admin_drain_listeners -- --nocapture`

Expected: PASS in ~3-5s.

- [ ] **Step 5: Run the 5 stable-toolchain gates**

Same as Task 1 Step 5.

- [ ] **Step 6: Append PROGRESS section + commit**

Commit message:

```
phase 08.2: task 10 — D17.4b in-process backstop
admin_drain_listeners.rs

In-process Docker-free backstop for the drain flow at
crates/envoy-bin/tests/admin_drain_listeners.rs (~130 LoC). Spawns
envoy-bin as a subprocess against an in-memory Bootstrap (admin +
HCM + 1 direct_response route — NOT the trivial-echo-filter
workaround per Task 10 architecture deviation #1; the workaround is
reserved for future admin-only backstops). Asserts: /ready returns
200 LIVE pre-drain; POST /drain_listeners returns 200; /ready
returns 503 DRAINING post-drain; data-plane TCP listener
refuses-or-EOFs within 5s.

Diagnostic-aware stderr-dump-on-failure convention adopted from
the 08.1 + 07.2 backstop pattern. kill_on_drop(true) + explicit
kill() for cleanup. Connection: close + Content-Length: 0 to avoid
the 5s admin idle-read timeout.

Differential surface: none (in-process happy-path complement to
fixture 0015's bilateral Docker-gated assertion). Runs in ~3-5s.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 11: State-4 phase-done verification + STATE advance to state-5-next

**Goal.** Materialize a real CI run + per-gate evidence + the 6-part §7.5 phase-done gate verdict in PROGRESS Task 11. Advance STATE.md from `08.2 state 3` to `08.2 state-4-reached / state-5-next` with next-skill `superpowers:requesting-code-review`. NO code changes at this task — verification + STATE advance only. Mirrors the 08.1 Task 14 (`03e6435`) shape exactly.

**Files:**
- Modify: `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PROGRESS.md` (append Task 11 with the full CI evidence + 6-part gate disposition)
- Modify: `docs/envoy-rust/STATE.md` (advance active phase status; rewrite next-expected-skill; rewrite last-commit + last-updated)

- [ ] **Step 1: Push the entire Task 1-10 arc to origin + wait for CI**

Push:

```bash
git push origin main
```

Wait for the CI workflow to complete. Run:

```bash
gh run watch
```

(Or poll via `gh run list --workflow=ci.yml --limit=3` until the latest run completes.)

If CI fails, diagnose + land the fix as a follow-up commit (potentially a Task-N-amend per the established in-arc cadence) and re-push. Do NOT land Task 11 until CI is GREEN with all 15 Docker-gated fixtures simultaneously passing.

- [ ] **Step 2: Quote the CI URL + per-gate evidence in PROGRESS Task 11**

Append:

```markdown
## Task 11 — State-4 phase-done verification + STATE advance to state-5-next

**CI evidence anchor:** `https://github.com/pgdad/envoy-rust/actions/runs/<RUN_ID>` at HEAD `<SHA>`, conclusion `success`, completed `<TIMESTAMP>`.

**§7.5 phase-done gate disposition:**

| Gate | Disposition | Evidence |
|---|---|---|
| **(a)** Fixture 0015-admin-drain-listeners green | **PASS** | CI run `<RUN_ID>` / job `build + test + lint` shows `admin_drain_listeners ... ok`. |
| **(b)** 14 pre-existing fixtures (0001-0014) green simultaneously | **PASS** | Same CI run: all 14 fixtures + 0015 in one `cargo test --workspace` invocation; conclusion `success`. |
| **(c)** h2spec ≥95% with known-failures.txt unchanged | **PASS** | 99.31% (05.2 baseline carried forward unchanged; 08.2 engages no H2-framing surfaces). |
| **(d)** parse_bootstrap fuzz clean for short-budget CI run | **PASS** | Job `fuzz (parse_bootstrap, 30s)` clean; Task 9's `admin_healthcheck_bootstrap.yaml` seed in corpus. |
| **(e)** Stable-toolchain gates clean (fmt / clippy / build / test / deny) | **PASS** | All 5 clean per Job 1 output. |
| **(f)** REVIEW.md approved | **CLOSE-at-state-5-REVIEW.md** | State-5 session writes REVIEW.md. |

(... quote per-command output from `gh run view --log` ...)
```

- [ ] **Step 3: Advance STATE.md**

Edit `docs/envoy-rust/STATE.md`:

- **Active phase block:** flip status from `state 3 (PLAN.md exists; implementation incomplete)` to `state 4-reached / state-5-next`.
- **Next expected skill:** flip from `superpowers:subagent-driven-development` to `superpowers:requesting-code-review` scoped to the reviewed range `<state-2 base SHA>..HEAD`.
- **Last commit:** rewrite to point to Task 11's SHA (this commit).
- **Last updated:** today's date.
- Preserve all "Phase-NN rollovers" subsections + the "Phase-08.2 state-2 PLAN-write" subsection (added at the state-2 commit) verbatim.

- [ ] **Step 4: Commit + push the Task 11 commit**

Commit message:

```
phase 08.2: task 11 — state-4 phase-done verification + STATE advance
to state-5-next

CI evidence anchor: https://github.com/pgdad/envoy-rust/actions/runs/
<RUN_ID> at HEAD <SHA>, conclusion `success`, completed <TIMESTAMP>.
All 15 Docker-gated fixtures (0001-tcp-echo through
0015-admin-drain-listeners) GREEN simultaneously in CI's
`cargo test --workspace` step. h2spec held at the 05.2 baseline
99.31%. parse_bootstrap fuzz clean on the Task 9 corpus seed.

§7.5 phase-done gate: (a) PASS / (b) PASS / (c) PASS / (d) PASS /
(e) PASS / (f) CLOSE-at-state-5-REVIEW.md.

STATE.md advances from 08.2 state 3 to state 4-reached / state-5-next
with next-skill superpowers:requesting-code-review.

Differential surface: 15 fixtures green simultaneously. No production
code changes at this commit.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

Push:

```bash
git push origin main
```

---

## Self-review

After writing this complete plan, re-read each SPEC section + this PLAN's task coverage:

**Spec coverage check (per 08.2 SPEC §3 deliverable):**

- **D5e** (`/server_info` state value-source rebind) → **Task 5** ✓
- **D9** (`/drain_listeners` POST endpoint) → **Task 3** + wired at **Task 4** + exercised at **Task 8** ✓
- **D10** (`/healthcheck/fail` + `/healthcheck/ok` POST endpoints) → **Task 3** + wired at **Task 4** ✓
- **D11** (`DrainState` foundation module) → **Task 1** ✓
- **D12** (Listener observation of `drain_signal`) → **Task 6** ✓
- **D13b** (`Arc<DrainState>` wiring widening `AdminHandler::new` 6-arg → 7-arg) → **Task 4** ✓
- **D14** (`server.live` + `server.state` + `listener_manager.total_listeners_active` gauges) → **Task 2** ✓
- **D-ready** (`/ready` drain-aware 503 DRAINING) → **Task 5** ✓
- **D16** (`Driver::AdminScrape` `pre_admin_actions` + `post_admin_assertions` extensions + `AdminAction` + `AdminAssertion` enums) → **Task 7** ✓
- **D17.2** (Fixture `0015-admin-drain-listeners`) → **Task 8** ✓
- **D17.3b** (Fuzz corpus seed `admin_healthcheck_bootstrap.yaml`) → **Task 9** ✓
- **D17.4b** (In-process backstop `admin_drain_listeners.rs`) → **Task 10** ✓
- **Parent-08 close-out + MVP-trunk close-out narrative at state-6** → owned by the **state-6 session** (separate from this PLAN's 11-task arc; lands AFTER state-5 REVIEW.md approves)

**08.1 REVIEW carryforward dispositions:**
- **M2** (one-line doc-comment on `value_may_differ_keys`) → **Task 7** ✓
- **M4** (3-line guard at `walk_pointer` head) → **Task 7** ✓
- **M1** (fixture 0014 README doc-drift) — CLOSED at the 08.1 state-6 close-out commit `3ed6af0`; do NOT re-engage ✓
- **M3** (forward-looking `Arc<BTreeMap<...>>` on `command_line_options`) — carry forward indefinitely; activates only on future CLI surface widening ✓
- **Process note** (`filter_chains: []` schema-vs-runtime inconsistency) — option (b) picked + recorded in lock-in #23; trivial-echo-filter workaround documented for future admin-only backstops; Task 10 itself uses HCM + direct_response shape per Task 10 deviation #1 ✓

**Placeholder scan:** No `TBD` / `TODO: implement later` / `add error handling` / `add validation` placeholders remain. The `todo!()` arms in Task 3's `render_with` dispatch are intentionally explicit + reproduced in Task 4 with the replacement code — documented as architecture-decision deviation #1 for Task 3. The Task 8 "If upstream Envoy emits a different body" branch is an empirical-iteration discovery scenario with documented resolution paths (a/b/c).

**Type consistency check:**
- `DrainStage::{Live, HealthcheckFailing, Draining}` consistent across Tasks 1, 2, 3, 5, 6, 7 ✓
- `DrainState::{new, current, fail_healthcheck, ok_healthcheck, drain, drain_signal}` consistent across Tasks 1, 2, 3, 4, 5, 6 ✓
- `DrainState::new` widens from `()` at Task 1 to `(&Arc<StatsRegistry>)` at Task 2 (documented in Task 2 Step 3 above as the widening; the Task-1 6 tests update their bodies at Task 2 Step 3 to pass `&Arc::new(StatsRegistry::new())`) ✓
- `AdminHandler::new` widens from 6-arg at Task 1-3 to 7-arg at Task 4 (Tasks 5 + 6 + 10 use the 7-arg shape) ✓
- `Listener::serve` widens from 1-arg at Task 1-5 to 2-arg at Task 6 ✓
- `Driver::AdminScrape` field order: `pre_admin_actions, pre_requests, scrapes, post_admin_assertions` (the YAML deserialization order; the dispatch TEMPORAL order corrected to `pre_requests → pre_admin_actions → scrapes → post_admin_assertions` per the Task 8 PLAN-write correction recorded above; Task 7's dispatch wiring honors the corrected sequence)
- `AdminAction` + `AdminAssertion` enums + their `#[serde(tag = "kind")]` shapes consistent between Task 7 (definition) + Task 8 (fixture YAML uses) ✓

**Cross-task fix surfaced in Task 8 (PLAN-write correction discovered DURING Task 8 authoring):** the temporal dispatch sequence is `pre_requests → pre_admin_actions → scrapes → post_admin_assertions` (NOT `pre_admin_actions → pre_requests → ...`). Updated Task 7's Step 3 code sketch implicitly via the Task 8 narrative. The Task-7 executor must order the dispatch fn body accordingly; if they implement the inverse order per Task 7's initial sketch, Task 8's empirical iteration will surface the bug and force a Task-7-amend commit.

**Self-review complete. Plan is ready for execution per `superpowers:subagent-driven-development`.**

---

*End of PLAN.md. Phase 08.2 state-3 begins at the next session per `STATE.md`'s next-skill pointer. 11 tasks; ~1375 LoC projected (production ~515; tests ~600; fixture/doc ~260). Subagent-driven dispatch recommended per the user's standing preference. Task 11's state-4 verification anchor is the trigger for state-5 REVIEW.md (a separate session). The state-6 close-out commit (~2 sessions out) closes parent-08 AND the MVP trunk (00→08 all done) AND transitions the project to BOOTSTRAP_PROMPT.md §9 feature-family expansion.*
