# Phase 08 (`08-admin-api-and-drain`) — SPEC

- **Phase id:** `08`
- **Slug:** `08-admin-api-and-drain`
- **Status before this SPEC lands:** `planned` (per `docs/envoy-rust/ROADMAP.md` at HEAD `1d52156`; the parent-07 state-6 close-out commit).
- **Charter source:** `BOOTSTRAP_PROMPT.md` §8 row 08 — *"Minimum admin API (config_dump, stats, clusters, listeners, ready, server_info) + graceful drain"*. Differential surface at phase end: *"admin + drain fixtures green."*
- **Position in the MVP trunk:** the **last MVP-trunk phase**. Phase 08 closes the trunk; with state-6 the trunk stands **00→08 all `done`** and the `BOOTSTRAP_PROMPT.md` §9 feature-family expansion begins (phases 09+).

---

## 1. Goal and acceptance signal

Phase 08 extends the existing `crates/envoy-admin/` (HCM-backed admin listener; HTTP/1.1 only; landed in 06.1; comprehensive stats wired through it in 06.3) from its 3-endpoint 06.1 surface (`/ready`, `/stats`, `/stats/prometheus`) to the **7-endpoint MVP minimum admin surface** named in the charter, AND adds **endpoint-triggered graceful-drain semantics** wired through a small shared `DrainState` observed by the data-plane listener accept loops.

**Differential surface added by phase 08:**

- **Fixture `0014-admin-config-dump-server-info`** — admin scrape against `/config_dump`, `/server_info`, `/clusters`, `/listeners` on a non-trivial bootstrap config; bilateral assertion that body shapes are equivalent under the allow-listed dispositions documented in `BEHAVIOR_CONTRACT.md`'s new "Admin endpoint body shapes" subsection.
- **Fixture `0015-admin-drain-listeners`** — admin scrape sequence: `GET /ready` (200 LIVE) → `POST /drain_listeners` (200 OK) → `GET /ready` (503 DRAINING) → `GET /server_info` (`state: DRAINING`) → new connection to the data-plane listener refused-or-immediately-closed within the documented drain window. Bilateral on both proxies.

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixtures `0014` + `0015` green at Docker-gated CI.
- **(b)** All **13 pre-existing differential fixtures** (`0001-tcp-echo` through `0013-http-filter-header-mutation`) **remain green simultaneously** at the same CI run.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%; phase 08 engages no H2-framing surfaces).
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (new seeds for the admin endpoint configs).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent — fixture inheritance is a regression vector).

**Closes the MVP trunk.** At state-6 of phase 08 (which may itself be the state-6 of a sub-phase `08.k` if phase 08 splits at state-2), ROADMAP row `08` flips `in-progress` → `done` and the trunk reaches the §9 feature-family transition.

---

## 2. Behavior-contract scope for phase 08

Phase 08 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with four authored additions, landed at the tasks where each is first empirically exercised:

### 2.1 New top-level subsection — "Admin endpoint body shapes"

One row per phase-08 endpoint, with per-endpoint disposition:

