# Phase 08.1 (`08.1-admin-endpoint-surface`) — SPEC

- **Phase id:** `08.1`
- **Slug:** `08.1-admin-endpoint-surface`
- **Status before this SPEC lands:** `planned` (this SPEC's landing commit is the parent-08 state-2 split commit, which creates the row at status `planned` and immediately advances STATE.md to point at `08.1` lifecycle state 2; the row flips `planned` → `in-progress` at this commit per `BOOTSTRAP_PROMPT.md` §4.1 invariant 3).
- **Parent:** `docs/envoy-rust/phases/08-admin-api-and-drain/SPEC.md` (committed at HEAD `0202e38`; the parent-08 state-1 brainstorm SPEC). This sub-phase SPEC is a **surface-slice carve-out** of the parent SPEC — no new brainstorming session was needed because the parent SPEC §6.1 explicitly named the recommended split shape; this SPEC operationalizes that shape.
- **Sibling:** `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/SPEC.md` (created in the same split commit). 08.2 ships AFTER 08.1 in execution order; the ordering invariant + interaction surface are documented in §5 below.
- **Charter source:** parent-08 SPEC §6.1 recommended split-shape line: *"08.1 — admin endpoint surface + carryforward closures + fixture 0014 (~1000 LoC): D1 (06.1 I2 dedupe) + D2 (06.1 M1 reason_for_status) + D3 (06.1 M4 DRAIN_BUDGET hoist) + D4 (dispatch refactor) + D5 (/server_info) + D6 (/config_dump — includes Bootstrap Serialize derive) + D7 (/clusters) + D8 (/listeners) + D14 (stats wiring …; the planner may split D14 across 08.1/08.2 by stat-family) + D15 (JsonShape + TextLines harness rules) + D17.1 (fixture 0014) + partial D17.3 + partial D17.4."* This SPEC ratifies the shape and concretely allocates each deliverable to either 08.1 or 08.2.

---

## 1. Goal and acceptance signal

Phase 08.1 expands `crates/envoy-admin/` from its 3-endpoint 06.1 surface (`/ready`, `/stats`, `/stats/prometheus`) to a **7-endpoint GET-only admin surface** by adding **4 new GET endpoints** (`/server_info`, `/config_dump`, `/clusters`, `/listeners`) and the dispatch/serialization infrastructure that 08.2's POST endpoints (`/drain_listeners`, `/healthcheck/fail`, `/healthcheck/ok`) plug into. 08.1 closes the three 06.1 admin carryforward findings (REVIEW I2 / M1 / M4) as a Task-1 preamble so the new endpoints land on the cleaned-up `serialize_response` + dispatch surface. 08.1 does NOT engage drain semantics — the 4 new endpoints render against static bootstrap config (`/config_dump`, `/listeners`), the existing `Arc<ClusterManager>` (`/clusters`), and a hardcoded `state: "LIVE"` placeholder for `/server_info`'s `state` field. 08.2 rewires `/server_info`'s `state` to `DrainState::current()` when the drain machinery lands.

**Differential surface added by phase 08.1:**

- **Fixture `0014-admin-config-dump-server-info`** — admin scrape against `/config_dump`, `/server_info`, `/clusters`, `/listeners` on a non-trivial bootstrap config (reuses fixture 0008's HCM + STRICT_DNS + 1-listener shape). Bilateral assertion that body shapes are equivalent under the allow-listed dispositions documented in `BEHAVIOR_CONTRACT.md`'s new "Admin endpoint body shapes" subsection (4 rows landed at 08.1; 3 rows for POST endpoints land at 08.2).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0014-admin-config-dump-server-info` green at Docker-gated CI.
- **(b)** All **13 pre-existing differential fixtures** (`0001-tcp-echo` through `0013-http-filter-header-mutation`) **remain green simultaneously** at the same CI run.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%; phase 08.1 engages no H2-framing surfaces).
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (new seed for the admin-multi-endpoint bootstrap shape, D17.3a).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** at 08.1's state-4 verification (continues the project precedent — fixture inheritance is a regression vector). 08.1's state-6 close-out does NOT flip the parent-08 ROADMAP row (the closing sub-phase 08.2 owns parent-08 close-out per the project's "closing-sub-phase invariant").

---

## 2. Behavior-contract scope for phase 08.1

Phase 08.1 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with two authored additions, landed at the tasks where each is first empirically exercised. The remaining contract extensions named by the parent-08 SPEC §2 land at 08.2.

### 2.1 New top-level subsection — "Admin endpoint body shapes" (4 of 7 rows; 08.1 ships GET endpoints only)

One row per phase-08.1 endpoint, with per-endpoint disposition:

| Endpoint | Method | Body kind | Equivalence disposition |
|---|---|---|---|
| `/server_info` | GET | JSON object | Required keys `state`, `version`, `node`, `uptime_current_epoch_seconds`, `uptime_all_epochs_seconds`, `hot_restart_version`, `command_line_options`. `state` value-exact (08.1 emits the constant `"LIVE"`; 08.2 extends to `LIVE` / `DRAINING`); `node.*` value-exact from the parsed bootstrap; `version` + `hot_restart_version` + `command_line_options` allowlist-each-side (envoy-rust emits its own version string; Envoy emits its own); `uptime_*` name-required-value-may-differ (wall clock). |
| `/clusters` | GET | text/plain | Set-equal `<cluster_name>::observability_name::<name>` + `<cluster_name>::default_priority::endpoints` lines per Envoy v1.33's plain-text format. Per-endpoint numeric fields (success/error/timeout counts) name-required-value-may-differ. |
| `/listeners` | GET | text/plain | Set-equal `<listener_name>::<address>:<port>` lines. Order: sorted-by-name (deterministic). |
| `/config_dump` | GET | JSON object | Top-level shape `{ "configs": [...] }`. envoy-rust emits exactly one entry: `{ "@type": "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump", "bootstrap": <static-bootstrap-as-JSON>, "last_updated": <ISO-8601 timestamp> }`. Envoy may emit additional entries for xDS-derived configs; those land on `allowlist_envoy_only`. `bootstrap.static_resources` content value-exact-after-roundtrip (modulo serde renamings; the harness's `JsonShape::required_subtree` covers this). `last_updated` name-required-value-may-differ. |

The 3 POST-endpoint rows (`/drain_listeners`, `/healthcheck/fail`, `/healthcheck/ok`) land at 08.2 alongside their endpoint implementations.

### 2.2 Header allow-list extension — dedupe note

The 06.1-landed 4 admin standard headers (`cache-control: no-cache, max-age=0`, `x-content-type-options: nosniff`, `server: envoy-rust`, `date: <RFC 7231>`) gain one note: with phase 08.1's case-insensitive-dedupe in `serialize_response` (D1, closes 06.1 I2), a future endpoint may legitimately set its own `cache-control` and the dedupe ensures no duplicate header lands on the wire. The note lands at D1's task as a one-line BEHAVIOR_CONTRACT addition.

### 2.3 What 08.1 does NOT land

The remaining parent-08 SPEC §2 contract extensions land at 08.2:

- Subsection 2.1 rows for `/drain_listeners`, `/healthcheck/fail`, `/healthcheck/ok` (effect-only POST endpoints).
- Subsection 2.3 Stat-name mapping extension (3 new rows — `server.live`, `server.state`, `listener_manager.total_listeners_active`). These stats read from `DrainState` (lands at 08.2) and from listener-observation hooks (also 08.2); 08.1 ships ZERO new stats.
- Subsection 2.4 "Admin-action effect equivalence" (drain-related cross-proxy invariant).

08.1's state-5 REVIEW does NOT block on these — they are 08.2's surface.

---

## 3. Deliverables

Phase 08.1's scope is enumerated as deliverables `D1`–`D8` + `D13a` + `D15` + `D17.1` + `D17.3a` + `D17.4a` below. The state-3 PLAN-write session (the next session after this split commit) organizes deliverables into tasks — these are not 1:1 with tasks. Some compose into one task; some split across two. Deliverables are listed in roughly execution order, but the SPEC is not prescriptive about ordering beyond the Task-1 preamble (§3 below).

### Task 1 preamble — 06.1 admin carryforward closures (D1, D2, D3)

These three items are explicitly assigned to phase 08 by the 06.1 + 06.2 + 06.3 + 07.2 STATE.md "Phase-06.1 rollovers" subsection. 08.1 inherits them per the parent-08 SPEC §3 Task-1-preamble rule: they land **before** any new endpoint at the architectural seam where the new endpoints will plug in.

- **D1 — 06.1 REVIEW I2 close: `serialize_response` case-insensitive header dedupe.** At `crates/envoy-admin/src/handler.rs::serialize_response`, before injecting the 5 standard admin-response headers (`cache-control`, `x-content-type-options`, `server`, `date`, `connection`), check the rendered `Response.headers` for a case-insensitive name match. Inject the standard header only if not already present. ~10 LoC at one site. New unit test verifies a render path that sets its own `cache-control` does NOT emit a duplicate.

- **D2 — 06.1 REVIEW M1 close: `reason_for_status` helper.** Replace `resp.reason.unwrap_or("OK")` at `crates/envoy-admin/src/handler.rs::serialize_response` with a `reason_for_status(u16) -> &'static str` helper covering at minimum `200 → "OK"`, `400 → "Bad Request"`, `404 → "Not Found"`, `405 → "Method Not Allowed"`, `503 → "Service Unavailable"`, `500 → "Internal Server Error"`. New unit test verifies a `Response { status: 503, reason: None, ... }` renders the correct reason. Same site as D1.

- **D3 — 06.1 REVIEW M4 close: `DRAIN_BUDGET` constant consolidated.** Hoist `pub const DRAIN_BUDGET: Duration = Duration::from_secs(5)` from `crates/envoy-listener/src/lib.rs:165` (the older site) into a re-exported position. `crates/envoy-admin/src/handler.rs:28` imports it. Both sites collapse to one source of truth. Recommended hoist site: `envoy-listener` (the lower-layer crate; `envoy-admin` already depends on it). Three new unit-test cases verify the constant is the same numeric value at both call sites (a compile-time tautology after the hoist; the test exists to lock the lockstep).

### Admin dispatch refactor (D4)

- **D4 — `AdminEndpoint` method-dispatch refactor.** `crates/envoy-admin/src/endpoint.rs::AdminEndpoint::from_path` widens to `dispatch(method: &str, path: &str) -> Dispatch` where `Dispatch` is `enum { Endpoint(AdminEndpoint), NotFound, MethodNotAllowed { allow: &'static str } }`. Each `AdminEndpoint` variant carries its allowed-methods set internally (a `const ALLOWED: &str = "GET"` per variant; 08.2's POST endpoints declare `"POST"`). The 06.1 existing variants (`Ready`, `Stats`, `StatsPrometheus`) keep `"GET"`. The 06.1 hand-rolled GET-only 405 path at `handler::handle_inner` collapses into the new dispatch enum — `render_405` consults the `Dispatch::MethodNotAllowed { allow }` value for the `Allow:` header. **No behavior change on the existing endpoints; verified by the existing in-process backstop + fixture 0011.** Closes 06.1 REVIEW M1 as a side effect (every endpoint now has its 405-method-allowlist surface declared structurally).

### Admin endpoint surface — 4 new GET endpoints (D5–D8)

- **D5 — `/server_info` endpoint.** New variant `AdminEndpoint::ServerInfo` (GET). Renders JSON via `serde_json::to_vec_pretty` of a new `ServerInfoBody` struct:
  ```rust
  #[derive(Serialize)]
  struct ServerInfoBody {
      version: String,                       // e.g. "envoy-rust 0.1.0"
      state: &'static str,                   // 08.1 hardcodes "LIVE"; 08.2 rewires to DrainState mapping
      hot_restart_version: &'static str,     // "disabled" (envoy-rust does not implement hot-restart)
      command_line_options: BTreeMap<String, serde_yaml::Value>,
      node: envoy_config::Node,              // from parsed bootstrap
      uptime_current_epoch_seconds: u64,
      uptime_all_epochs_seconds: u64,        // == current_epoch (no hot-restart)
  }
  ```
  **08.1 emits `state: "LIVE"` as a literal.** 08.2 (its D5-extension task) patches this site to read from `DrainState::current()` per the mapping `Live → "LIVE"`, `HealthcheckFailing → "LIVE"`, `Draining → "DRAINING"`. The structural shape (the `state: &'static str` field) is identical pre/post-extension; 08.2's change is a swap of the value-binding source, not a struct change. `uptime_current_epoch_seconds` is the duration since `envoy-bin::main`'s start instant, threaded via a new `Arc<Instant>` startup handle (part of D13a, see below). `command_line_options` is built once at startup from the parsed CLI (currently `-c <config>`); admin-listener internals can serialize this lazily on first request to avoid threading more state at construction time.

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
  D6 includes the mechanical `Serialize` derive cascade on `Bootstrap` and its transitively-owned types (`StaticResources`, `Listener`, `Cluster`, `Admin`, `Node`, etc.). Existing `#[serde(rename = "...")]` field renamings on the deserialize side automatically apply to the serialize side; the JSON output matches the YAML input semantically. Per parent-08 SPEC §6.4, the planner should sanity-check that a representative bootstrap roundtrips clean (YAML→struct→JSON→struct) before wiring `/config_dump`. **No new dep; serde is already permitted (D-3.2).** Per BEHAVIOR_CONTRACT subsection 2.1, the harness asserts JSON-parse + required-key presence + `bootstrap.static_resources` subtree equality after both proxies' bodies are parsed and the envoy-only `configs[]` entries (`ClustersConfigDump`, `ListenersConfigDump`, etc.) are filtered to the `BootstrapConfigDump` entry. `last_updated` is the ISO-8601 timestamp at request-render time (reuses the 06.2-landed `envoy_accesslog::default_format::format_iso8601`).

- **D7 — `/clusters` endpoint.** New variant `AdminEndpoint::Clusters` (GET). Renders plain-text per Envoy v1.33's `/clusters` format (one cluster-stanza per cluster; cluster-stanza emits `<name>::observability_name::<name>` + `<name>::default_priority::endpoints` lines + per-endpoint numeric-counter lines). Reads from a new `Arc<ClusterManager>` handle threaded from `envoy-bin::main` (part of D13a). The harness's new `BodyRule::TextLines { required_lines, allowlist_envoy_only, allowlist_envoy_rust_only }` asserts set-equality on the cluster-name lines + allowlist for numeric counters. The `ClusterManager`'s public surface for `/clusters` rendering is `.clusters() -> impl Iterator<Item = &Cluster>` (already exists or trivially extends per parent-08 SPEC §6.5).

- **D8 — `/listeners` endpoint.** New variant `AdminEndpoint::Listeners` (GET). Renders plain-text per Envoy v1.33's `/listeners` default format (one line per listener: `<listener_name>::<bind_address>:<bind_port>`, sorted by name). Reads from the existing `Arc<Bootstrap>` handle threaded via D13a (listener config is statically declared; xDS-derived listeners absent until §9 family). Harness asserts via `BodyRule::TextLines`.

### Shared-handle wiring partial (D13a)

- **D13a — `envoy-bin::main` shared-handle wiring (08.1 portion).** Three of the four handles named in parent-08 SPEC §3 D13 land at 08.1:
  - `Arc<Bootstrap>` (already loaded once in `envoy-bin::main`; just clone the `Arc` into `AdminHandler::new`).
  - `Arc<ClusterManager>` (already exists; clone into `AdminHandler::new`).
  - `Arc<Instant>` (or equivalently, an `Instant` stored on `AdminHandler` at construction) for `uptime_*` rendering in `/server_info`.
  
  The fourth handle, `Arc<DrainState>`, lands at 08.2's D13b alongside D11.
  
  `AdminHandler::new` signature widens at 08.1 from `new(config: Arc<AdminConfig>, registry: Arc<StatsRegistry>)` to `new(config: Arc<AdminConfig>, registry: Arc<StatsRegistry>, bootstrap: Arc<Bootstrap>, cluster_manager: Arc<ClusterManager>, start_instant: Instant)`. 08.2's D13b adds the fifth parameter `drain: Arc<DrainState>` at the end. The admin endpoint renders read from `&self` via `&handler` closures (existing 06.1 pattern).

### Harness extensions (D15)

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
  Both parse the response body, build a set/map representation, and assert per-rule. `JsonShape` uses `serde_json::Value` internally; `TextLines` does line-split + per-line classification. Both produce error messages naming the offending key/line. 08.2's `Driver::AdminScrape` extension (D16) does not require changes to these `BodyRule` variants — they cover both phase-08.1 and phase-08.2 fixture shapes.

### Fixture (D17.1) + corpus seed (D17.3a) + backstop (D17.4a)

- **D17.1 — Fixture `tests/fixtures/0014-admin-config-dump-server-info/`.** Reuses fixture 0008's bootstrap shape (HCM + STRICT_DNS cluster + 1 listener) so `/config_dump` + `/clusters` + `/listeners` have non-trivial content to dump. Four admin-scrape sub-cases (one per endpoint), driven via the existing 06.1-landed `Driver::AdminScrape` (no new harness driver variant required — the existing variant suffices for GET-only endpoints) with `BodyRule::JsonShape` for `/server_info` + `/config_dump` and `BodyRule::TextLines` for `/clusters` + `/listeners`. Fixture files: `envoy.yaml`, `envoy-rust.yaml`, `inputs/payload.bin` (0-byte placeholder), `expectations.yaml`, `README.md`. Plus the Docker-gated wrapper `tests/differential/tests/admin_config_dump_server_info.rs`.

- **D17.3a — Fuzz corpus seed extension.** One new YAML seed under `crates/envoy-config/fuzz/corpus/parse_bootstrap/`: `admin_multi_endpoint_bootstrap.yaml` with admin + multi-cluster + multi-listener shape (the bootstrap shape fixture 0014 needs). Mirrors the 06.1 admin-corpus-seed and 07.2 header-mutation-corpus-seed pattern. 08.2's D17.3b adds a healthcheck-relevant seed.

- **D17.4a — In-process backstop.** One new file `crates/envoy-bin/tests/admin_config_dump_server_info.rs` — exercises `/config_dump` + `/server_info` + `/clusters` + `/listeners` in-process (no Docker). Asserts JSON-parse + required-key presence on `/config_dump` + `/server_info`; asserts line-presence on `/clusters` + `/listeners`. 08.2's D17.4b adds `admin_drain_listeners.rs` for the drain flow.

---

## 4. Out of scope (deferred to 08.2 or beyond)

Phase 08.1 explicitly does NOT land:

- **Drain machinery (D9, D10, D11, D12).** `/drain_listeners`, `/healthcheck/fail`, `/healthcheck/ok` POST endpoints; `DrainState` module; listener observation of `drain_signal`. All defer to 08.2.
- **Drain-related stats (D14: `server.live`, `server.state`, `listener_manager.total_listeners_active`).** Defer to 08.2 alongside DrainState.
- **`Arc<DrainState>` shared-handle wiring (D13b).** Defers to 08.2.
- **`Driver::AdminScrape` `pre_admin_actions` / `post_admin_assertions` extensions (D16).** Defers to 08.2 (used by fixture 0015's drain assertion).
- **Fixture `0015-admin-drain-listeners` (D17.2).** Defers to 08.2.
- **`/ready` drain-aware response (503 DRAINING).** Defers to 08.2. 08.1 leaves 06.1's existing `/ready` semantics (200 LIVE / 503 not-ready) unchanged.
- **Parent-08 ROADMAP-row close-out.** Defers to 08.2 per the closing-sub-phase invariant.

Beyond phase 08 (deferred to feature families per parent-08 SPEC §4, summarized for cross-reference):

- **Admin endpoint surface beyond the 7-endpoint set** (`/quitquitquit`, `/runtime`, `/certs`, `/logging`, `/memory`, etc.) — defers.
- **Admin auth / TLS** — defers (06.1 cross-sub-phase rule 3).
- **HTTP/2 admin** — defers indefinitely.
- **xDS-derived config in `/config_dump`** — defers to xDS family.
- **Admin-side access logging** — `Admin.access_log_path` remains parse-and-ignore per the 06.1 / ADR-0026 pattern.
- **Query-string honoring** — `?format=json`, `?usedonly`, `?filter=` defer.

---

## 5. Architectural invariants

Phase 08.1 honors and extends the established cross-crate invariants:

### 5.1 Crate boundaries

- **`envoy-admin` stays sole-dep-owner of admin endpoints.** All 4 new endpoint variants land in `crates/envoy-admin/src/endpoint.rs`. No new top-level crate is created.
- **HTTP/1.1-only admin** (cross-sub-phase rule 3 from 06.1). Carries forward unchanged.
- **No new top-level Cargo deps.** `serde_json` (D-3.2 permitted) handles `/config_dump` + `/server_info`. No foundations grants required.

### 5.2 Endpoint dispatch

- **Exact-match path routing** continues (06.1 cross-sub-phase rule 5). Query strings stripped before dispatch.
- **Method-strict per-endpoint allowlist** (D4). 08.1's 7 endpoints are GET-only (`/ready` + `/stats` + `/stats/prometheus` from 06.1 + the 4 new ones from D5–D8). 08.2 adds 3 POST-only endpoints.
- **The dispatch enum surface 08.1 lands (`Dispatch::{Endpoint, NotFound, MethodNotAllowed}`) is what 08.2's POST endpoints plug into.** Adding 08.2's POST variants is a pure additive extension (new `AdminEndpoint` variants with `ALLOWED = "POST"`); no further refactor of dispatch.

### 5.3 `Bootstrap` Serialize-derive scope

Per parent-08 SPEC §6.4, the `Bootstrap` struct (in `crates/envoy-config/src/bootstrap.rs`) currently derives only `Deserialize`. D6 requires `Serialize` on `Bootstrap` and all its transitively-owned types. The derive cascade is mechanical (a `#[derive(Serialize)]` addition next to each `#[derive(Deserialize)]`) but touches many types. **Pre-D6 sanity check (per parent-08 SPEC §6.4):** the planner should verify a representative bootstrap roundtrips clean — YAML → `Bootstrap` → JSON via `serde_json` → `Bootstrap` via `serde_json::from_str` — before wiring D6. Field renamings (`#[serde(rename = "...")]`) automatically apply to both sides. **Caveat:** YAML allows duplicate keys / certain casings that JSON does not; the sanity-check catches surprises here. **No new dep** — serde is already permitted via D-3.2.

### 5.4 `/server_info` state field surface (08.1 placeholder; 08.2 rewires)

08.1 emits `state: "LIVE"` as a literal `&'static str`. This is the structural-shape commitment: the `state` field on `ServerInfoBody` IS a `&'static str` (or `String`; the planner picks at PLAN-time). 08.2's D5-extension task swaps the value-binding source from the literal to `match drain.current() { Live | HealthcheckFailing => "LIVE", Draining => "DRAINING" }`. **The struct shape does NOT change at the 08.1 → 08.2 boundary.** The harness's `BodyRule::JsonShape` for fixture 0014's `/server_info` row asserts `state == "LIVE"` value-exact under 08.1's deterministic-non-drain harness; the same assertion holds at 08.2's fixture 0014 re-run because fixture 0014 never invokes `/drain_listeners`.

### 5.5 Stats namespacing — no new stats in 08.1

Phase 08.1 wires ZERO new stats. The three parent-08 SPEC §3 D14 stats (`server.live`, `server.state`, `listener_manager.total_listeners_active`) all require either `DrainState` (which lands at 08.2) or listener-observation hooks (also 08.2). 08.1's existing stats surface (06.1's `listener.<name>.downstream_cx_total` / `cluster.<name>.upstream_cx_total` / `http.<stat_prefix>.downstream_rq_total` plus 06.3's comprehensive set) carries forward unchanged.

### 5.6 The admin handler reuses the 06.1-established 4 response headers

The 06.1 `serialize_response` 4-header set (`cache-control: no-cache, max-age=0`, `x-content-type-options: nosniff`, `server: envoy-rust`, `date: <RFC 7231>`) covers all 08.1 endpoints. D5/D6 (JSON-emitting endpoints) set `content-type: application/json` per-endpoint BEFORE `serialize_response` injects the 4 standard headers; D1's case-insensitive dedupe ensures no `content-type` collision (the standard set does not include `content-type`). D7/D8 (text-emitting endpoints) set `content-type: text/plain` per-endpoint; same dedupe applies.

---

## 6. Implementation signposts for the planner

The state-3 (PLAN-write) session reads this section to drive PLAN structure.

### 6.1 Split-gate re-evaluation (read first)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-3 PLAN-write evaluates whether 08.1's PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 08.1's surface estimate at split-commit time:

- 3 carryforward-closure deliverables (D1/D2/D3) — ~50 LoC + tests, ~1 task or two co-located.
- 1 dispatch refactor (D4) — ~80 LoC + tests, ~1 task.
- 4 endpoint deliverables (D5–D8) — ~500-600 LoC + tests across endpoint.rs + handler.rs + new tests + `Bootstrap` Serialize cascade. ~4-5 tasks.
- Shared-handle wiring (D13a) — ~50 LoC + tests, ~1 task (or folded into D5 prerequisites).
- Harness extension (D15) — ~150 LoC, ~1 task.
- Fixture (D17.1) + corpus seed (D17.3a) + backstop (D17.4a) — ~250 LoC, ~2-3 tasks.
- State-4 verification + STATE advance — 1 task.

**Projection: ~12-14 tasks; ~1080-1180 LoC.** **Comfortably under the §6.1 split-gate** with healthy drift headroom (~25-30% LoC headroom; ~45% task-count headroom). **No nested split projected** — the planner lands a standalone PLAN.md per the 04.3 / 05.1 / 06.1 / 06.2 / 06.3 / 07.1 / 07.2 standardized standalone-pre-Task-1-commit posture. Per parent-08 SPEC §6.1 alternative (vi) ("Not recommended: nested splits"), the gate would have to fire materially (>15% over) before a nested split is considered; this projection is under by margin.

### 6.2 Carryforward-closure ordering

D1/D2/D3 land **first**, before any new endpoint. Same Task-1-preamble pattern as 06.3 Task 2 (`ConfigError::Http2ClusterFromHttp1Listener` validator gate) and 05.1 Task 1 (`ClusterType::StrictDns` schema). The new endpoints (D4-D8) plug into the cleaned-up `serialize_response` and `dispatch` machinery; landing them in the dirty pre-D1/D2/D3 shape would force D1/D2/D3 to be touched up later.

### 6.3 D4 ordering — refactor BEFORE new variants

D4 (dispatch refactor) lands BEFORE D5/D6/D7/D8 (new endpoint variants). Reason: D4 widens the dispatch signature from `from_path(&str) -> AdminEndpoint` to `dispatch(&str, &str) -> Dispatch`; D5/D6/D7/D8 each add a new `AdminEndpoint` variant with its `const ALLOWED: &str = "GET"`. Landing D4 first means D5/D6/D7/D8 each add exactly one variant + its match arm; landing D4 after means each of D5/D6/D7/D8 has to be retroactively touched at D4 time. **No diamond reorder.** Suggested order: D1 → D2 → D3 (Task 1 preamble) → D4 (dispatch refactor) → D6 (`/config_dump` — first JSON endpoint; lands `Bootstrap` Serialize cascade as its prerequisite sub-step) → D5 (`/server_info` — second JSON endpoint; reuses the serde_json infrastructure D6 wired) → D7 (`/clusters`) → D8 (`/listeners`) → D13a (shared-handle wiring; may be folded into D5's prerequisites) → D15 (harness) → D17.1 (fixture) → D17.3a (corpus seed) → D17.4a (backstop) → state-4.

The planner may reorder D5/D6/D7/D8 freely as long as the dispatch enum and the shared-handle wiring (D13a) are in place before each endpoint's task. D6 leading is recommended (the `Bootstrap` Serialize cascade is a known mechanical risk surface; landing it first reveals any field-renaming or YAML-vs-JSON-roundtrip surprises before they compound with `/server_info` or `/clusters`/`/listeners` work).

### 6.4 `Bootstrap` Serialize derive: pre-D6 sanity check

Per parent-08 SPEC §6.4 + §5.3 above. The planner should add a sub-task immediately before D6's main work:

> Sanity-check: take a representative bootstrap (fixture 0008's `envoy-rust.yaml` recommended — it exercises HCM + STRICT_DNS cluster + listener + http_filters + multiple route entries), parse it into `Bootstrap` via `serde_yaml::from_str`, serialize to JSON via `serde_json::to_string_pretty`, deserialize back via `serde_json::from_str`, assert structural equality. Fail-fast catches YAML-vs-JSON casing surprises (YAML allows `dns_lookup_family: V4_ONLY`; JSON requires the field renames apply correctly) or any duplicate-key-in-YAML cases that lose information on the JSON round-trip.

Adding the `#[derive(Serialize)]` cascade is mechanical once the sanity-check passes; the planner names every type touched in the cascade (~20-30 types) and verifies `cargo build` clean per-step.

### 6.5 `ClusterManager` `.clusters()` accessor

`crates/envoy-cluster/` already exposes `ClusterManager` per the 02.1 + 05.3 architecture. Phase 08.1 needs `.clusters() -> impl Iterator<Item = &Cluster>` (or `&[Cluster]`) for D7's `/clusters` rendering. The planner verifies the accessor exists; if not, adds it as a sub-task prefix to D7. Should be ~5 LoC at most.

### 6.6 Pre-state-4 fmt discipline

Per 06.1 REVIEW §7 R-9 + 07.1 + 07.2 PROGRESS attestation pattern, per-task PROGRESS sections run `cargo fmt --all -- --check` at every task close, NOT just at state-4. The 5 stable-toolchain gates (`build` / `clippy` / `fmt` / `test` / `deny`) are quoted in every per-task PROGRESS entry.

### 6.7 State-4 evidence-discipline

Per the 05.3 REVIEW I3 → 06.1 / 06.2 / 06.3 closure chain (each closed at a real CI URL + HEAD SHA + completion timestamp + per-gate quoted evidence), phase 08.1's state-4 verification must materialize a real CI run + per-gate evidence in PROGRESS.md. The 07.2 Task 10 commit `f921fdd` shape is the most recent precedent. Fixture 0014 + all 13 pre-existing fixtures (0001-0013) must be green simultaneously at the state-4 CI run.

### 6.8 Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 (Cargo.lock cadence ratification ADR) carries forward unchanged through 08.1 if no new top-level Cargo deps are added (the projected posture). The `Cargo.lock` diff at the 08.1 reviewed range is expected to be minimal (workspace-internal path-dep registrations only).

---

## 7. ADR projection

**Recommended posture: NO new ADRs land in 08.1.** All work fits inside the existing permitted-foundations set: `serde_json` (D-3.2 permitted), `Bootstrap` Serialize cascade (mechanical, no new dep). The DECISIONS.md ledger head stays at **ADR-0032** through 08.1's state-1+state-2 split commit (this commit's predecessor in lex order); the next-available number for 08.1 work is **ADR-0033**.

One conditional ADR slot stays reserved-available for state-3 / execution-time landing if reality forces it:

- **Conditional ADR-0033 — foundations grant (only if `/config_dump` requires it).** Lands at the task where the implementer surfaces a materially-worse-than-foundation result for JSON serialization (e.g., a need for `prost-reflect` or `protobuf-json` to reproduce Envoy's protobuf-JSON shape exactly). Recommended posture per parent-08 SPEC §5.3 + §7: it does NOT fire. envoy-rust's `/config_dump` is documented as semantic-equal modulo allow-list per BEHAVIOR_CONTRACT subsection 2.1, so byte-shape divergence from Envoy's proto-JSON is contract-loosened, not contract-violating. If ADR-0033 lands at 08.1 state 3, it takes the next-sequential number; if it does not land, the number stays available for 08.2 or beyond.

---

## 8. State-machine signposts for the phase-08.1 state-2 session

The next session (state 2 — the 08.1 PLAN-write) reads this section and acts.

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `docs/envoy-rust/phases/08.1-admin-endpoint-surface/PLAN.md` (standalone pre-Task-1 commit per the 04.3 / 05.1 / 06.1 / 06.2 / 06.3 / 07.1 / 07.2 PLAN-write cadence).
- **Split-gate re-evaluation:** §6.1 above projects ~12-14 tasks / ~1080-1180 LoC — comfortably under the §6.1 split-gate with healthy drift headroom. Recommended posture: no nested split.
- **Advance posture:** STATE.md advances to lifecycle state 3 with `next-skill = superpowers:subagent-driven-development` per the user's standing preference (auto-memory `feedback_execution_style`).

---

## 9. Commit message format (for state 6 of the phase-08.1 lifecycle)

```
phase 08.1: admin endpoint surface (config_dump, server_info, clusters, listeners) + 06.1 carryforward closures (I2, M1, M4) [ADRs as appropriate]

<1-3 sentence summary>

Differential surface: fixture 0014-admin-config-dump-server-info; all 14 Docker-gated fixtures (0001-0014) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline; no H2-framing surfaces engaged.
```

Per `BOOTSTRAP_PROMPT.md` §5.3, the bracketed ADR list is omitted if no ADRs landed.

---

## 10. State-machine commit (the parent-08 state-2 split commit landing this SPEC)

This SPEC is created at the parent-08 state-2 split commit alongside the sibling 08.2 SPEC, ADR-0032, ROADMAP-row additions, and STATE.md advance. Specifically the split commit:

- **CREATE** `docs/envoy-rust/phases/08.1-admin-endpoint-surface/SPEC.md` (this file).
- **CREATE** `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/SPEC.md` (sibling sub-phase SPEC).
- **MODIFY** `docs/envoy-rust/DECISIONS.md` — appends ADR-0032 documenting the parent-08 split.
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — adds rows `08.1` and `08.2` at `status: planned`; updates parent row `08`'s `sub-phases` column from `—` to `08.1, 08.2`; row `08`'s `status` stays `in-progress`.
- **MODIFY** `docs/envoy-rust/STATE.md` — advances active phase from `08` state 2 to `08.1` state 2; rewrites "Next expected skill" to `superpowers:writing-plans` scoped to this SPEC; rewrites "Last commit"; rewrites "Last updated"; adds "Phase-08 state-2 split" subsection to "Notes".

No code changes, no fixture changes, no Cargo.toml changes, no BEHAVIOR_CONTRACT.md changes. The ledger head advances **ADR-0031 → ADR-0032** at this commit (the split ADR).

**Predecessor:** `0202e38` — phase-08 state-1 SPEC.md (the parent-08 brainstorm SPEC).

**Origin/main:** `20a393d` (the 07.2 Task 9 commit; the CI evidence anchor for parent-07 close). Five docs-only commits sit unpushed on local `main` after this commit lands (`f921fdd`, `ab01755`, `1d52156`, `0202e38`, plus this split commit). Per the established precedent, accumulated docs-only state commits ride to `origin` at the next code commit / CI gate — the 08.1 state-3 execution arc's first code commit.

---

*End of SPEC. Phase 08.1 lifecycle state 2 begins at this SPEC's landing (the parent-08 state-2 split commit). The next session writes PLAN.md per `superpowers:writing-plans` and advances STATE.md to state 3 with `next-skill = superpowers:subagent-driven-development`.*
