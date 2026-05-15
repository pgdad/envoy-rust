# Phase 08.2 (`08.2-endpoint-triggered-drain`) — SPEC

- **Phase id:** `08.2`
- **Slug:** `08.2-endpoint-triggered-drain`
- **Status before this SPEC lands:** `planned` (this SPEC's landing commit is the parent-08 state-2 split commit, which creates this row at status `planned`; row 08.2 flips `planned` → `in-progress` later when STATE.md first points at it — after 08.1 reaches state 6 and STATE.md advances to 08.2 state 1).
- **Parent:** `docs/envoy-rust/phases/08-admin-api-and-drain/SPEC.md` (committed at HEAD `0202e38`; the parent-08 state-1 brainstorm SPEC). This sub-phase SPEC is a **surface-slice carve-out** of the parent SPEC — no new brainstorming session needed because the parent SPEC §6.1 explicitly named the recommended split shape; this SPEC operationalizes that shape's drain-side slice.
- **Sibling:** `docs/envoy-rust/phases/08.1-admin-endpoint-surface/SPEC.md` (created in the same split commit). **08.2 ships AFTER 08.1 in execution order** (08.1 → 08.2; not parallelizable per §5 ordering invariant below).
- **Position:** 08.2 is the **closing sub-phase of parent-08** AND the **closing phase of the MVP trunk**. Per the project's closing-sub-phase invariant (mirrors phase-02's `f04e21a`, phase-03's `ca81226`, phase-04's `e626862`, phase-05's `82c26b8`, phase-06's `b918f33`, phase-07's `1d52156`), 08.2's state-6 close-out commit ALSO flips parent ROADMAP row `08` from `in-progress` to `done`. With the parent-08 close-out, the MVP trunk stands **00→08 all `done`** and the `BOOTSTRAP_PROMPT.md` §9 feature-family expansion begins (phases 09+).
- **Charter source:** parent-08 SPEC §6.1 recommended split-shape line: *"08.2 — drain machinery + fixture 0015 + parent-08 close-out (~800 LoC): D9 (/drain_listeners) + D10 (/healthcheck/fail + /healthcheck/ok) + D11 (DrainState module at crates/envoy-listener/src/drain.rs, re-exported from envoy-admin) + D12 (listener observation … envoy-admin::serve does NOT observe its own drain_signal) + D13 (envoy-bin::main shared-handle wiring — Arc<Bootstrap> + Arc<ClusterManager> + Arc<DrainState> + Arc<Instant>) + D16 (AdminAction + AdminAssertion harness extensions) + D17.2 (fixture 0015) + partial D17.3 + partial D17.4 + parent-08 close-out (closes the MVP trunk)."*

---

## 1. Goal and acceptance signal

Phase 08.2 ships the **endpoint-triggered graceful-drain semantics** named by `BOOTSTRAP_PROMPT.md` §8 row 08, atop the admin endpoint surface 08.1 lands. Specifically: adds **3 new POST-only admin endpoints** (`/drain_listeners`, `/healthcheck/fail`, `/healthcheck/ok`), a small shared `DrainState` module observed by the data-plane listener accept loops, the listener-observation wiring, the four-handle plumbing in `envoy-bin::main`, three new drain-related stats, and the differential fixture asserting the wire-level drain effect. 08.2 also patches `/server_info`'s `state` field source from 08.1's hardcoded `"LIVE"` placeholder to `DrainState::current()` and `/ready`'s response from the existing 06.1 200-LIVE-only path to drain-aware 503-DRAINING. 08.2 closes the MVP trunk at its state-6 commit.

**Differential surface added by phase 08.2:**

- **Fixture `0015-admin-drain-listeners`** — admin scrape sequence: `GET /ready` (200 LIVE) → `POST /drain_listeners` (200 OK) → `GET /ready` (503 DRAINING) → `GET /server_info` (`state: "DRAINING"`) → new connection to the data-plane listener refused-or-immediately-closed within the documented drain window. Bilateral on both proxies.

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0015-admin-drain-listeners` green at Docker-gated CI.
- **(b)** All **14 pre-existing differential fixtures** (`0001-tcp-echo` through `0014-admin-config-dump-server-info`) **remain green simultaneously** at the same CI run.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%; phase 08.2 engages no H2-framing surfaces).
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (new seed for the healthcheck-relevant bootstrap shape, D17.3b).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** at 08.2's state-4 verification — all **15 Docker-gated fixtures (0001-0015) green simultaneously** at the state-4 CI run.

**Closes the MVP trunk.** At state-6 (which is this sub-phase's close-out commit), ROADMAP row `08.2` flips `in-progress` → `done` AND ROADMAP row `08` flips `in-progress` → `done` (the closing-sub-phase invariant). The project then transitions to the `BOOTSTRAP_PROMPT.md` §9 feature-family expansion.

---

## 2. Behavior-contract scope for phase 08.2

Phase 08.2 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with two authored additions, landed at the tasks where each is first empirically exercised. (08.1 landed the GET-endpoint rows + the header allow-list dedupe note.)

### 2.1 New top-level subsection — "Admin endpoint body shapes" (3 of 7 rows; 08.2 ships POST endpoints)

Three new rows appended to the subsection 08.1 created:

| Endpoint | Method | Body kind | Equivalence disposition |
|---|---|---|---|
| `/drain_listeners` | POST | empty | Status 200; empty body; effect-only endpoint. |
| `/healthcheck/fail` | POST | empty | Status 200; empty body; effect-only endpoint. |
| `/healthcheck/ok` | POST | empty | Status 200; empty body; effect-only endpoint. Restores from `HealthcheckFailing` → `Live`. `Draining` is sticky — `/healthcheck/ok` after `/drain_listeners` does NOT un-drain. |

08.2 also patches 08.1's `/server_info` row note: the `state` field's source switches from the literal `"LIVE"` to `DrainState::current()` per the mapping `Live → "LIVE"`, `HealthcheckFailing → "LIVE"` (server-state independent of healthcheck-fail per Envoy semantic — `/ready` flips to 503 but `/server_info.state` stays `"LIVE"`), `Draining → "DRAINING"`. The structural shape (the JSON keys + `state: <string>` field) is unchanged from 08.1; the value-binding source is what 08.2 changes.

### 2.2 Stat-name mapping extension (3 new rows)

Per parent-08 SPEC §2.3, three new rows appended to `BEHAVIOR_CONTRACT.md`'s `Stat-name mapping` section:

- `server.live` (gauge, value-exact). `1` when `DrainState::current() == Live`; `0` otherwise. Both proxies emit on every snapshot. Updated inline at the `DrainState::fail_healthcheck()` / `ok_healthcheck()` / `drain()` state-transition sites (NOT polled — one source of truth).
- `server.state` (gauge, value-exact for the deterministic-`LIVE` harness path). Encoded enum: `Live=0`, `HealthcheckFailing=1`, `Draining=2`. Phase 08.2's fixtures assert the `Live=0` initial state and the post-drain `Draining=2` state.
- `listener_manager.total_listeners_active` (gauge, value-exact). Count of currently-active data-plane listeners. Incremented at `Listener::serve` entry; decremented at `Listener::serve` epilogue (after drain completes). Mirrors the existing 06.3 `listener.<name>.downstream_cx_active` gauge pattern (RAII guard).

### 2.3 New top-level subsection — "Admin-action effect equivalence"

A small subsection (5–8 lines) stating the cross-proxy invariant: `POST /drain_listeners` on either proxy must drive that proxy to refuse-or-immediately-close new connections on its data-plane listeners within the drain window (5s `DRAIN_BUDGET`). This is the wire-level observable both proxies satisfy; the internal mechanism is implementation-specific. Mirrors the parent-08 SPEC §2.4 sketch.

---

## 3. Deliverables

Phase 08.2's scope is enumerated as deliverables `D5e` (08.1's `/server_info` extension), `D9`, `D10`, `D11`, `D12`, `D13b`, `D14`, `D16`, `D17.2`, `D17.3b`, `D17.4b`, plus the parent-08 close-out. Deliverables are listed in roughly execution order, but the SPEC is not prescriptive about ordering beyond the foundation-first rule (§3 below).

### Foundation-first — `DrainState` module (D11)

D11 lands FIRST because every downstream surface (D9, D10, D12, D13b, D14, D5e, the `/ready` patch) consumes `DrainState`. Landing it first means each subsequent task plugs into a stable type.

- **D11 — `DrainState` module.** Lives at `crates/envoy-listener/src/drain.rs` per parent-08 SPEC §5.1's Cargo-cycle resolution (re-exported from `envoy-admin::DrainState` so admin-side call sites read naturally). The natural `envoy-admin::DrainState` placement would create a Cargo cycle (envoy-admin already depends on envoy-listener::ConnectionHandler); the resolution mirrors the M4 `DRAIN_BUDGET` hoist (D3 at 08.1) + the 05.3 / 07.1 cycle-resolution doctrine. **No new ADR required** — doctrine-clear by precedent. Module shape:
  ```rust
  pub enum DrainStage { Live, HealthcheckFailing, Draining }
  pub struct DrainState {
      state: std::sync::atomic::AtomicU8,
      notify: tokio::sync::Notify,
  }
  impl DrainState {
      pub fn new() -> Self;
      pub fn current(&self) -> DrainStage;
      pub fn fail_healthcheck(&self);
      pub fn ok_healthcheck(&self);
      pub fn drain(&self);
      pub fn drain_signal(&self) -> impl std::future::Future<Output = ()> + '_;
  }
  ```
  - `state: AtomicU8` (`Live=0`, `HealthcheckFailing=1`, `Draining=2`).
  - `drain()` does `compare_exchange` to set state to `Draining` and calls `notify.notify_waiters()` **exactly once** on the first `Live → Draining` or `HealthcheckFailing → Draining` transition. Repeat calls are idempotent (no-op + no notify).
  - `drain_signal()` is `notify.notified()` if state currently `< Draining`, or an immediately-ready future (e.g., `future::ready(())`) if state already `Draining`. The signal is observed by listener accept loops; observation is idempotent and re-entrant.
  - `fail_healthcheck()`: `Live → HealthcheckFailing` (compare_exchange); `HealthcheckFailing → HealthcheckFailing` (no-op); `Draining → Draining` (no-op — sticky).
  - `ok_healthcheck()`: `HealthcheckFailing → Live`; `Live → Live`; `Draining → Draining` (no-op — sticky).
  - All state transitions documented inline. **Sticky-drain semantic per parent-08 SPEC §5.6.**
  
  Unit tests (≥6): `new_returns_live`, `drain_flips_to_draining_and_notifies_waiters_once`, `fail_healthcheck_flips_to_healthcheck_failing`, `ok_healthcheck_restores_to_live`, `ok_healthcheck_after_drain_is_noop_sticky`, `repeat_drain_calls_are_idempotent`. The notify-once test uses two `tokio::spawn` tasks each awaiting `drain_signal()` + a third spawning `drain()` once; both signal tasks must complete; a fourth post-drain `drain_signal()` must complete immediately.

### Admin POST endpoints (D9, D10)

- **D9 — `/drain_listeners` endpoint.** New variant `AdminEndpoint::DrainListeners` (POST, `ALLOWED = "POST"`). Renders 200 OK with empty body. Side effect: invokes `DrainState::drain()` on the shared `Arc<DrainState>` handle (read from `&handler` via the `Arc<DrainState>` 08.2 wires via D13b). Sticky — repeat POSTs are idempotent.

- **D10 — `/healthcheck/fail` + `/healthcheck/ok` endpoints.** Two new variants `AdminEndpoint::HealthcheckFail` (POST) + `AdminEndpoint::HealthcheckOk` (POST). Render 200 OK with empty body. Side effects: invoke `DrainState::fail_healthcheck()` / `DrainState::ok_healthcheck()` respectively. `/healthcheck/ok` after `/drain_listeners` is a no-op per the sticky-drain semantic at D11.

### Listener observation (D12)

- **D12 — Listener observation of `drain_signal`.**
  - **`crates/envoy-listener/src/lib.rs::Listener::serve`** accepts a new `drain: Arc<DrainState>` parameter (signature widened; the existing single `shutdown` future stays). The select arm `_ = drain.drain_signal() => { ... }` drives the same drain code path as the existing `_ = &mut shutdown` arm (stop accepting + 5s `DRAIN_BUDGET` for in-flight + abort stragglers). The 06.3-landed RAII gauge guard for `listener.<name>.downstream_cx_active` is unchanged; the new D14 gauge `listener_manager.total_listeners_active` adds a separate RAII guard at `serve` entry / exit (see D14 below).
  - **`crates/envoy-admin/src/handler.rs::serve`** — the admin listener does **NOT** observe its own `drain_signal`. The admin listener stays serving during drain (so `/server_info` and `/stats/prometheus` remain reachable for operators and Prometheus pollers). The admin listener drains only via SIGTERM, exercised via the existing `CancellationToken` path in `envoy-bin::main`. Per parent-08 SPEC §5.5.
  - Unit tests in `envoy-listener`: `serve_returns_when_drain_signal_fires` (constructs a `DrainState`, spawns a `serve` task with a manual `shutdown` future that never completes, calls `.drain()`, asserts the `serve` task completes within `DRAIN_BUDGET + 1s`). Plus the existing 06.3-landed listener tests stay green under the widened signature (signature-update churn only).

### Shared-handle wiring partial (D13b)

- **D13b — `envoy-bin::main` `Arc<DrainState>` wiring (08.2 portion).** 08.1's D13a wired three handles (`Arc<Bootstrap>`, `Arc<ClusterManager>`, `Arc<Instant>`); D13b adds the fourth.
  - One `Arc<DrainState>` constructed at startup in `envoy-bin::main`.
  - Cloned into the admin handler (writer; for D9/D10 endpoint side-effects + D5e's `/server_info` `state` read).
  - Cloned into each data-plane listener accept-loop spawn (reader/observer; for D12).
  
  `AdminHandler::new` signature widens from 08.1's `new(config, registry, bootstrap, cluster_manager, start_instant)` to `new(config, registry, bootstrap, cluster_manager, start_instant, drain: Arc<DrainState>)`. The admin endpoint renders read `drain.current()` from `&self` via the `&handler` closures (existing 06.1 pattern). The 08.1-landed `/server_info` `state` literal swaps to `match drain.current() { ... }` at D5e.

### `/server_info` state rebind (D5e) + `/ready` drain-aware response (D-ready)

- **D5e — `/server_info` state-field source rebind.** Per §5.4 of the 08.1 SPEC, 08.1 emits `state: "LIVE"` as a literal. 08.2 patches this site to `match drain.current() { Live | HealthcheckFailing => "LIVE", Draining => "DRAINING" }`. Pure value-binding swap; no struct-shape change. ~5 LoC.

- **D-ready — `/ready` drain-aware response.** 06.1's `/ready` returns 200 LIVE / 503 not-ready (the not-ready arm is for transient init paths that don't fire in static-bootstrap mode). 08.2 extends to also return 503 DRAINING when `DrainState::current() == Draining`. The HealthcheckFailing case ALSO returns 503 (matches Envoy semantic — `/ready` is operator-facing for load-balancer take-out). Response body: `"DRAINING\n"` on the Draining 503; existing body on the LIVE 200; existing body on the HealthcheckFailing 503 (`"Service Unavailable\n"` or equivalent per 06.1). Per parent-08 SPEC §3 fixture-0015 sub-case 3 (`GET /ready` after `POST /drain_listeners` returns 503 with body `"DRAINING\n"`). ~15 LoC + 3 unit tests (LIVE / HealthcheckFailing / Draining).

### Stats wiring (D14)

- **D14 — `server.live` + `server.state` + `listener_manager.total_listeners_active` stats.** Three new gauges registered at startup against the existing `StatsRegistry`. Updated via:
  - `server.live` ← `1` when `DrainState::Live`, else `0`. Updated inline at the `DrainState::fail_healthcheck()` / `ok_healthcheck()` / `drain()` state-transition sites (one source of truth; NOT polled). The gauge handle is constructed in `DrainState::new(registry: &Arc<StatsRegistry>)` and stored as a `DrainState` field — or, equivalently, passed in as a per-method `&Gauge` reference. Planner picks at PLAN time; the simpler `DrainState` field shape is recommended.
  - `server.state` ← discriminant of `DrainStage` (0/1/2). Same update site as `server.live`.
  - `listener_manager.total_listeners_active` ← incremented at `Listener::serve` entry via `gauge.inc()`; decremented at `Listener::serve` epilogue via `gauge.dec()` (RAII guard wraps the increment + Drop-decrement). Mirrors the existing 06.3 `listener.<name>.downstream_cx_active` gauge pattern.

  All three values value-exact under deterministic-harness conditions; the BEHAVIOR_CONTRACT entries land per §2.2 above.

### Harness extensions (D16)

- **D16 — `Driver::AdminScrape` action-sequence extension.** The 06.1-landed `Driver::AdminScrape { pre_requests: Vec<...>, request: ..., expected_status, response_body, ... }` shape extends with two new fields:
  ```rust
  Driver::AdminScrape {
      pre_admin_actions: Vec<AdminAction>,   // NEW — fires before `request`
      pre_requests: Vec<...>,                // existing
      request: ...,                          // existing
      expected_status: ...,                  // existing
      response_body: ...,                    // existing
      post_admin_assertions: Vec<AdminAssertion>,  // NEW — fires after `request`
  }
  enum AdminAction { Post { path: String, expected_status: u16 }, /* extensible */ }
  enum AdminAssertion {
      DataPlaneConnectionRefused { listener_address: String, within: Duration },
      // extensible
  }
  ```
  Fixture 0015 uses `pre_admin_actions: [Post { path: "/drain_listeners", expected_status: 200 }]` + `post_admin_assertions: [DataPlaneConnectionRefused { listener_address, within: 5s }]`. The implementation drives both `pre_admin_actions` against each proxy (both Envoy and envoy-rust) before driving `request`; drives `post_admin_assertions` against each proxy after `request`. Assertion `DataPlaneConnectionRefused` opens a `TcpStream::connect` to the named listener address with a `within`-deadline timeout; success criteria: connect returns Err (refused) OR connect succeeds + read returns immediately with EOF (immediately-closed). Either disposition is accepted.

### Fixture (D17.2) + corpus seed (D17.3b) + backstop (D17.4b)

- **D17.2 — Fixture `tests/fixtures/0015-admin-drain-listeners/`.** Reuses fixture 0007's bootstrap shape (HCM + direct_response) — minimal data-plane surface — so the drain assertion focuses on the listener-rejection behavior, not on upstream complexity. Single admin-scrape sequence: `pre_requests: [GET /ready → 200 LIVE]`, `pre_admin_actions: [POST /drain_listeners → 200]`, `request: GET /ready` with `expected_status: 503`, `response_body: BodyRule::ByteExact { value: "DRAINING\n" }`, `post_admin_assertions: [DataPlaneConnectionRefused { listener_address: <bound port>, within: 5s }]`. Fixture files: `envoy.yaml`, `envoy-rust.yaml`, `inputs/payload.bin` (0-byte placeholder), `expectations.yaml`, `README.md`. Plus the Docker-gated wrapper `tests/differential/tests/admin_drain_listeners.rs`.

- **D17.3b — Fuzz corpus seed extension.** One new YAML seed under `crates/envoy-config/fuzz/corpus/parse_bootstrap/`: `admin_healthcheck_bootstrap.yaml` with a healthcheck-relevant bootstrap shape. Mirrors the 06.1 admin-corpus-seed + 07.2 header-mutation-corpus-seed pattern.

- **D17.4b — In-process backstop.** One new file `crates/envoy-bin/tests/admin_drain_listeners.rs` — exercises the same drain flow as fixture 0015 in-process (no Docker). Asserts: `GET /ready` returns 200 LIVE; `POST /drain_listeners` returns 200; `GET /ready` returns 503 DRAINING; the data-plane listener rejects new TCP connections within 5s of `POST /drain_listeners`.

### Parent-08 + MVP-trunk close-out

08.2's state-6 commit:
- Flips ROADMAP row `08.2` `in-progress` → `done`.
- Flips parent ROADMAP row `08` `in-progress` → `done` (the closing-sub-phase invariant).
- Advances STATE.md "Active phase" to point at "awaiting next planning" (MVP trunk complete; the next session per `BOOTSTRAP_PROMPT.md` §9 brainstorms the first feature-family phase 09+).
- The MVP-trunk close-out narrative is added to STATE.md's "Notes" section as a new subsection.

---

## 4. Out of scope

Phase 08.2 does NOT engage:

- **Beyond the 7-endpoint admin surface.** `/quitquitquit`, `/runtime`, `/logging`, etc. — defer per parent-08 SPEC §4. The MVP-trunk close at this commit is the threshold; feature-family phases (§9 of `BOOTSTRAP_PROMPT.md`) extend.
- **Admin auth / TLS** — defers indefinitely per 06.1 cross-sub-phase rule 3.
- **HTTP/2 admin** — defers indefinitely.
- **Drain-time config (`drain_time_s`, `--drain-strategy`).** envoy-rust's drain budget stays fixed at 5s per the existing `DRAIN_BUDGET`. Tunable drain timing is its own design decision; not gated by MVP.
- **`Connection: close` injection during drain.** Endpoint-triggered drain is testable via listener-rejection alone; `Connection: close` on in-flight responses is a separable later feature.
- **Drain-state propagation to health-check filter responses.** envoy-rust has no health-check filter yet (HTTP filter family is post-MVP); the future health-check filter consumes `DrainState` for the response-code flip.
- **SIGKILL→SIGTERM of harness subject subprocess (phase-00 I3).** Continues to defer. Endpoint-triggered drain exercises drain via `POST /drain_listeners`, NOT via signal; the `nix` crate stays off the permitted-foundations list.
- **xDS-derived config in `/config_dump`** — defers to xDS family.
- **`/server_info` hot-restart fields beyond the constant `"disabled"`** — envoy-rust does not implement hot-restart; the runtime + hot restart family (§9) extends if it lands.

---

## 5. Architectural invariants

### 5.1 Ordering invariant — 08.2 ships AFTER 08.1

08.1 must reach state 6 (done) before 08.2 enters state 1 (per the ROADMAP `depends-on` column). Reasons:
- 08.2's D9/D10 add new `AdminEndpoint` variants. The `Dispatch` enum + the `const ALLOWED` per-variant pattern are 08.1's D4 surface; 08.2 reuses them.
- 08.2's `/server_info` D5e patch swaps the value source on 08.1's `ServerInfoBody`. The struct must exist first.
- 08.2's D-ready (`/ready` drain-aware) patches the 06.1 + 08.1 surface; 08.1's dispatch infrastructure must be in place.
- 08.2's `Arc<DrainState>` D13b extends 08.1's `AdminHandler::new` signature; 08.1's three-handle widening must land first.

The two sub-phases are NOT parallelizable.

### 5.2 `DrainState` placement — settled at parent-08 SPEC §5.1

Lives at `crates/envoy-listener/src/drain.rs`, re-exported from `envoy-admin::DrainState`. Mirrors the 05.3 / 07.1 cycle-resolution doctrine + the M4 `DRAIN_BUDGET` hoist (D3 at 08.1). No new ADR.

### 5.3 Wire-state vs internal-state mapping (per parent-08 SPEC §5.5)

- `DrainStage::Live → "/server_info.state": "LIVE"`, `/ready → 200`.
- `DrainStage::HealthcheckFailing → "/server_info.state": "LIVE"` (server-state independent of healthcheck-fail per Envoy semantic), `/ready → 503` (operator-facing — flips to 503 so LB takes the instance out of rotation).
- `DrainStage::Draining → "/server_info.state": "DRAINING"`, `/ready → 503` with body `"DRAINING\n"`.

`/server_info.state` never emits `"PRE_INITIALIZING"` (envoy-rust's listener bind is synchronous in `envoy-bin::main` so by the time the admin listener serves its first request, all data-plane listeners are already bound; the wire enum carries `PRE_INITIALIZING` for forward-compat but no code path emits it).

### 5.4 Admin listener stays serving during drain

Per parent-08 SPEC §5.5 + D12. The admin listener observes shutdown but NOT its own `drain_signal`. `GET /server_info` returns 200 with `state: "DRAINING"` even while the data-plane listener is refusing new connections. Operator-tool reachability during drain is the whole point of the endpoint-triggered model.

### 5.5 Sticky drain

Per parent-08 SPEC §5.6 + D11. Once `DrainState::drain()` fires, the state is sticky — `/healthcheck/ok` does NOT un-drain. Matches Envoy's documented behavior and prevents fixture flakiness.

### 5.6 No new top-level Cargo deps

`tokio::sync::Notify` (already used; part of `tokio`), `std::sync::atomic::AtomicU8` (std-lib) cover the new surfaces. No foundations grants required.

### 5.7 Stats namespacing — 2 new namespaces

`server.<stat>` for server-state metrics; `listener_manager.<stat>` for listener-manager metrics. Both are NEW namespaces (08.1 / 06.1 / 06.3 only wired `listener.<name>.*`, `cluster.<name>.*`, `http.<stat_prefix>.*`). Per parent-08 SPEC §5.4. The BEHAVIOR_CONTRACT extension lands per §2.2.

---

## 6. Implementation signposts for the planner

### 6.1 Split-gate re-evaluation

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether 08.2's PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 08.2's surface estimate at split-commit time:

- D11 (DrainState module) — ~120 LoC + 6 unit tests, ~1 task.
- D9/D10 (3 POST endpoints) — ~80 LoC + tests, ~1 task (the three endpoints are mechanically symmetric; one task covers all three).
- D12 (listener observation) — ~60 LoC at listener + signature-update churn across existing callers + ~3 unit tests, ~1-2 tasks.
- D13b (envoy-bin Arc<DrainState> wiring) — ~30 LoC, ~1 task.
- D5e (`/server_info` state rebind) — ~5 LoC + 1 test, folded into the D9/D10 task or its own ~0.5-task.
- D-ready (`/ready` drain-aware) — ~15 LoC + 3 tests, ~1 task.
- D14 (3 new stats) — ~80 LoC + tests, ~1 task.
- D16 (harness extension) — ~120 LoC, ~1 task.
- D17.2 + D17.3b + D17.4b (fixture + corpus seed + backstop) — ~300 LoC, ~2-3 tasks.
- State-4 verification — 1 task.
- Parent-08 close-out + STATE.md MVP-trunk completion narrative — 1 task (combined with state-6).

**Projection: ~10-13 tasks; ~810-920 LoC.** **Comfortably under the §6.1 split-gate** with healthy drift headroom (~40% LoC headroom; ~50% task-count headroom). **No nested split projected** — the planner lands a standalone PLAN.md per the standardized standalone-pre-Task-1-commit posture.

### 6.2 D11 ordering — DrainState lands FIRST

D11 lands at Task 1 (or Task 2 if a PROGRESS preamble task is needed; matches the 06.3 Task-1-preamble cadence). All downstream surfaces consume `DrainState`; landing it first means each subsequent task plugs into a stable type. Recommended task order: D11 (DrainState foundation) → D14 (stats registration alongside DrainState construction) → D9/D10 (POST endpoints; consumes `DrainState::drain()` / etc.) → D12 (listener observation; consumes `DrainState::drain_signal()`) → D13b (envoy-bin wiring; threads the `Arc<DrainState>`) → D5e + D-ready (consume `DrainState::current()` for state rendering) → D16 (harness) → D17.2 (fixture) → D17.3b (corpus seed) → D17.4b (backstop) → state-4 → state-6 (close).

### 6.3 `DrainState`'s notify shape

Per parent-08 SPEC §6.6. The `tokio::sync::Notify` pattern is the right primitive: it supports multiple consumers (one per listener), zero-copy notification, and is cheap to clone via `Arc`. The state-3 implementer must ensure `notify.notify_waiters()` fires **exactly once** on the first `Live → Draining` (or `HealthcheckFailing → Draining`) transition. Use `compare_exchange` + a successful-CAS guard; do NOT call `notify_waiters` unconditionally — repeat calls work but waste cycles. On already-`Draining`, `drain_signal()` returns an immediately-ready future (check state before calling `notified()` and `return future::ready(())` on the early-return branch).

### 6.4 Stats integration — inline at state transitions (not polled)

Per parent-08 SPEC §3 D14 + §3 above. The simplest source of truth is the state-transition methods on `DrainState` (`fail_healthcheck()` / `ok_healthcheck()` / `drain()`). Each method calls `compare_exchange` to update the AtomicU8 + `gauge.set(...)` for `server.live` + `server.state` if the CAS succeeded. This avoids a polling task and locks the gauge value to the atomic state by construction. The `listener_manager.total_listeners_active` gauge is independent — RAII guard at `Listener::serve` entry/exit.

### 6.5 `/ready` drain-aware response — fixture 0011 stays green

The 06.1-landed fixture 0011 exercises `/ready` returning 200 LIVE. Phase 08.2's `/ready` extension MUST keep fixture 0011 green: in fixture 0011 no `/drain_listeners` is POSTed, so `DrainState::current()` stays `Live` throughout, so `/ready` returns 200 LIVE as before. The new fixture 0015 exercises the drain path. **Both fixtures green at state-4 simultaneously** is the regression-equivalence proof.

### 6.6 Pre-state-4 fmt discipline

Per 06.1 REVIEW §7 R-9 + 07.1 + 07.2 PROGRESS attestation pattern. Per-task PROGRESS runs all 5 stable-toolchain gates at every task close.

### 6.7 State-4 evidence-discipline

Per the 05.3 REVIEW I3 → 06.1 / 06.2 / 06.3 / 07.2 closure chain. Phase 08.2's state-4 verification must materialize a real CI run + per-gate evidence in PROGRESS.md. **All 15 Docker-gated fixtures (0001-0015) green simultaneously** at the state-4 CI run.

### 6.8 Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 carryforward continues — 08.2 adds zero new top-level Cargo deps so the cadence pick stays unforced.

---

## 7. ADR projection

**Recommended posture: NO new ADRs land in 08.2.** All work fits inside the existing permitted-foundations set (`tokio::sync::Notify`, `std::sync::atomic::AtomicU8`). The DECISIONS.md ledger head stays at **ADR-0032** through 08.1's execution arc (assuming 08.1 lands no new ADR per the recommended posture); ADR-0033 is the next-available number at 08.2 start.

No conditional ADR slots projected. If reality forces one at 08.2 state 3 (e.g., the drain-signal observer needs `tokio::sync::watch` semantics that `Notify` doesn't cleanly express), it lands at the next-available number per D-3.5.

---

## 8. State-machine signposts for the phase-08.2 state-1 session

The session that enters 08.2 (after 08.1 reaches state 6) reads this section and acts.

- **Lifecycle state at session start:** State 1 (ROADMAP row exists at status `planned`; directory exists with this SPEC.md; PLAN.md does not exist).

  **NOTE:** The state-1 brainstorm session for 08.2 may be SKIPPED per the established cross-phase precedent — the parent-08 state-2 split commit (this commit) already brainstormed the full surface in the parent SPEC and the carve-out into 08.2's slice in this SPEC. The next session entering 08.2 may go directly to state 2 (PLAN-write) with this SPEC as input. **However, that decision rests with the state-1 / state-2 session: if the session reads STATE.md and finds `08.2` state 2 (with this SPEC already on disk and no PLAN.md), it routes to writing-plans directly; if STATE.md says state 1, it routes to brainstorming first (likely producing an SPEC.md addendum or confirming this SPEC suffices).** The 08.1 state-6 close-out commit, which advances STATE.md to point at 08.2, picks one of these dispositions.

- **Skill (if state 2 directly):** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PLAN.md` (standalone pre-Task-1 commit).
- **Split-gate re-evaluation:** §6.1 above projects ~10-13 tasks / ~810-920 LoC — comfortably under the §6.1 split-gate. **No nested split.**
- **Advance posture:** STATE.md advances to lifecycle state 3 with `next-skill = superpowers:subagent-driven-development` per the user's standing preference (auto-memory `feedback_execution_style`).

---

## 9. Commit message format (for state 6 of the phase-08.2 lifecycle — closes MVP trunk)

```
phase 08.2: endpoint-triggered drain (drain_listeners, healthcheck/fail, healthcheck/ok) + DrainState + listener observation [parent 08 done] [MVP trunk complete] [ADRs as appropriate]

<1-3 sentence summary>

Differential surface: fixture 0015-admin-drain-listeners; all 15 Docker-gated fixtures (0001-0015) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline; no H2-framing surfaces engaged.
```

Per `BOOTSTRAP_PROMPT.md` §5.3, the bracketed ADR list is omitted if no ADRs landed. The `[parent 08 done]` + `[MVP trunk complete]` annotations are project-precedent markers (mirrors 07.2's `[parent 07 done]` annotation at `1d52156`).

---

## 10. State-machine commit (the parent-08 state-2 split commit landing this SPEC)

This SPEC is created at the parent-08 state-2 split commit alongside the sibling 08.1 SPEC, ADR-0032, ROADMAP-row additions, and STATE.md advance. The commit's full file-list is documented in 08.1's SPEC §10.

**Predecessor:** `0202e38` — phase-08 state-1 SPEC.md (the parent-08 brainstorm SPEC).

---

*End of SPEC. Phase 08.2 lifecycle state 1 begins when STATE.md first points at this directory (after 08.1 reaches state 6). The session that enters 08.2 may route directly to state 2 if this SPEC suffices, or to state 1 if a brainstorm addendum is needed. With phase 08.2's state-6 close-out, ROADMAP row 08 flips done, the MVP trunk stands 00→08 all done, and the project transitions to BOOTSTRAP_PROMPT.md §9 feature-family expansion (phases 09+).*