| Endpoint | Method | Body kind | Equivalence disposition |
|---|---|---|---|
| `/server_info` | GET | JSON object | Required keys `state`, `version`, `node`, `uptime_current_epoch_seconds`, `uptime_all_epochs_seconds`, `hot_restart_version`, `command_line_options`. `state` value-exact (`LIVE` / `PRE_INITIALIZING` / `DRAINING`); `node.*` value-exact from the parsed bootstrap; `version` + `hot_restart_version` + `command_line_options` allowlist-each-side (envoy-rust emits its own version string; Envoy emits its own); `uptime_*` name-required-value-may-differ (wall clock). |
| `/clusters` | GET | text/plain | Set-equal `<cluster_name>::observability_name::<name>` + `<cluster_name>::default_priority::endpoints` lines per Envoy v1.33's plain-text format. Per-endpoint numeric fields (success/error/timeout counts) name-required-value-may-differ. |
| `/listeners` | GET | text/plain | Set-equal `<listener_name>::<address>:<port>` lines. Order: sorted-by-name (deterministic). |
| `/config_dump` | GET | JSON object | Top-level shape `{ "configs": [...] }`. envoy-rust emits exactly one entry: `{ "@type": "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump", "bootstrap": <static-bootstrap-as-JSON>, "last_updated": <ISO-8601 timestamp> }`. Envoy may emit additional entries for xDS-derived configs; those land on `allowlist_envoy_only`. `bootstrap.static_resources` content value-exact-after-roundtrip (modulo serde renamings; the harness's `JsonShape::required_subtree` covers this). `last_updated` name-required-value-may-differ. |
| `/drain_listeners` | POST | empty | Status 200; empty body; effect-only endpoint. |
| `/healthcheck/fail` | POST | empty | Status 200; empty body; effect-only endpoint. |
| `/healthcheck/ok` | POST | empty | Status 200; empty body; effect-only endpoint. Restores from `HealthcheckFailing` → `Live`. `Draining` is sticky — `/healthcheck/ok` after `/drain_listeners` does NOT un-drain. |

### 2.2 Header allow-list extension

The 06.1-landed 4 admin standard headers (`cache-control: no-cache, max-age=0`, `x-content-type-options: nosniff`, `server: envoy-rust`, `date: <RFC 7231>`) gain one note: with phase 08's case-insensitive-dedupe in `serialize_response` (closes 06.1 I2), a future endpoint may legitimately set its own `cache-control` and the dedupe ensures no duplicate header lands on the wire.

### 2.3 Stat-name mapping extension

Three new rows:

- `server.live` (gauge, value-exact). `1` when `DrainState::current() == Live`; `0` otherwise. Both proxies emit on every snapshot.
- `server.state` (gauge, value-exact for the deterministic-`LIVE` harness path). Encoded enum: `Live=0`, `HealthcheckFailing=1`, `Draining=2`. Phase 08's fixtures assert the `Live=0` initial state and the post-drain `Draining=2` state.
- `listener_manager.total_listeners_active` (gauge, value-exact). Count of currently-active data-plane listeners. Decrements as listeners drain.

### 2.4 New top-level subsection — "Admin-action effect equivalence"

A small subsection (5-8 lines) stating the cross-proxy invariant: `POST /drain_listeners` on either proxy must drive that proxy to refuse-or-immediately-close new connections on its data-plane listeners within the drain window (5s `DRAIN_BUDGET`). This is the wire-level observable both proxies satisfy; the internal mechanism is implementation-specific.

---

## 3. Deliverables

Phase 08's scope is enumerated as deliverables `D1`–`D17` below. **The state-2 PLAN-writer organizes deliverables into tasks** (and evaluates the §6.1 split gate) — these are not 1:1 with tasks. Some deliverables compose into one task; some split across two. The deliverables are LISTED in roughly the order the PLAN-writer is expected to execute them, but the SPEC is not prescriptive about the order; only about the surface.

### Task 1 preamble — 06.1 admin carryforward closures (D1, D2, D3)

These three items are explicitly assigned to phase 08 by the 06.1 + 06.2 + 06.3 + 07.2 STATE.md "Phase-06.1 rollovers" subsection. They land **before** any new endpoint or new drain machinery, at the architectural seam where the new endpoints will plug in.

- **D1 — 06.1 REVIEW I2 close: `serialize_response` case-insensitive header dedupe.** At `crates/envoy-admin/src/handler.rs::serialize_response`, before injecting the 5 standard admin-response headers (`cache-control`, `x-content-type-options`, `server`, `date`, `connection`), check the rendered `Response.headers` for a case-insensitive name match. Inject the standard header only if not already present. ~10 LoC at one site. New unit test verifies a render path that sets its own `cache-control` does NOT emit a duplicate.

- **D2 — 06.1 REVIEW M1 close: `reason_for_status` helper.** Replace `resp.reason.unwrap_or("OK")` at `crates/envoy-admin/src/handler.rs::serialize_response` with a `reason_for_status(u16) -> &'static str` helper covering at minimum `200 → "OK"`, `400 → "Bad Request"`, `404 → "Not Found"`, `405 → "Method Not Allowed"`, `503 → "Service Unavailable"`, `500 → "Internal Server Error"`. New unit test verifies a `Response { status: 503, reason: None, ... }` renders the correct reason. Same site as D1.

- **D3 — 06.1 REVIEW M4 close: `DRAIN_BUDGET` constant consolidated.** Hoist `pub const DRAIN_BUDGET: Duration = Duration::from_secs(5)` from `crates/envoy-listener/src/lib.rs:135` (the older site) into a re-exported position. `crates/envoy-admin/src/handler.rs:28` imports it. Both sites collapse to one source of truth. Recommended hoist site: `envoy-listener` (the lower-layer crate; `envoy-admin` already depends on it). Three new unit-test cases verify the constant is the same numeric value at both call sites (a compile-time tautology after the hoist; the test exists to lock the lockstep).

### Admin endpoint surface (D4–D10)

- **D4 — `AdminEndpoint` method-dispatch refactor.** `crates/envoy-admin/src/endpoint.rs::AdminEndpoint::from_path` widens to `dispatch(method: &str, path: &str) -> Dispatch` where `Dispatch` is `enum { Endpoint(AdminEndpoint), NotFound, MethodNotAllowed { allow: &'static str } }`. Each `AdminEndpoint` variant carries its allowed-methods set internally (a `const ALLOWED: &str = "GET"` per variant; POST endpoints declare `"POST"`). The 06.1 existing variants (`Ready`, `Stats`, `StatsPrometheus`) keep `"GET"`. The 06.1 hand-rolled GET-only 405 path at `handler::handle_inner` collapses into the new dispatch enum — `render_405` consults the `Dispatch::MethodNotAllowed { allow }` value for the `Allow:` header. No behavior change on the existing endpoints; verified by the existing in-process backstop + fixture 0011.

- **D5 — `/server_info` endpoint.** New variant `AdminEndpoint::ServerInfo` (GET). Renders JSON via `serde_json::to_vec_pretty` of a new `ServerInfoBody` struct:
  ```rust
  #[derive(Serialize)]
  struct ServerInfoBody {
      version: String,                       // e.g. "envoy-rust 0.1.0"
      state: &'static str,                   // "LIVE" / "DRAINING" / "PRE_INITIALIZING"
      hot_restart_version: &'static str,     // "disabled" (envoy-rust does not implement hot-restart)
      command_line_options: BTreeMap<String, serde_yaml::Value>,
      node: envoy_config::Node,              // from parsed bootstrap
      uptime_current_epoch_seconds: u64,
      uptime_all_epochs_seconds: u64,        // == current_epoch (no hot-restart)
  }
  ```
  `state` is rendered from `DrainState::current()` via the mapping `Live → "LIVE"`, `HealthcheckFailing → "LIVE"` (server-state is independent of healthcheck-fail per Envoy's documented semantic; `/ready` flips to 503 but `/server_info.state` stays `"LIVE"`), `Draining → "DRAINING"`. envoy-rust does not engage the `"PRE_INITIALIZING"` state — listener bind is synchronous in `envoy-bin::main` so by the time the admin listener serves its first request, all data-plane listeners are already bound. The wire enum carries `PRE_INITIALIZING` for forward-compatibility with Envoy parity but no envoy-rust code path emits it in phase 08. `uptime_current_epoch_seconds` is the duration since `envoy-bin::main`'s start instant, threaded via a new `Arc<Instant>` startup handle. `command_line_options` is built once at startup from the parsed CLI (currently `-c <config>`); admin-listener internals can serialize this lazily on first request to avoid threading more state at construction time.

- **D6 — `/config_dump` endpoint.** New variant `AdminEndpoint::ConfigDump` (GET). Renders JSON via `serde_json::to_vec_pretty` of:
  ```rust
  #[derive(Serialize)]
  struct ConfigDumpBody { configs: Vec<ConfigDumpEntry> }
  #[derive(Serialize)]
  #[serde(tag = "@type")]
  enum ConfigDumpEntry {
      #[serde(rename = "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump")]
      Bootstrap { bootstrap: envoy_config::Bootstrap, last_updated: String },
      // xDS-derived variants intentionally absent in phase 08; the xDS family adds them.
  }
  ```
  The `Bootstrap` struct already derives `Serialize` (via `serde`) on its existing serde derives; the JSON shape is whatever `serde_json` emits over the existing field names. Per BEHAVIOR_CONTRACT subsection 2.1, the harness asserts JSON-parse + required-key presence + `bootstrap.static_resources` subtree equality after both proxies' bodies are parsed and the envoy-only `configs[]` entries (`ClustersConfigDump`, `ListenersConfigDump`, etc.) are filtered to the `BootstrapConfigDump` entry. `last_updated` is the ISO-8601 timestamp at request-render time (reuses the 06.2-landed `envoy_accesslog::default_format::format_iso8601`).

- **D7 — `/clusters` endpoint.** New variant `AdminEndpoint::Clusters` (GET). Renders plain-text per Envoy v1.33's `/clusters` format (one cluster-stanza per cluster; cluster-stanza emits `<name>::observability_name::<name>` + `<name>::default_priority::endpoints` lines + per-endpoint numeric-counter lines). Reads from a new `Arc<ClusterManager>` handle threaded from `envoy-bin::main`. The harness's new `BodyRule::TextLines { required_lines, allowlist_envoy_only, allowlist_envoy_rust_only }` asserts set-equality on the cluster-name lines + allowlist for numeric counters.

- **D8 — `/listeners` endpoint.** New variant `AdminEndpoint::Listeners` (GET). Renders plain-text per Envoy v1.33's `/listeners` default format (one line per listener: `<listener_name>::<bind_address>:<bind_port>`, sorted by name). Reads from the existing `Arc<Bootstrap>` handle (listener config is statically declared; xDS-derived listeners absent until §9 family). Harness asserts via `BodyRule::TextLines`.

- **D9 — `/drain_listeners` endpoint.** New variant `AdminEndpoint::DrainListeners` (POST). Renders 200 OK with empty body. Side effect: invokes `DrainState::drain()` on the shared `Arc<DrainState>` handle. Sticky — repeat POSTs are idempotent.

- **D10 — `/healthcheck/fail` + `/healthcheck/ok` endpoints.** Two new variants `AdminEndpoint::HealthcheckFail` (POST) + `AdminEndpoint::HealthcheckOk` (POST). Render 200 OK with empty body. Side effects: invoke `DrainState::fail_healthcheck()` / `DrainState::ok_healthcheck()` respectively. `/healthcheck/ok` after `/drain_listeners` is a no-op (sticky-drain semantics — `DrainState::ok_healthcheck()` matches `Live → Live` and `HealthcheckFailing → Live`; on `Draining` it returns without effect).

### Drain machinery (D11–D13)

- **D11 — `DrainState` module.** Lives at `crates/envoy-listener/src/drain.rs` per §5.1's Cargo-cycle resolution (re-exported from `envoy-admin::DrainState` so admin-side call sites read naturally). New module shape:
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
  - `drain()` does `compare_exchange` to set state to `Draining` and calls `notify.notify_waiters()` exactly once on the first transition.
  - `drain_signal()` is `notify.notified()` if state currently `< Draining`, or an immediately-ready future if state already `Draining`. The signal is observed by listener accept loops; observation is idempotent and re-entrant.
  - All state transitions are documented inline. `Live → HealthcheckFailing` and `HealthcheckFailing → Live` are reversible; `* → Draining` is sticky.

- **D12 — Listener observation of `drain_signal`.** Two listener accept loops gain a second future in their `tokio::select!`:
  - `crates/envoy-listener/src/lib.rs::Listener::serve` — accepts a new `drain: Arc<DrainState>` parameter (signature widened; the existing single `shutdown` future stays). The select arm `_ = drain.drain_signal() => { ... }` drives the same drain code path as the existing `_ = &mut shutdown` arm (stop accepting + 5s `DRAIN_BUDGET` for in-flight + abort stragglers).
  - `crates/envoy-admin/src/handler.rs::serve` — admin listener does **NOT** observe its own `drain_signal`. The admin listener stays serving during drain (so `/server_info` and `/stats/prometheus` remain reachable for operators and Prometheus pollers). The admin listener drains only via SIGTERM, exercised via the existing `CancellationToken` path in `envoy-bin::main`.

- **D13 — `envoy-bin::main` shared-handle wiring.** `envoy-bin::main` constructs:
  - One `Arc<DrainState>` at startup; cloned into both the admin handler (writer) and each data-plane listener accept-loop spawn (reader/observer).
  - One `Arc<Bootstrap>` (already loaded once; just clone the `Arc` into `AdminHandler::new`).
  - One `Arc<ClusterManager>` (already exists; clone into `AdminHandler::new`).
  - One `Arc<Instant>` for `uptime_*` rendering (or equivalently, an `Instant` stored on `AdminHandler` at construction).
  
  `AdminHandler::new` signature widens to `new(config: Arc<AdminConfig>, registry: Arc<StatsRegistry>, bootstrap: Arc<Bootstrap>, cluster_manager: Arc<ClusterManager>, drain: Arc<DrainState>, start_instant: Instant)`. The admin endpoint renders read from `&self` via `&handler` closures (existing pattern).

### Stats wiring (D14)

- **D14 — `server.live` + `server.state` + `listener_manager.total_listeners_active` stats.** Three new gauges registered at startup against the existing `StatsRegistry`. Updated via:
  - `server.live` ← `1` when `DrainState::Live`, else `0`. Update fires from `DrainState::current()` polled by a dedicated `tokio::spawn` task (or equivalently, on every state transition via inline `gauge.set(...)` calls in the `fail_healthcheck` / `ok_healthcheck` / `drain` methods — simpler; one source of truth).
  - `server.state` ← discriminant of `DrainStage` (0/1/2). Same update site as `server.live`.
  - `listener_manager.total_listeners_active` ← incremented at `Listener::serve` entry; decremented at `Listener::serve` epilogue (after drain completes). Mirrors the existing `listener.<name>.downstream_cx_active` gauge pattern from 06.3 (RAII guard).

### Harness extensions (D15–D16)

- **D15 — `BodyRule::JsonShape` + `BodyRule::TextLines`.** Two new variants on `tests/differential/src/lib.rs::BodyRule`:
  ```rust
  BodyRule::JsonShape {
      required_keys: Vec<String>,
      required_subtree: Option<(String, serde_yaml::Value)>,   // dotted-key path → expected value
      allowlist_envoy_only_keys: Vec<String>,
      allowlist_envoy_rust_only_keys: Vec<String>,
      value_may_differ_keys: Vec<String>,
  },
  BodyRule::TextLines {
      required_lines: Vec<String>,        // exact match
      required_line_prefixes: Vec<String>,// prefix match (for `<name>::endpoints` shapes where the suffix varies)
      allowlist_envoy_only_lines: Vec<String>,
      allowlist_envoy_rust_only_lines: Vec<String>,
  },
  ```
  Both parse the response body, build a set/map representation, and assert per-rule. `JsonShape` uses `serde_json::Value` internally; `TextLines` does line-split + per-line classification. Both produce error messages naming the offending key/line.

- **D16 — `Driver::AdminScrape` action-sequence extension.** The 06.1-landed `Driver::AdminScrape { pre_requests: Vec<...>, request: ..., expected_status, response_body, ... }` shape extends with:
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
  Fixture 0015 uses `pre_admin_actions: [Post { path: "/drain_listeners", expected_status: 200 }]` + `post_admin_assertions: [DataPlaneConnectionRefused { listener_address, within: 5s }]`.

### Fixtures (D17)

- **D17.1 — Fixture `tests/fixtures/0014-admin-config-dump-server-info/`.** Reuses fixture 0008's bootstrap shape (HCM + STRICT_DNS cluster + 1 listener) so `/config_dump` + `/clusters` + `/listeners` have non-trivial content to dump. Four admin-scrape sub-cases (one per endpoint), driven via `Driver::AdminScrape` with `BodyRule::JsonShape` for `/server_info` + `/config_dump` and `BodyRule::TextLines` for `/clusters` + `/listeners`.

- **D17.2 — Fixture `tests/fixtures/0015-admin-drain-listeners/`.** Reuses fixture 0007's bootstrap shape (HCM + direct_response) — minimal data-plane surface — so the drain assertion focuses on the listener-rejection behavior, not on upstream complexity. Single admin-scrape sequence: `pre_requests: [GET /ready → 200 LIVE]`, `pre_admin_actions: [POST /drain_listeners → 200]`, `request: GET /ready` with `expected_status: 503`, `response_body: BodyRule::ByteExact { value: "DRAINING\n" }`, `post_admin_assertions: [DataPlaneConnectionRefused { listener_address: <bound port>, within: 5s }]`.

- **D17.3 — Fuzz corpus seed extensions.** Two new YAML seeds under `crates/envoy-config/fuzz/corpus/parse_bootstrap/`: one with admin + drain-relevant bootstrap shapes (admin endpoint + multi-cluster); one with a healthcheck-related bootstrap. Mirrors the 06.1 admin-corpus-seed and 07.2 header-mutation-corpus-seed pattern.

- **D17.4 — In-process backstops.** Two new files under `crates/envoy-bin/tests/`:
  - `admin_drain_listeners.rs` — exercises the same drain flow as fixture 0015 in-process (no Docker). Asserts the listener rejects new TCP connections within 5s of `POST /drain_listeners`.
  - `admin_config_dump_server_info.rs` — exercises `/config_dump` + `/server_info` in-process. Asserts JSON-parse and required-key presence.

---

## 4. Out of scope (deferred non-goals)

Phase 08 explicitly does NOT land:

- **Admin endpoint surface beyond the 7-endpoint set.** `/quitquitquit`, `/runtime`, `/runtime_modify`, `/certs`, `/contention`, `/heapprofiler`, `/cpuprofiler`, `/hot_restart_version`, `/help`, `/init_dump`, `/reset_counters`, `/clusters?format=json`, `/listeners?format=json`, `/stats?filter=`, `/stats?usedonly`, `/server_info` POST variants, `/logging`, `/memory`, `/stats/recentlookups`, `/ready?fast` — all defer. Each is named by a feature family or a future MVP-trunk-extension phase. `/help` lands when the admin surface is stable enough to enumerate; no MVP need.
- **Admin auth / TLS.** Admin listener stays plaintext HTTP/1.1 per the 06.1 cross-sub-phase rule 3. xDS-family or a dedicated admin-hardening phase covers `--admin-address-path` socket binding / mTLS-on-admin.
- **HTTP/2 admin.** Defers indefinitely per the 06.1 architectural rule. Prometheus pollers tolerate HTTP/1.1.
- **xDS-derived config in `/config_dump`.** envoy-rust has no xDS yet; phase 08's `/config_dump` covers static bootstrap only. The xDS family extends with `ClustersConfigDump`, `ListenersConfigDump`, `RoutesConfigDump`, `ScopedRoutesConfigDump`, `SecretsConfigDump`, `EndpointsConfigDump`. Envoy-emitted xDS-config entries land on the `allowlist_envoy_only_keys` side of `JsonShape` for phase 08's fixtures.
- **Drain-time config (`drain_time_s`, `--drain-strategy`).** Defers. envoy-rust's drain budget stays fixed at 5s per the existing `DRAIN_BUDGET`. Tunable drain timing (gradual `Connection: close` injection over a configurable window) is its own design decision; not gated by MVP.
- **`Connection: close` injection during drain.** Defers. Endpoint-triggered drain is testable via listener-rejection alone; `Connection: close` on in-flight responses is a separable later feature.
- **Drain-state propagation to health-check filter responses.** Defers — envoy-rust has no health-check filter yet (the HTTP filter family is post-MVP). When the health-check filter family lands, it consumes `DrainState` for the response-code flip.
- **SIGKILL→SIGTERM of harness subject subprocess (phase-00 I3).** Continues to defer. The `nix` crate is still off the permitted-foundations list; endpoint-triggered drain does not engage signal-based termination of the subject, so the deferral remains valid through phase 08.
- **Admin-side access logging.** `Admin.access_log_path` remains parse-and-ignore per the 06.1 / ADR-0026 pattern. The xDS family or a dedicated observability phase wires it.
- **Comprehensive 4-class differential coverage of admin response codes** (06.3 REVIEW I2 carryforward — synthetic 5xx backend). Stays at the upstream-robustness family per the named owner.
- **`/server_info` hot-restart fields beyond the constant `"disabled"`.** envoy-rust does not implement hot-restart; `hot_restart_version` ships as a literal `"disabled"`. The runtime + hot restart family (§9) extends this if it ever lands.
- **Live ROADMAP-row mutation by `/drain_listeners`.** ROADMAP is project-doc-only; the runtime ROADMAP-row state-machine reasoning lives in STATE.md and is updated by the state-6 close-out, not by data-plane signals.

---

## 5. Architectural invariants

Phase 08 honors and extends the established cross-crate invariants:

### 5.1 Crate boundaries

- **`envoy-admin` stays sole-dep-owner of admin endpoints.** All new endpoint variants land in `crates/envoy-admin/src/endpoint.rs`. No new top-level crate is created.
- **`DrainState` lives in `envoy-listener`, not `envoy-admin`.** The natural placement would be `envoy-admin::drain` (admin endpoints are its only writers), but `envoy-listener::Listener::serve` must consume a typed `Arc<DrainState>` parameter for its accept-loop `tokio::select!`, which would force `envoy-listener` to depend on `envoy-admin` — and `envoy-admin` already depends on `envoy-listener::ConnectionHandler`. **That's a Cargo cycle**, structurally identical to the 05.3 (`envoy-http1` ↔ `envoy-http2`) and 07.1 (`envoy-filter` ↔ `envoy-http1`) cycles. The resolution adopted here at SPEC time: **`DrainState` lives at `crates/envoy-listener/src/drain.rs` and is re-exported from `envoy-admin::DrainState`** so admin-side call sites read as if it lives there. This is option (b) of the standard three resolution paths (other options considered and rejected: option (a) extract a new `envoy-drain` crate — extra crate for one tiny type, YAGNI; option (c) define a `trait DrainObserver` in `envoy-listener` with admin-side `impl` — extra indirection for no benefit). **No new ADR required**: the resolution mirrors the M4 `DRAIN_BUDGET` hoist exactly (both `DrainState` and `DRAIN_BUDGET` live in the same crate; both are re-exported from `envoy-admin`); ADR-0028 + ADR-0031 already documented the cycle-resolution-by-hoist posture as project doctrine.

- **HTTP/1.1-only admin** (cross-sub-phase rule 3 from 06.1). Carries forward unchanged.

### 5.2 Endpoint dispatch

- **Exact-match path routing** continues (06.1 cross-sub-phase rule 5). Query strings are stripped before dispatch; `?format=json` and `?usedonly` are NOT honored (and produce 404 on the bare `/clusters?format=json` path — Envoy parses query strings as part of the endpoint dispatch, but phase 08 does not). When the xDS family or a dedicated phase adds query-string honoring, it extends `dispatch()`'s contract.
- **Method-strict per-endpoint allowlist.** Each `AdminEndpoint` variant has exactly one allowed method. `/ready` / `/stats` / `/stats/prometheus` / `/server_info` / `/clusters` / `/listeners` / `/config_dump` are GET-only; `/drain_listeners` / `/healthcheck/fail` / `/healthcheck/ok` are POST-only. Wrong method produces 405 with the correct `Allow:` header per RFC 7231 §6.5.5.

### 5.3 No new top-level Cargo deps

The recommended no-foundations-grants posture per parent-07 SPEC §7 carries forward through phase 08. `serde_json` (D-3.2 permitted), `tokio::sync::Notify` (already used; part of `tokio`), `std::sync::atomic` (std-lib) cover the new surfaces. **If the state-3 implementer surfaces a genuine foundation need at execution time, a foundations-grant ADR lands per D-3.5 — see §7 for the conditional-ADR slots.**

### 5.4 Stats namespacing

Phase 08 stats follow Envoy's documented namespacing: `server.<stat>` for server-state metrics; `listener_manager.<stat>` for listener-manager metrics. Both are NEW namespaces (06.1 / 06.3 wired only `listener.<name>.*`, `cluster.<name>.*`, `http.<stat_prefix>.*`). Each new namespace adds one row to `BEHAVIOR_CONTRACT.md`'s `Stat-name mapping` table.

### 5.5 Admin listener stays serving during drain

Per D11 + D12, the admin listener observes shutdown but **not** its own `drain_signal`. This is the architectural invariant the fixtures' drain assertions rest on: `GET /server_info` returns 200 with `state: DRAINING` even while the data-plane listener is refusing new connections. Operator-tool reachability during drain is the whole point of the endpoint-triggered model.

### 5.6 Sticky drain

Once `DrainState::drain()` fires, the state is sticky — `/healthcheck/ok` does NOT un-drain. This matches Envoy's documented behavior and prevents fixture flakiness (an operator running multiple drain experiments would otherwise see non-deterministic re-enable behavior).

---

## 6. Implementation signposts for the planner

The state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation (read first)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 08's surface estimate at SPEC time:

- 3 carryforward-closure deliverables (D1/D2/D3) — ~50 LoC + tests, ~1 task or two co-located.
- 7 endpoint deliverables (D4–D10) — ~600-800 LoC + tests across endpoint.rs + handler.rs + new tests. ~5-7 tasks if grouped by endpoint pair.
- Drain machinery (D11/D12/D13) — ~350 LoC + tests including listener-observation wiring + shared-handle plumbing. ~2-3 tasks.
- Stats wiring (D14) — ~80 LoC + tests. ~1 task.
- Harness extensions (D15/D16) — ~200 LoC. ~2 tasks.
- Fixtures (D17.1/.2/.3/.4) — fixture YAMLs + expectations + 2 in-process backstops. ~3-4 tasks.
- State-4 verification + STATE advance — 1 task.

**SPEC-time projection: ~21-23 tasks; ~1500-1800 LoC.** The phase is **close to or at** the split-gate threshold. The state-2 PLAN-writer should evaluate concretely and split if needed. **Recommended split posture if the gate fires:**

- **08.1 — admin endpoint surface + carryforward closures + fixture 0014** (D1–D8, D14, D15, D17.1, D17.3 partial, D17.4 partial; ~1000 LoC).
- **08.2 — drain machinery + fixture 0015 + parent-08 close-out** (D9, D10, D11, D12, D13, D16, D17.2, D17.3 partial, D17.4 partial; ~800 LoC).

The split would mirror the 06.1/06.2/06.3 + 07.1/07.2 precedent: foundation slice first, integration slice second; the closing sub-phase closes the parent.

**Not recommended: nested splits.** If the split gate fires, the planner picks 08.1/08.2 and stops; neither sub-phase re-splits. Per parent-06 SPEC §5 alternative (vi).

### 6.2 Carryforward-closure ordering

D1/D2/D3 land **first**, before any new endpoint. This is the same Task-1-preamble pattern 06.3 used (Task 2 — `Http2ClusterFromHttp1Listener` parse-time gate; Task 9 — admin idle-read-timeout; Task 4 — H1 state-init tightening). The new endpoints (D4–D10) plug into the cleaned-up `serialize_response` and `dispatch` machinery; landing them in the dirty pre-D1/D2/D3 shape would force D1/D2/D3 to be touched up later. Landing them in the clean shape avoids the rework.

### 6.3 `serde_json` is a permitted foundation; usage starts here

D-3.2 permits `serde_json`; nothing in phases 00-07 has consumed it (the existing 06.1 / 06.2 work used `bytes::BytesMut` + hand-rolled emitters). Phase 08 is the first consumer. **No ADR required for first use** — D-3.2 already permits. The PLAN-write should signpost the first-use site (likely D6, `/config_dump`) and the second-use site (D5, `/server_info`).

### 6.4 `Bootstrap` struct must derive `Serialize`

`crates/envoy-config/src/bootstrap.rs::Bootstrap` currently derives `Deserialize` (loaded from YAML) but not `Serialize`. D6 requires `Serialize`; the same applies to all of its transitively-owned types (`StaticResources`, `Listener`, `Cluster`, `Admin`, `Node`, etc.). Adding `Serialize` derives is mechanical but touches many types; the PLAN should fold this into D6's task or pre-D6 as a dedicated sub-task. **No new dep; serde is already permitted.** Field renamings on the deserialize side (`#[serde(rename = "...")]`) automatically apply to serialize-side too, so the JSON output matches the YAML input semantically. **Caveat:** YAML allows duplicate keys / certain casings that JSON does not; pre-D6 the planner should sanity-check a representative bootstrap roundtrips clean.

### 6.5 The `ClusterManager` handle

`crates/envoy-cluster/` already exposes a `ClusterManager` type per the 02.1 + 05.3 architecture. Phase 08 needs an `Arc<ClusterManager>` thread from `envoy-bin::main` to the admin handler. The `ClusterManager`'s public surface for `/clusters` rendering is `.clusters() -> impl Iterator<Item = &Cluster>` (already exists or trivially extends; phase 08's PLAN-writer verifies and either uses or adds).

### 6.6 `DrainState`'s notify shape

The Notify-based signal pattern is the right primitive: it supports multiple consumers (one per listener), zero-copy notification, and is cheap to clone via `Arc`. The state-3 implementer must ensure `notify.notify_waiters()` fires *exactly once* on the first `Live → Draining` transition (use `compare_exchange` + a successful-CAS guard; do NOT call `notify_waiters` unconditionally — repeat calls work but waste cycles). On already-`Draining`, `drain_signal()` returns an immediately-ready future (check state before calling `notified()` and `return future::ready(())` on the early-return branch).

### 6.7 Pre-state-4 fmt discipline

Per 06.1 REVIEW §7 R-9 (continuing the post-state-4 fmt-drift catch precedent), per-task PROGRESS sections should run `cargo fmt --all -- --check` at every task close, NOT just at state-4. The 07.1 + 07.2 PROGRESS attestation pattern (per-task quoting of the 5 stable-toolchain gates including `cargo fmt`) carries forward.

### 6.8 State-4 evidence-discipline

Per the 05.3 REVIEW I3 → 06.1 / 06.2 / 06.3 closure chain (each closed at a real CI URL + HEAD SHA + completion timestamp + per-gate quoted evidence), phase 08's state-4 verification must materialize a real CI run + per-gate evidence in PROGRESS.md. The 07.2 Task 10 commit `f921fdd` shape is the most recent precedent.

### 6.9 Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 (Cargo.lock cadence ratification ADR) carries forward unchanged through phase 08 if no new top-level Cargo deps are added (the recommended posture). The Cargo.lock diff at the phase-08 reviewed range is expected to be minimal (workspace-internal path-dep registrations only). If a foundations-grant ADR fires (conditional ADR-0032), the cadence pick is forced and must land alongside.

---

## 7. ADR projection

**Recommended posture: NO new ADRs land in phase 08.** The work fits inside the existing permitted-foundations set per §5.3 above. The DECISIONS.md ledger head stays at **ADR-0031** through phase 08's state-1 (this) commit; the next-available number is **ADR-0032**.

Two conditional ADR slots stay reserved-available for state-3 / state-2 execution-time landing if reality forces them:

- **Conditional ADR-0032 — phase-08 split decision.** Lands at the state-2 split commit if the §6.1 split-gate fires per §6.1 of this SPEC. Mirrors ADR-0020 (phase 04 split) / ADR-0022 (phase 05 split) / ADR-0029 (phase 06 split) / ADR-0030 (phase 07 split) — same shape, same provenance. If state-2 does not split, ADR-0032 stays reserved-available for phase 09+.

- **Conditional ADR-0033 — foundations grant (only if `/config_dump` requires it).** Lands at the task where the implementer surfaces a materially-worse-than-foundation result for JSON serialization (e.g., a need for `prost-reflect` or `protobuf-json` to reproduce Envoy's protobuf-JSON shape exactly). Recommended posture per §5.3: it does NOT fire. envoy-rust's `/config_dump` is documented as semantic-equal modulo allow-list per BEHAVIOR_CONTRACT subsection 2.1, so byte-shape divergence from Envoy's proto-JSON is contract-loosened, not contract-violating. If ADR-0033 lands, it numbers AFTER ADR-0032 (if the split also fires) or AT ADR-0032 (if the split does not fire); the actual numbering happens at landing time per D-3.5 append-only discipline.

If both conditional ADRs land in lex-then-execution order (split first at state-2; foundations grant later at state-3), the ledger advances `ADR-0031 → ADR-0032 (split) → ADR-0033 (foundations)`. If only the split lands, advances to ADR-0032. If only the foundations grant lands (no split), advances to ADR-0032 (the grant takes the next-available number).

---

## 8. State-machine signposts for the phase-08 state-2 session

The next session (state 2) reads this section and acts.

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `docs/envoy-rust/phases/08-admin-api-and-drain/PLAN.md` (standalone pre-Task-1 commit per the 04.3 / 05.1 / 06.1 / 06.2 / 06.3 / 07.1 / 07.2 PLAN-write cadence).
- **Split-gate evaluation:** §6.1 above. If the gate fires, the state-2 session writes ADR-0032 + the parent-08 split commit (creates `docs/envoy-rust/phases/08.1-...`/`08.2-...` directories with their own SPECs derived from this one; updates ROADMAP rows; advances STATE.md to `08.1` state 2). If the gate does NOT fire, the state-2 session lands a single PLAN.md at this directory and advances STATE.md to lifecycle state 3 with `next-skill = superpowers:subagent-driven-development` per the user's standing preference (auto-memory `feedback_execution_style`).
- **Bootstrap-struct `Serialize` derive sanity-check:** §6.4 above. The PLAN should treat this as either a dedicated sub-task or a D6 prerequisite.
- **`DrainState` placement decided at SPEC time:** §5.1 above settles the location as `crates/envoy-listener/src/drain.rs` with re-export from `envoy-admin`. No PLAN-time hedge needed; the planner can write the D11 task with the location pre-committed. No new ADR.

---

## 9. Commit message format (for state 6 of the phase-08 lifecycle)

If phase 08 lands as a single phase (no split):

```
phase 08: admin API (config_dump, clusters, listeners, server_info, drain_listeners, healthcheck) + endpoint-triggered drain

<1-3 sentence summary>

Differential surface: fixtures 0014-admin-config-dump-server-info + 0015-admin-drain-listeners; all 15 Docker-gated fixtures (0001-0015) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline; no H2-framing surfaces engaged.
```

If phase 08 splits at state-2 into 08.1 + 08.2:

- State-6 of 08.1: `phase 08.1: <subtitle> [ADR-0032 if split-decision fires]`
- State-6 of 08.2: `phase 08.2: <subtitle> [parent 08 done] [ADRs as appropriate]`

Per `BOOTSTRAP_PROMPT.md` §5.3, the bracketed ADR list is omitted if no ADRs landed in the phase.

---

## 10. State-machine commit (this commit — phase-08 state-1 close-out)

This SPEC is the state-1 output. The state-1 close-out commit is **docs-only** and touches:

- **CREATE** `docs/envoy-rust/phases/08-admin-api-and-drain/SPEC.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flips row `08` `planned` → `in-progress` (a phase enters `in-progress` when STATE.md points at it AND its directory is created — both true at this commit).
- **MODIFY** `docs/envoy-rust/STATE.md` — advances active phase block from `08` state 1 to `08` state 2; rewrites "Next expected skill" to `superpowers:writing-plans` scoped to this SPEC; rewrites "Last commit"; rewrites "Last updated".

No code changes, no fixture changes, no Cargo.toml changes, no DECISIONS.md changes, no BEHAVIOR_CONTRACT.md changes. The ledger head stays at **ADR-0031**.

**Commit message:**

```
phase 08: state-1 SPEC.md (admin API + endpoint-triggered drain)
```

Per the project precedent (07.2 state-2 standalone PLAN commit `c7dea4c` shape; 07.1 state-1 SPEC commit shape; 06.3 state-2 PLAN commit `3a964cc` shape), state-1 commits are short and self-explanatory.

**Predecessor:** `1d52156` — phase 07.2 state-6 phase-done close-out + parent-07 close-out.

**Origin/main:** `20a393d` (the 07.2 Task 9 commit; the CI evidence anchor for parent-07 close). Three docs-only commits sit unpushed on local `main` (`f921fdd`, `ab01755`, `1d52156`). Per the established precedent, accumulated docs-only state commits ride to `origin` at the next phase's first code commit / CI gate; the phase-08 state-1 SPEC commit is itself docs-only and may also accumulate locally. Pushing is not required at state 1.

---

*End of SPEC. Phase 08 state-1 lifecycle complete on landing. The next session enters state 2 — writes PLAN.md per `superpowers:writing-plans` and evaluates the §6.1 split gate.*
