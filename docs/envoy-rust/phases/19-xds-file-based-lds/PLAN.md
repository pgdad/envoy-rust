# Phase 19 (`19-xds-file-based-lds`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (the project default per `feedback_execution_style`; implementers dispatched SERIALLY per `feedback_serial_subagent_dispatch`) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK (NOT deferred to state-4) per `project_state3_arc_skips_clippy`.

**Goal:** Make file-based dynamic listener discovery (LDS over the filesystem transport — `dynamic_resources.lds_config.path_config_source`) work end-to-end: listeners loaded from a YAML file at startup, serving data-plane traffic, observable via the `listener_manager.lds.*` stat subset + the `/config_dump` `ListenersConfigDump` entry, proven bilaterally by fixture `0027-xds-file-based-lds` — Envoy's documented canonical LDS+CDS filesystem-dynamic-config topology.

**Architecture:** The LDS file is read and parsed **synchronously at config-load time** inside the existing `envoy_config::load_dynamic_resources(&mut Bootstrap)` (a parallel LDS branch beside the phase-18 CDS branch — `std::fs`, NOT tokio; the fuzz target `parse_bootstrap` stays pure). Dynamic listeners land in a `#[serde(skip)]` `Bootstrap.dynamic_listeners: Option<Vec<Listener>>` side-field; every downstream consumer migrates from `bootstrap.static_resources.listeners` to a new `Bootstrap::all_listeners()` iterator, so dynamic listeners are full Listeners indistinguishable downstream (SPEC §5.3). **The §5.7 merge-ordering invariant:** the CDS merge AND the LDS merge both complete before ONE post-merge re-validation runs, so a dynamic listener's HCM routes may reference a dynamic cluster (the fixture-0027 composition). All LDS load failures are **fatal at startup** (the L4 lock-in — the ADR-0049 decision-2 posture extended to LDS; a recorded divergence from Envoy's warn-and-serve for content errors). The `listener_manager.lds.*` stats register conditionally via a new unit-testable `envoy_listener::register_lds_stats` free function (envoy-listener already depends on envoy-config + envoy-stats); the `ListenersConfigDump` entry renders from the Bootstrap side-field already held by `AdminHandler` and is pushed AFTER the ClustersConfigDump entry (Envoy's verified `configs[]` order: Bootstrap[0], Clusters[1], Listeners[2]).

**Tech Stack:** Rust (workspace crates `envoy-config`, `envoy-listener`, `envoy-admin`, `envoy-bin`); `serde`/`serde_yaml`; `envoy-stats` `Counter`/`Gauge`; `testcontainers` differential harness (`with_copy_to` container file mounting); `cargo fuzz` (`parse_bootstrap`).

---

## §6.2 empirical lock-ins (verified against `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, 2026-06-03; **NO reconciliation ADR — ADR-0051 does NOT fire**)

The state-2 PLAN-write ran the HEAVY SPEC §6.2 verification in Docker (LDS+CDS-configured bootstrap [zero static listeners + one static cluster + an LDS file routing `/static` → the static cluster and `/dynamic` → a CDS cluster] + host HTTP backend + admin `/stats` + `/config_dump` scrapes; version string `b0f43d67aa25c1b03c97186a200cc187f4c22db3/1.33.0/Clean/RELEASE/BoringSSL`). **All three ADR-0051 trigger items (1: envelope; 4: negative-path disposition; 5: ListenersConfigDump shape/ordering) CONFIRM the SPEC projections** — the projections were built from the phase-18 CDS empirical findings, and Envoy's LDS surface mirrors its CDS surface almost exactly. The refinements found (the ✧-marked items below) are assertion/shape lock-ins whose envoy-rust posture was already pre-ratified by ADR-0050/SPEC §5.7; none changes a decision, so no ADR lands. Findings:

- **L1 — LDS file envelope (item 1; CONFIRMS).** The bare `resources:` list with per-resource `@type: type.googleapis.com/envoy.config.listener.v3.Listener` is accepted (`lds.update_success: 1`); the full `DiscoveryResponse` shape (`version_info` + `resources`) is ALSO accepted; omitting `@type` → `lds.update_failure: 1` (parse-class, warn-and-serve in Envoy; the exact log: `Unable to parse JSON as proto … missing @type in Any is only allowed for an empty object`). **The exact minimal working LDS file is the SPEC §6.2 projection shape** (a `Listener` payload with name/address/filter_chains exactly as `static_resources.listeners` carries). envoy-rust (Task 2): `parse_lds_file` requires `@type` per resource, accepts the bare envelope AND an ignored `version_info`, parses with `serde_yaml` regardless of extension (the ADR-0049 decision-1 always-YAML posture). **The Envoy-side container path substituted into `{{LDS_PATH}}` MUST end in `.yaml`** (Task 6/7 constraint, same as `{{CDS_PATH}}`).

- **L2 — zero-static-listeners validity + readiness ordering (item 2; CONFIRMS).** A bootstrap with clusters but ZERO static listeners + `lds_config` is valid; the dynamic listener is accepting connections by the instant `/ready` first returns 200. Envoy's startup ordering: `loading 0 listener(s)` → `cm init: initializing cds` → `cds: added/updated 1 cluster(s)` → `all clusters initialized` → `lds: add/update listener 'dynamic_listener'` → `all dependencies initialized. starting workers` — **clusters initialize BEFORE listeners are added** (Envoy's own ordering mirrors the §5.7 invariant). envoy-rust mirrors naturally (sync load before listeners bind). Zero settle/timing machinery (SPEC §5.6 stands).

- **L3 — the full `listener_manager.*` stat tree after a successful LDS load (item 3; CONFIRMS, one precision caveat ✧).** Envoy emits 21 `listener_manager.*` names. envoy-rust's minimum-viable subset is the **6 names** of SPEC §2.1:

  | Stat | Kind | Fixture-0027 value (bilateral) |
  |---|---|---|
  | `listener_manager.lds.update_attempt` | counter | 1 |
  | `listener_manager.lds.update_success` | counter | 1 |
  | `listener_manager.lds.update_failure` | counter | 0 |
  | `listener_manager.lds.update_rejected` | counter | 0 |
  | `listener_manager.listener_added` | counter | 1 |
  | `listener_manager.total_listeners_active` | gauge | 1 |

  **✧ Precision caveat:** `listener_manager.listener_create_success` is **per-worker** (observed value 12 on a 12-core host — one tick per worker thread per listener); it must NEVER be asserted bilaterally and is NOT in the subset. Envoy-only unasserted names (ignored by the named-stat scrape): `listener_create_success`, `listener_create_failure`, `listener_modified`, `listener_removed`, `listener_stopped`, `listener_in_place_updated`, `total_listeners_warming`, `total_listeners_draining`, `total_filter_chains_draining`, `workers_started`, `lds.update_time`, `lds.update_duration`, `lds.version`, `lds.version_text`, `lds.init_fetch_timeout`. The phase-18 `cluster_manager.*` 6-name subset also appears on fixture 0027 (it configures `cds_config` too) with `cluster_added: 2` / `active_clusters: 2` (1 static + 1 dynamic cluster — the count includes static clusters, the phase-18 L3 conditionality lesson).

- **L4 — negative-path disposition (item 4; CONFIRMS — Envoy's LDS split exactly mirrors its CDS split).** **(a)** nonexistent `path` → **hard startup failure** (exit 1; `paths must refer to an existing path in the system` — bootstrap-level PGV); **(b)** file exists but malformed YAML → Envoy **starts and serves** (`/ready` 200), `lds.update_failure: 1` (log: `Filesystem config update failure: … yaml-cpp: error at line 2…`); **(c)** valid YAML but semantically-invalid listener (PGV violation, e.g. `port_value: 99999999`) → starts and serves, `lds.update_rejected: 1` (NOT `update_failure`; log: `Filesystem config update rejected: Proto constraint validation failed`). **envoy-rust (the ADR-0049 decision-2 posture extended to LDS, pre-ratified by ADR-0050):** ALL LDS load errors are **FATAL at startup** — missing/unreadable file, malformed YAML, missing `@type`, unknown fields, per-listener validation failure. The divergence for classes (b)/(c) is recorded in BEHAVIOR_CONTRACT (Task 10). `lds.update_failure`/`lds.update_rejected` register at 0 and are structurally unreachable non-zero in envoy-rust; fixture 0027 asserts both at 0 bilaterally (satisfiable — a successful load).

- **L5 — `ListenersConfigDump` shape + `configs[]` ordering (item 5; CONFIRMS — fixture 0026's index assertion HOLDS; one shape detail ✧).** With both LDS+CDS configured, Envoy's `/config_dump` `configs[]` order is: `BootstrapConfigDump`[0], **`ClustersConfigDump`[1]**, **`ListenersConfigDump`[2]**, `ScopedRoutesConfigDump`[3], `RoutesConfigDump`[4], `SecretsConfigDump`[5]. **Clusters comes BEFORE Listeners — fixture 0026's `configs.1.dynamic_active_clusters…` assertion needs NO amendment** (re-verified on the CDS-only topology too: Clusters stays at [1]). The ListenersConfigDump entry (verbatim):

  ```json
  {
    "@type": "type.googleapis.com/envoy.admin.v3.ListenersConfigDump",
    "dynamic_listeners": [
      {
        "name": "dynamic_listener",
        "active_state": {
          "listener": {
            "@type": "type.googleapis.com/envoy.config.listener.v3.Listener",
            "name": "dynamic_listener",
            "address": { "...": "..." },
            "filter_chains": [ "..." ]
          },
          "last_updated": "2026-06-03T11:39:51.878Z"
        }
      }
    ]
  }
  ```

  **✧ Shape details:** the listener IS nested under `active_state.listener` (a DIFFERENT nesting from the CDS dump's flat `dynamic_active_clusters[].cluster`); `active_state` has **NO `version_info` key** (absent for file-based LDS — proto3 JSON omits empty fields); the `static_listeners` key is entirely ABSENT when no static listeners exist (not an empty array). Envoy's `/listeners` admin endpoint lists the LDS-supplied listener (`dynamic_listener::0.0.0.0:10000`; JSON form carries `listener_statuses[].name` + `local_address`). envoy-rust (Task 5): emit the entry ONLY when `lds_config` is configured, pushed AFTER the Clusters entry (Envoy's order); keys `static_listeners`/`dynamic_listeners` each `skip_serializing_if = Vec::is_empty`; NO `version_info`. **Known recorded narrowing (no differential observable at this phase):** on an LDS-only (no-CDS) bootstrap, envoy-rust's Listeners entry would land at `configs[1]` whereas Envoy's lands at `[2]` (Envoy emits a ClustersConfigDump for static clusters unconditionally; envoy-rust's is CDS-conditional per phase 18 L10) — fixture 0027 configures BOTH so the indices align bilaterally; recorded in the Task-10 contract row for any future LDS-only fixture.

- **L6 ✧ — LDS+CDS composition + `validate_clusters` (item 6; DIVERGES from the SPEC D7 projection — fixture simplification, envoy-rust posture unchanged).** The composition works (both `/static` → 200 via the static cluster and `/dynamic` → 200 via the CDS cluster). **The route_config inside an LDS-supplied listener does NOT require `validate_clusters: false`** — Envoy skips inline route-table cluster validation entirely for dynamically-delivered listeners (removing the flag changes nothing: `update_success: 1`, no `unknown cluster` error, serves fine). This reverses the SPEC D7 projection (which carried `validate_clusters: false` per the ADR-0049 L12b precedent — that finding's context was a STATIC route_config). **Consequences:** (a) fixture 0027's LDS file templates carry NO `validate_clusters` (mirroring Envoy's canonical shape); (b) envoy-rust's posture is UNCHANGED from ADR-0050/SPEC §5.7 — dynamic-listener routes go through the same defer-then-revalidate enforcement, so an LDS-listener route to a cluster in NEITHER list **fails envoy-rust startup** (vs Envoy's start-and-503; the recorded divergence extends ADR-0049 decision 4's class to LDS routes — Task 10 contract row + Task 8 backstop proof). The `node.id`+`node.cluster` requirement DOES apply identically (hard exit: `node 'id' and 'cluster' are required`) — fixture 0027 carries `node:` on both sides.

- **L7 — static/dynamic listener name collision (item 7; CONFIRMS the CDS-L9 mirror).** A listener defined both statically and in the LDS file → the STATIC listener wins silently: only the static listener's port binds, `/config_dump` shows `static_listeners` only (`dynamic_listeners` empty), `lds.update_success` still ticks 1, `listener_added: 1`, no error/warning log. envoy-rust mirrors (collision-skip + `tracing::warn!` — the phase-18 L9 merge pattern applied to listeners; Task 3).

- **L8 — wire shape through the dynamic listener (item 8; CONFIRMS).** Standard header set (`server: envoy`, `date`, `content-length`, `x-envoy-upstream-service-time`); no new response headers, no LDS-specific markers. Fixture 0027's data-plane probes are the standard fixture-0008/0026 shape.

- **L9 — listener address/port semantics (item 9; CONFIRMS).** The LDS listener binds `0.0.0.0:<port>` inside the container identically to a static listener; `{{PORT}}` substitution inside the LDS file behaves identically to main-config substitution (the kv-map render is file-agnostic). Envoy's lds log line carries the listener NAME only; the address surfaces via `/listeners`.

- **L10 — stat conditionality cross-check (item 10; CONFIRMS — the §5.2 inertness carve is exactly as projected).** On the fixture-0026 topology (CDS configured, NO `lds_config`): Envoy emits **ZERO `listener_manager.lds.*` names** (the lds subtree is conditional even in Envoy), but DOES emit the base names (`listener_added`, `listener_create_success`, `total_listeners_active`, `workers_started`, …) unconditionally. envoy-rust registers the 4 `lds.*` names + `listener_added` conditionally (ONLY when `lds_config` is configured — the recorded narrowing for the base name `listener_added`); `total_listeners_active` keeps its pre-existing unconditional registration (08.2 D14). Envoy also emits a ListenersConfigDump entry for static-only listeners (at `configs[2]`); envoy-rust's stays LDS-conditional (fixtures 0014 + 0026 untouched — their config_dump shapes carry zero new entries).

- **L11 ✧ — duplicate-address disposition (item 11, opportunistic; informs the backstop only).** Two LDS listeners on the same address in one update → Envoy ticks `lds.update_rejected: 1` (NOT `listener_create_failure`), warn-and-serves, and the application is NON-ATOMIC (the first listener from the rejected update is still bound). Irrelevant to envoy-rust's surface: the `TooManyListeners` gate (migrated to the merged list, Task 1) makes any 2-listener config fatal regardless. The backstop's negative paths use missing-file/malformed-file/name-collision/unresolved-route instead (Task 8).

## PLAN-time SPEC corrections (verified against HEAD `8ef6f5b03`)

A read-only Explore subagent verified the SPEC §0/§3 anchors against HEAD; the controller re-verified the load-bearing items by direct grep. **All 16 anchors CONFIRMED** (`DynamicResources`/`ConfigSource` `bootstrap.rs:61-79`; `cds.rs` envelope `:31-46` + `parse_cds_file` `:57-72`; `load_dynamic_resources` `lib.rs:538` + call site `main.rs:54`; `all_clusters()` `bootstrap.rs:38-43`; `cds_configured_but_unloaded()` `:49-55`; `Listener` schema `:282-298`; spawn site `main.rs:216`; validator gates `:1934`/`:1939`; per-listener loop `:1994`; `defer_cluster_refs` `:1961` → `validate_hcm` `:2068`/`:2087`/`:2350`; listener stats `envoy-listener/src/lib.rs:158-186`; `ConfigDumpEntry` `endpoint.rs:303-323` + `render_listeners` `:578-612`; harness CDS machinery `lib.rs:2183-2242` + kv maps `:2391-2472` + render/mount `:2483-2520`; `upstream::start` + `CDS_CONTAINER_PATH` `upstream.rs:11`/`:73-139`; conditional registration template `cluster.rs:1068-1097`; backstop helpers `envoy-bin/tests/xds_file_based_cds.rs`; corpus arithmetic). **One anchor drifted; five structural corrections:**

- **Correction 1 — the D3 consumer-migration sweep is 5 sites, not 4.** The SPEC §3 D3 missed the **`NoRuntime` validator gate at `bootstrap.rs:1939`** (`admin.is_none() && static_resources.listeners.is_empty()`), which is a distinct listener-list consumer from the `TooManyListeners` gate at `:1934`. The complete set: (i) `main.rs:216` spawn; (ii) `endpoint.rs:584-587` `render_listeners`; (iii) `bootstrap.rs:1934` `TooManyListeners` gate; (iv) `bootstrap.rs:1939` `NoRuntime` gate; (v) `bootstrap.rs:1994` per-listener validation loop.
- **Correction 2 — the `NoRuntime` gate needs the LDS deferral.** A zero-static-listeners + no-admin + `lds_config` bootstrap must NOT fail at `parse_bootstrap` time (listeners may arrive from the LDS file); the gate defers iff `lds_configured_but_unloaded()` and re-enforces post-merge (the phase-18 `defer_cluster_refs` pattern applied to a second gate). The `TooManyListeners` gate needs no deferral — it migrates to `all_listeners().count()` and is naturally re-checked post-merge.
- **Correction 3 — the D4 registration site is a new `envoy_listener::register_lds_stats` free function** (the SPEC left it as a PLAN-write decision between `Listener::bind` and an envoy-bin hook). `envoy-listener` already depends on `envoy-config` + `envoy-stats` (Cargo.toml confirms — no new dependency edge), so a `pub fn register_lds_stats(bootstrap: &Bootstrap, registry: &StatsRegistry) -> Result<(), ListenerError>` is unit-testable in-crate and called once from `main.rs` after registry construction. `Listener::bind` is the WRONG site (it runs per-listener at spawn time and takes a single listener config, not the bootstrap; the lds.* family is a once-per-process load fact).
- **Correction 4 — the corpus arithmetic.** 29 tracked seeds = 29 `.gitignore` allow-list entries = **25 SUCCESS + 3 REJECT + 1 minimal** (the test arrays at `bootstrap.rs:4037-4090`; the corpus is fully consistent entering this phase per the phase-18 carryforward-disposition-1 closure). Task 9: the new seed lands in the SUCCESS array + the allow-list atomically → 30 = 26 + 3 + 1.
- **Correction 5 — the LDS file templates are PER-SIDE, not shared (revises SPEC §3 D6/D7).** The SPEC projected a single shared `lds.yaml` rendered per side (the `cds.yaml` pattern). That cannot work: the LDS payload carries the HCM, and the HCM config has the established per-side field-set divergence — the Envoy side needs `generate_request_id: false` + `route_config.request_headers_to_remove` (to strip Envoy-injected headers so the echo body is byte-exact bilaterally), and envoy-rust's `deny_unknown_fields` parser rejects both fields. Fixture 0027 therefore carries **`lds-envoy.yaml` + `lds-envoy-rust.yaml`** (per-side LDS templates, mirroring the existing `envoy.yaml`/`envoy-rust.yaml` main-config convention), each rendered through its side's kv map. `cds.yaml` stays a shared template (cluster payloads have no per-side fields). The harness (Task 6) reads the per-side LDS template for each side.
- **Anchor DRIFT (minor) — none beyond Correction 1.** The fuzz corpus is consistent (no drift, unlike phase 18's PLAN-write).

## §6.1 split-gate decision

Re-estimated against the §6.2-refined surface (which CONFIRMS the projections, DROPS `validate_clusters` from the LDS templates [L6], ADDS the NoRuntime-gate deferral [Correction 2] and the per-side LDS templates [Correction 5]): **~1250–1450 LoC / 11 tasks** (production ~425, tests ~865 incl. backstop + fixture, docs ~95) — under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task gate, with MORE margin than the phase-16 (~1450–1650) / 17 (~1450–1550) / 18 (~1480–1600) no-split decisions, as the SPEC §6.1 projected. **Single un-split phase.** The work is tightly coupled: Tasks 1–3 form one schema+parse+merge unit whose output (the merged listener list) Tasks 4–5 observe and Task 7's single fixture asserts end-to-end. **ADR-0052 (the reserved split ADR) does NOT fire.** (If a single task's sub-steps blow up past ~10 items mid-execution, §6.1 permits a mid-execution split.)

---

## File structure

- `crates/envoy-config/src/bootstrap.rs` — `DynamicResources` gains `lds_config: Option<ConfigSource>`; `Bootstrap` gains `#[serde(skip)] dynamic_listeners: Option<Vec<Listener>>` + `all_listeners()` + `lds_configured_but_unloaded()`; the `TooManyListeners` gate migrates to `all_listeners()`; the `NoRuntime` gate gains the LDS deferral; the per-listener validation loop covers dynamic listeners.
- `crates/envoy-config/src/lds.rs` — NEW module: the LDS file envelope structs + `parse_lds_file(&str, &str) -> Result<Vec<Listener>, ConfigError>`.
- `crates/envoy-config/src/lib.rs` — `pub mod lds;` + 2 new `ConfigError` variants + `load_dynamic_resources` gains the LDS branch + the §5.7 single post-merge re-validation.
- `crates/envoy-bin/src/main.rs` — the listener spawn site migrates to `all_listeners()`; the `register_lds_stats` call after registry construction.
- `crates/envoy-listener/src/lib.rs` — NEW `pub fn register_lds_stats(bootstrap, registry)` (conditional `listener_manager.lds.*` + `listener_added` registration).
- `crates/envoy-admin/src/endpoint.rs` — `ConfigDumpEntry` gains the `Listeners` variant (conditional emission, pushed after `Clusters`); `render_listeners` migrates to `all_listeners()`.
- `tests/differential/src/lib.rs` — `{{LDS_PATH}}` detection + per-side LDS template rendering; the backend-detection + `uses_host_gateway` scans gain the LDS renditions.
- `tests/differential/src/upstream.rs` — `LDS_CONTAINER_PATH` constant + the LDS file `with_copy_to` mount.
- `tests/fixtures/0027-xds-file-based-lds/` — `envoy.yaml`, `envoy-rust.yaml`, `lds-envoy.yaml`, `lds-envoy-rust.yaml`, `cds.yaml`, `expectations.yaml`, `README.md`.
- `tests/differential/tests/xds_file_based_lds.rs` — Docker-gated wrapper.
- `crates/envoy-bin/tests/xds_file_based_lds.rs` — in-process backstop (happy + 4 negative paths + inertness witness).
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/dynamic_resources_lds.yaml` — NEW seed (+ `.gitignore` allow-list + SUCCESS-array entries, atomically).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — "19 entries" stat rows + the xDS-wire-state-machine LDS extension + the ListenersConfigDump admin-body-shapes row + the `/listeners` row note.

---

### Task 1: `envoy-config` schema — `lds_config` + the listener side-field + `all_listeners()` + the validator-gate migration

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`DynamicResources` `:61-66`; `Bootstrap` struct + `all_clusters()`/`cds_configured_but_unloaded()` `:30-55`; the validator gates `:1934`/`:1939`)
- Modify: `crates/envoy-config/src/lib.rs` (`ConfigError` — 2 new variants)
- Test: `crates/envoy-config/src/bootstrap.rs` test module

- [ ] **Step 1: Write failing tests.** (a) **`lds_config` parses:** a bootstrap with `dynamic_resources: { lds_config: { resource_api_version: V3, path_config_source: { path: /tmp/lds.yaml } } }` parses; `bootstrap.dynamic_resources.unwrap().lds_config.is_some()`; (b) **both configs together parse** (`cds_config` + `lds_config` side by side — the fixture-0027 shape); (c) **deferred fields still reject:** `ads_config:` / `api_config_source:` / `watched_directory:` inside `dynamic_resources` each still fail with serde's unknown-field error (the deny_unknown_fields regression gate); (d) **`lds_configured_but_unloaded()` transitions:** true when `lds_config` configured and `dynamic_listeners.is_none()`; false when unconfigured; false after `dynamic_listeners = Some(vec![])`; (e) **`all_listeners()` chains both fields:** a bootstrap with 1 static listener + `dynamic_listeners: Some(vec![listener2])` → `all_listeners().count() == 2`; (f) **the `TooManyListeners` gate fires on the MERGED count:** 1 static + 1 dynamic (distinct names) → `validate()` errs with `TooManyListeners(2)`; (g) **the `NoRuntime` gate defers:** a bootstrap with NO admin + ZERO static listeners + `lds_config` configured (unloaded) → `parse_bootstrap` succeeds (deferred); the same bootstrap WITHOUT `lds_config` → `NoRuntime` error (the pre-existing behavior); the same bootstrap with `dynamic_listeners: Some(vec![])` (loaded-but-empty) → `NoRuntime` error (post-merge enforcement); (h) **`resource_api_version` on `lds_config`:** `"V3"`/absent accepted, `"V2"` rejected with `UnsupportedResourceApiVersion` (the existing check must cover the new field).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config lds`. Expected: FAIL (fields/methods don't exist).
- [ ] **Step 3: Implement.**

```rust
// bootstrap.rs — DynamicResources (:61-66) gains the LDS field:
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DynamicResources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cds_config: Option<ConfigSource>,
    /// 19 D1 (ADR-0050): file-based LDS. Reuses ConfigSource/PathConfigSource
    /// verbatim (resource-type-agnostic). ads_config / api_config_source /
    /// watched_directory remain rejected by deny_unknown_fields (deferred, SPEC §4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lds_config: Option<ConfigSource>,
}

// Bootstrap gains (beside dynamic_clusters / all_clusters / cds_configured_but_unloaded):
    /// 19 D3: dynamic listeners loaded from the LDS file by load_dynamic_resources.
    /// #[serde(skip)]: NEVER serialized into BootstrapConfigDump (§5.5 separation).
    #[serde(skip)]
    pub dynamic_listeners: Option<Vec<Listener>>,

    /// 19 D3: the effective listener list — static then dynamic. Every consumer
    /// that previously iterated static_resources.listeners goes through this.
    pub fn all_listeners(&self) -> impl Iterator<Item = &Listener> {
        self.static_resources
            .listeners
            .iter()
            .chain(self.dynamic_listeners.iter().flatten())
    }

    /// 19 D1: true iff lds_config is configured but load_dynamic_resources has
    /// not yet populated dynamic_listeners (the NoRuntime-gate deferral predicate;
    /// mirrors cds_configured_but_unloaded).
    pub(crate) fn lds_configured_but_unloaded(&self) -> bool {
        self.dynamic_resources
            .as_ref()
            .and_then(|dr| dr.lds_config.as_ref())
            .is_some()
            && self.dynamic_listeners.is_none()
    }
```

```rust
// lib.rs — ConfigError gains (mirroring CdsFileError/CdsParseError):
    #[error("reading LDS file '{path}': {source}")]
    LdsFileError {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("parsing LDS file '{path}': {message}")]
    LdsParseError { path: String, message: String },
```

```rust
// bootstrap.rs validate() — gate migration (:1934-1941 becomes):
    // 19 D3 (Correction 1): the single-listener limitation applies to the MERGED
    // list (static + dynamic together ≤ 1; the pre-existing limitation is
    // preserved, not lifted — SPEC §4).
    let total_listeners = bootstrap.all_listeners().count();
    if total_listeners > 1 {
        return Err(crate::ConfigError::TooManyListeners(total_listeners));
    }
    // 19 D3 (Correction 2): the no-runtime gate DEFERS while lds_config is
    // configured-but-unloaded (listeners may arrive from the LDS file); the
    // post-merge re-validation (load_dynamic_resources → validate()) re-enforces.
    if bootstrap.admin.is_none()
        && total_listeners == 0
        && !bootstrap.lds_configured_but_unloaded()
    {
        return Err(crate::ConfigError::NoRuntime);
    }
```

Also: the `resource_api_version` validator check (find the existing `UnsupportedResourceApiVersion` site that checks `cds_config.resource_api_version`) must run for `lds_config.resource_api_version` too — extend it to iterate both `Option<ConfigSource>` fields.

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-config` (full — every pre-existing test must stay green; the `dynamic_clusters`-era tests are untouched). Expected: PASS.
- [ ] **Step 5: Workspace-compile check.** `Bootstrap` gained a field — any exhaustive `Bootstrap` struct literal in OTHER crates' tests breaks and must be extended with `dynamic_listeners: None` in the SAME commit (the phase-16/17/18 Task-1 workspace-compile lesson). Run: `cargo build --workspace --all-targets`. Fix any broken literal.
- [ ] **Step 6: clippy + fmt + standalone build + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p envoy-config
git add crates/
git commit -m "phase 19 Task 1: lds_config schema + dynamic_listeners side-field + all_listeners() + validator-gate migration [ADR-0050]"
```

---

### Task 2: `envoy-config` — the LDS file parser (`lds.rs`)

**Files:**
- Create: `crates/envoy-config/src/lds.rs`
- Modify: `crates/envoy-config/src/lib.rs` (`pub mod lds;` + re-export)
- Test: `crates/envoy-config/src/lds.rs` test module

- [ ] **Step 1: Write failing tests.** (a) **the minimal working LDS file parses** (the L1 shape — bare `resources:` list, one `@type`-tagged Listener with name/address/filter_chains carrying an HCM): `parse_lds_file` returns 1 Listener named `dynamic_listener`; (b) **the DiscoveryResponse envelope parses** (`version_info: "1"` + `resources:` — version_info accept-and-ignore) and yields the same listener; (c) **missing `@type` rejects** with `LdsParseError` (message mentions the tag); (d) **a non-Listener `@type` rejects** (`type.googleapis.com/envoy.config.cluster.v3.Cluster` inside an LDS file → `LdsParseError`); (e) **malformed YAML rejects** with `LdsParseError` carrying the path; (f) **unknown fields inside the Listener resource reject** (the deny_unknown_fields strictness rides through the tagged enum — the L4 fail-loud posture; e.g. a `bogus_field: 1` inside the listener); (g) **an empty `resources:` list parses** to an empty Vec (no error — the merge handles emptiness).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config lds_file`. Expected: FAIL (module doesn't exist).
- [ ] **Step 3: Implement** (mirror `cds.rs` verbatim, substituting the resource type):

```rust
//! 19 D2 (ADR-0050 / §6.2 L1): the LDS file parser — the filesystem xDS
//! transport's Listener-resource envelope. Mirrors cds.rs (phase 18): the
//! bare `resources:` list AND the full DiscoveryResponse shape are accepted;
//! `version_info` is accept-and-ignore; each resource MUST carry
//! `@type: type.googleapis.com/envoy.config.listener.v3.Listener` (the
//! ADR-0014 internally-tagged pattern); parsing is always-YAML regardless of
//! file extension (the ADR-0049 decision-1 posture).
//!
//! UNLIKE parse_cds_file (which runs validate_cluster per resource), this
//! parser does NOT validate listeners: listener validation needs the cluster
//! list (route→cluster references) and MUST run against the MERGED cluster
//! list (the §5.7 ordering invariant) — it happens at the post-merge
//! re-validation inside load_dynamic_resources, not here.

use crate::bootstrap::Listener;

#[derive(Debug, Deserialize)]
struct LdsFile {
    #[serde(default)]
    resources: Vec<LdsResource>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "@type")]
enum LdsResource {
    #[serde(rename = "type.googleapis.com/envoy.config.listener.v3.Listener")]
    Listener(Listener),
}

/// Parse an LDS file's contents. `path` is used for error messages only.
pub fn parse_lds_file(path: &str, contents: &str) -> Result<Vec<Listener>, crate::ConfigError> {
    let file: LdsFile =
        serde_yaml::from_str(contents).map_err(|e| crate::ConfigError::LdsParseError {
            path: path.to_string(),
            message: e.to_string(),
        })?;
    Ok(file
        .resources
        .into_iter()
        .map(|LdsResource::Listener(l)| l)
        .collect())
}
```

(Check `cds.rs`'s actual import/`Deserialize` derive style and mirror it exactly — including how `serde` is brought into scope. lib.rs: `pub mod lds;` + `pub use lds::parse_lds_file;`.)

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-config`. Expected: PASS.
- [ ] **Step 5: clippy + fmt + standalone build + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p envoy-config
git add crates/envoy-config/
git commit -m "phase 19 Task 2: LDS file parser (lds.rs) — @type-tagged Listener envelope [ADR-0050]"
```

---

### Task 3: `load_dynamic_resources` LDS branch + the §5.7 merge ordering + the 5-site consumer migration

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (`load_dynamic_resources` `:538-580`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (the per-listener validation loop `:1994`)
- Modify: `crates/envoy-bin/src/main.rs` (the spawn site `:216`)
- Modify: `crates/envoy-admin/src/endpoint.rs` (`render_listeners` `:578-612`)
- Test: `crates/envoy-config/src/lib.rs` + `bootstrap.rs` + `crates/envoy-admin/src/endpoint.rs` test modules

- [ ] **Step 1: Write failing tests** (envoy-config; use `tempfile` for LDS/CDS files — the existing dev-dep). (a) **the LDS branch loads:** a bootstrap with `lds_config` pointing at a temp LDS file (1 listener, HCM routing to a static cluster) + 0 static listeners → `load_dynamic_resources` succeeds; `dynamic_listeners == Some(vec![1 listener])`; `all_listeners().count() == 1`; (b) **missing LDS file is fatal:** `LdsFileError`; (c) **malformed LDS file is fatal:** `LdsParseError`; (d) **the §5.7 composition resolves:** a bootstrap with BOTH `cds_config` (temp CDS file defining cluster `dyn_c`) and `lds_config` (temp LDS file whose listener's HCM routes to `dyn_c`) → `load_dynamic_resources` succeeds (the dynamic-listener route to a dynamic cluster resolves because clusters merge BEFORE the re-validation); (e) **unresolved dynamic-listener route is fatal (the L6 recorded divergence):** the LDS listener routes to cluster `nope` (in NEITHER list) → `UnknownCluster` error, NOT a panic; (f) **listener name collision — static wins (L7):** 1 static listener named `x` + an LDS file defining listener `x` → load succeeds, `dynamic_listeners == Some(vec![])` (collision-skipped), `all_listeners().count() == 1`; (g) **dynamic listeners go through per-listener validation:** an LDS listener with an invalid HCM (e.g. no `http_filters`) → the corresponding existing `ConfigError` (whatever `validate_hcm` returns for that today), proving the validation loop covers dynamic listeners; (h) **post-merge NoRuntime enforcement:** no admin + 0 static listeners + an LDS file with an EMPTY `resources:` list → `NoRuntime` (deferred at parse, enforced post-merge).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config load_dynamic`. Expected: FAIL.
- [ ] **Step 3: Implement the load_dynamic_resources restructure.**

```rust
// lib.rs — the function becomes (preserving the CDS branch byte-for-byte where
// possible; the structural change is: no early-return when CDS is unconfigured,
// and the post-merge re-validation runs ONCE after BOTH branches — §5.7):
pub fn load_dynamic_resources(bootstrap: &mut Bootstrap) -> Result<(), ConfigError> {
    // ---- CDS branch (phase 18, ADR-0048/0049) ----
    let cds_path = bootstrap
        .dynamic_resources
        .as_ref()
        .and_then(|dr| dr.cds_config.as_ref())
        .map(|cs| cs.path_config_source.path.clone());
    if let Some(path) = cds_path {
        let contents = std::fs::read_to_string(&path).map_err(|source| ConfigError::CdsFileError {
            path: path.clone(),
            source,
        })?;
        let parsed = cds::parse_cds_file(&path, &contents)?;
        // L9: static wins on name collision; intra-file first wins.
        let mut dynamic = Vec::with_capacity(parsed.len());
        for cluster in parsed {
            if bootstrap.static_resources.clusters.iter().any(|c| c.name == cluster.name) {
                tracing::warn!(cluster = %cluster.name, "CDS cluster collides with a static cluster; static wins (skipped)");
                continue;
            }
            if dynamic.iter().any(|c: &Cluster| c.name == cluster.name) {
                tracing::warn!(cluster = %cluster.name, "duplicate cluster in CDS file; first wins (skipped)");
                continue;
            }
            dynamic.push(cluster);
        }
        bootstrap.dynamic_clusters = Some(dynamic);
    }
    // ---- LDS branch (phase 19, ADR-0050; §6.2 L1/L4/L7) ----
    let lds_path = bootstrap
        .dynamic_resources
        .as_ref()
        .and_then(|dr| dr.lds_config.as_ref())
        .map(|cs| cs.path_config_source.path.clone());
    if let Some(path) = lds_path {
        let contents = std::fs::read_to_string(&path).map_err(|source| ConfigError::LdsFileError {
            path: path.clone(),
            source,
        })?;
        let parsed = lds::parse_lds_file(&path, &contents)?;
        // L7: static wins on listener name collision; intra-file first wins.
        let mut dynamic = Vec::with_capacity(parsed.len());
        for listener in parsed {
            if bootstrap.static_resources.listeners.iter().any(|l| l.name == listener.name) {
                tracing::warn!(listener = %listener.name, "LDS listener collides with a static listener; static wins (skipped)");
                continue;
            }
            if dynamic.iter().any(|l: &Listener| l.name == listener.name) {
                tracing::warn!(listener = %listener.name, "duplicate listener in LDS file; first wins (skipped)");
                continue;
            }
            dynamic.push(listener);
        }
        bootstrap.dynamic_listeners = Some(dynamic);
    }
    // ---- §5.7: ONE post-merge re-validation after BOTH merges ----
    // Dynamic-listener routes may reference dynamic clusters; the deferred
    // cluster-reference checks + the deferred NoRuntime gate re-enforce here
    // against the full effective state. A reference to a cluster in NEITHER
    // list is fatal (the L6 recorded divergence vs Envoy's runtime-503).
    if bootstrap.dynamic_clusters.is_some() || bootstrap.dynamic_listeners.is_some() {
        bootstrap::validate(bootstrap)?;
    }
    Ok(())
}
```

  (Preserve the existing doc comment, extending it with the LDS branch + the §5.7 ordering note + the M18-1-noted on-error-mutation caveat. Update the `Cluster`/`Listener` imports as needed.)

- [ ] **Step 4: Implement the per-listener validation loop extension** (`bootstrap.rs:1994`). The loop `for listener in &mut bootstrap.static_resources.listeners` must also cover dynamic listeners. `static_resources.listeners` and `dynamic_listeners` are disjoint `Bootstrap` fields, so a chained mutable iterator borrows cleanly:

```rust
    // 19 D3 (§5.3/§5.7): dynamic listeners go through the SAME validation
    // gauntlet as static listeners (HCM shape, route-cluster references against
    // the merged cluster list, TLS checks, the H2-from-H1 gate). At parse time
    // dynamic_listeners is None (the chain is empty); at the post-merge
    // re-validation it covers the LDS-supplied listeners.
    let (static_listeners, dynamic_listeners) = (
        &mut bootstrap.static_resources.listeners,
        &mut bootstrap.dynamic_listeners,
    );
    for listener in static_listeners
        .iter_mut()
        .chain(dynamic_listeners.iter_mut().flatten())
    {
        // ... existing loop body unchanged ...
    }
```

  (The exact destructuring shape depends on what else the loop body borrows from `bootstrap` — the phase-18 Task-3 review noted the loop body reads a pre-collected `Vec<&Cluster>` snapshot, which must now also be collected BEFORE this split borrow. Follow the existing `defer_cluster_refs` snapshot pattern at `:1961`.)

- [ ] **Step 5: Implement the consumer migrations.** (i) `main.rs:216`: `bootstrap.static_resources.listeners.first()` → `bootstrap.all_listeners().next()`; (ii) `endpoint.rs:584-587` (`render_listeners`): `.bootstrap().static_resources.listeners.iter()` → `.bootstrap().all_listeners()`; add an envoy-admin test: a handler whose bootstrap carries `dynamic_listeners: Some(vec![listener "dyn_l"])` → `/listeners` body contains the line `dyn_l::<address>`. (Sites iii/iv — the validator gates — landed at Task 1; site v is Step 4 above.)
- [ ] **Step 6: Run, verify pass.** Run: `cargo test -p envoy-config && cargo test -p envoy-admin && cargo build --workspace --all-targets`. Expected: PASS (all pre-existing tests green — the chained loop is identity-equivalent when `dynamic_listeners` is None).
- [ ] **Step 7: clippy + fmt + standalone builds + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p envoy-config && cargo build -p envoy-cluster && cargo build -p envoy-http1 && cargo build -p envoy-http2
git add crates/
git commit -m "phase 19 Task 3: LDS load branch + §5.7 merge ordering + all_listeners() consumer migration [ADR-0050]"
```

---

### Task 4: `listener_manager.*` stats (conditional registration via `envoy_listener::register_lds_stats`)

**Files:**
- Modify: `crates/envoy-listener/src/lib.rs` (new free function + tests)
- Modify: `crates/envoy-bin/src/main.rs` (the call site, after registry construction)
- Test: `crates/envoy-listener/src/lib.rs` test module

- [ ] **Step 1: Write failing tests.** (a) **conditional registration (the §5.2 inertness invariant):** `register_lds_stats` on a bootstrap WITHOUT `lds_config` (including one WITH `cds_config` — the fixture-0026 inertness witness) → NO stat whose name starts with `listener_manager.lds.` and NO `listener_manager.listener_added` exists in the registry afterward; (b) **the 5-name subset on an LDS bootstrap:** a bootstrap with `lds_config` + `dynamic_listeners: Some(vec![1 listener])` (constructed directly — no file I/O at this layer) → `listener_manager.lds.update_attempt == 1`, `lds.update_success == 1`, `lds.update_failure == 0`, `lds.update_rejected == 0`, `listener_added == 1`; (c) **the count includes static listeners (the L3 conditionality lesson):** 1 static + 1 dynamic listener (hypothetical — constructed directly, bypassing the TooManyListeners gate by not calling validate) → `listener_added == 2`.
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-listener lds_stats`. Expected: FAIL.
- [ ] **Step 3: Implement.**

```rust
// envoy-listener/src/lib.rs (after the Listener impl; uses the existing
// ListenerError::StatsRegistration variant):

/// 19 D4 (ADR-0050; §6.2 L3/L10): the listener_manager.lds.* stat family +
/// listener_added — registered ONLY when dynamic_resources.lds_config is
/// configured (the §5.2 conditional-registration discipline; Envoy emits the
/// base listener_manager.* names unconditionally — those stay Envoy-only-
/// unasserted on non-LDS fixtures). All LDS load failures are fatal
/// pre-registration (the L4 posture), so update_failure / update_rejected
/// register at 0 and never tick. listener_manager.total_listeners_active is
/// NOT registered here — it keeps its pre-existing unconditional registration
/// inside Listener::bind (08.2 D14).
///
/// Called once from envoy-bin main(), after the StatsRegistry is constructed
/// and after load_dynamic_resources has populated dynamic_listeners.
pub fn register_lds_stats(
    bootstrap: &envoy_config::Bootstrap,
    registry: &envoy_stats::StatsRegistry,
) -> Result<(), ListenerError> {
    if bootstrap
        .dynamic_resources
        .as_ref()
        .and_then(|dr| dr.lds_config.as_ref())
        .is_none()
    {
        return Ok(());
    }
    let mk = |name: &str| {
        registry
            .register_counter(name)
            .map_err(|e| ListenerError::StatsRegistration(e.to_string()))
    };
    mk("listener_manager.lds.update_attempt")?.add(1);
    mk("listener_manager.lds.update_success")?.add(1);
    mk("listener_manager.lds.update_failure")?; // registers at 0 (L4)
    mk("listener_manager.lds.update_rejected")?; // registers at 0 (L4)
    let added = mk("listener_manager.listener_added")?;
    added.add(bootstrap.all_listeners().count() as u64);
    Ok(())
}
```

  (Adapt the `ListenerError::StatsRegistration` construction to its actual variant shape at `lib.rs:60-65` — it may be a tuple or struct variant. The `Counter::add` signature per `envoy-stats`.)

- [ ] **Step 4: Implement the main.rs call site.** In `crates/envoy-bin/src/main.rs`, immediately after the `StatsRegistry` construction block (`:105-107`):

```rust
    // 19 D4: conditional listener_manager.lds.* registration (no-op when
    // lds_config is unconfigured — the §5.2 inertness invariant).
    envoy_listener::register_lds_stats(&bootstrap, &registry)
        .context("registering listener_manager.lds stats")?;
```

- [ ] **Step 5: Run, verify pass.** Run: `cargo test -p envoy-listener && cargo build --workspace --all-targets`. Expected: PASS.
- [ ] **Step 6: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-listener/ crates/envoy-bin/
git commit -m "phase 19 Task 4: conditional listener_manager.lds.* stat family (register_lds_stats) [ADR-0050]"
```

---

### Task 5: `/config_dump` `ListenersConfigDump` entry (conditional emission, after Clusters)

**Files:**
- Modify: `crates/envoy-admin/src/endpoint.rs` (`ConfigDumpEntry` enum `:303-323` + the `render_config_dump` builder `:406-464`)
- Test: `crates/envoy-admin/src/endpoint.rs` test module

- [ ] **Step 1: Write failing tests.** (a) **conditional emission:** a bootstrap WITHOUT `lds_config` (one plain + one with `cds_config` only) → `/config_dump` renders NO entry whose `@type` is `…ListenersConfigDump` (fixture-0014 + fixture-0026 regression shapes — the CDS-only one still renders exactly Bootstrap[0] + Clusters[1]); (b) **the entry with dynamic listeners:** a bootstrap with `lds_config` + `cds_config` + `dynamic_listeners: Some(vec![listener "dynamic_listener"])` + `dynamic_clusters: Some(vec![…])` → `configs` has THREE entries; `configs[1]["@type"]` is ClustersConfigDump (the order lock: Clusters BEFORE Listeners, L5); `configs[2]["@type"] == "type.googleapis.com/envoy.admin.v3.ListenersConfigDump"`; `configs[2]["dynamic_listeners"][0]["name"] == "dynamic_listener"`; `configs[2]["dynamic_listeners"][0]["active_state"]["listener"]["@type"] == "type.googleapis.com/envoy.config.listener.v3.Listener"`; `configs[2]["dynamic_listeners"][0]["active_state"]["listener"]["name"] == "dynamic_listener"`; `configs[2]["dynamic_listeners"][0]["active_state"]["last_updated"]` parses as ISO-8601; **NO `version_info` key** anywhere in `active_state` (L5 ✧); (c) **empty-key omission:** zero static listeners → NO `static_listeners` key in the entry JSON; (d) **the BootstrapConfigDump never shows dynamic listeners** (§5.5 — `dynamic_listeners` is `#[serde(skip)]`, structurally guaranteed; assert the BootstrapConfigDump's `static_resources` has no listeners on the all-dynamic bootstrap).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-admin listeners_config_dump`. Expected: FAIL.
- [ ] **Step 3: Implement.** Extend `ConfigDumpEntry` (`endpoint.rs:303-323`) + the entry structs (mirror the phase-18 `StaticClusterEntry`/`DynamicClusterEntry`/`TaggedCluster` shapes):

```rust
    /// 19 D5 (§6.2 L5): emitted ONLY when dynamic_resources.lds_config is
    /// configured (fixtures 0014 + 0026 untouched). Pushed AFTER the Clusters
    /// entry — Envoy v1.33's verified configs[] order is Bootstrap[0],
    /// Clusters[1], Listeners[2]. Envoy's LDS dump nests the listener under
    /// dynamic_listeners[].active_state.listener (a DIFFERENT shape from the
    /// CDS dump's flat dynamic_active_clusters[].cluster); there is NO
    /// version_info key for file-based LDS.
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.ListenersConfigDump")]
    Listeners {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        static_listeners: Vec<StaticListenerEntry<'a>>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        dynamic_listeners: Vec<DynamicListenerEntry<'a>>,
    },

/// One static listener inside ListenersConfigDump (Envoy shape: {"listener": {...}, "last_updated": ...}).
#[derive(Serialize)]
pub(crate) struct StaticListenerEntry<'a> {
    pub(crate) listener: TaggedListener<'a>,
    pub(crate) last_updated: String,
}

/// One dynamically-loaded listener (Envoy shape: {"name": ..., "active_state": {...}}).
#[derive(Serialize)]
pub(crate) struct DynamicListenerEntry<'a> {
    pub(crate) name: &'a str,
    pub(crate) active_state: ListenerActiveState<'a>,
}

/// The active_state nesting (L5: listener + last_updated; NO version_info).
#[derive(Serialize)]
pub(crate) struct ListenerActiveState<'a> {
    pub(crate) listener: TaggedListener<'a>,
    pub(crate) last_updated: String,
}

/// A Listener serialized with the inner @type tag Envoy's Any-projection carries.
#[derive(Serialize)]
pub(crate) struct TaggedListener<'a> {
    #[serde(rename = "@type")]
    pub(crate) type_url: &'static str, // "type.googleapis.com/envoy.config.listener.v3.Listener"
    #[serde(flatten)]
    pub(crate) listener: &'a envoy_config::Listener,
}
```

  In the `render_config_dump` builder (`:406-464`): after the existing conditional `Clusters` push, add a conditional `Listeners` push gated on `bootstrap.dynamic_resources.as_ref().and_then(|dr| dr.lds_config.as_ref()).is_some()`, populated from `bootstrap.static_resources.listeners` (static_listeners) + `bootstrap.dynamic_listeners.iter().flatten()` (dynamic_listeners), with `last_updated` from the same ISO-8601 source.

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-admin` (full — the fixture-0014/0026-shape regression tests must stay green). Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-admin/
git commit -m "phase 19 Task 5: ListenersConfigDump config_dump entry (conditional, after Clusters) [ADR-0050]"
```

---

### Task 6: Harness — per-side LDS-template rendering/mounting (`{{LDS_PATH}}`)

**Files:**
- Modify: `tests/differential/src/lib.rs` (template detection `:2183-2207`; kv maps `:2391-2472`; render/write `:2483-2520`; the backend-detection + `uses_host_gateway` scans)
- Modify: `tests/differential/src/upstream.rs` (`LDS_CONTAINER_PATH` constant + `start` signature + the `with_copy_to` block)
- Test: `tests/differential/src/lib.rs` test module (render-path unit tests; the Docker-gated end-to-end proof is Task 7)

- [ ] **Step 1: Write failing unit tests.** (a) **per-side LDS template detection + rendering:** a fixture whose main templates reference `{{LDS_PATH}}` and whose dir carries `lds-envoy.yaml` + `lds-envoy-rust.yaml` → the pre-flight renders BOTH per-side LDS files through their side's kv maps (upstream gets container-perspective values, subject gets host-perspective values) and writes them to temp (mirror the existing CDS render-path test pattern at `lib.rs:4604+`); (b) **the upstream-side `{{LDS_PATH}}` substitution value is `upstream::LDS_CONTAINER_PATH`** and it ends in `.yaml` (the L1 constraint); the subject-side value is the host temp path; (c) **residual-marker fail-fast:** an unsubstituted `{{MARKER}}` inside a rendered LDS file bails pre-launch naming the marker (the phase-18 CDS pattern); (d) **the combined-source scans cover the LDS renditions:** a fixture whose `host.docker.internal` / `{{HTTP1_BACKEND_PORT}}` references live ONLY in the LDS file → `needs_http1_backend` is true and `uses_host_gateway` is true (the phase-18 carryforward-disposition-2 bug-class lesson — scan ALL rendered sources; this was phase 18's only escaped-to-CI Critical).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p differential lds`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - **`upstream.rs`:** `pub const LDS_CONTAINER_PATH: &str = "/etc/envoy-lds/lds.yaml";` + `upstream::start` gains an `lds_file: Option<&Path>` parameter mirroring `cds_file` (the `with_copy_to(LDS_CONTAINER_PATH, lds_abs)` mount).
  - **Detection + per-side templates (Correction 5):** alongside `needs_cds` (`:2183`): `let needs_lds = upstream_template.contains("{{LDS_PATH}}") || subject_template.contains("{{LDS_PATH}}");`. When true, read `fixture_dir.join("lds-envoy.yaml")` AND `fixture_dir.join("lds-envoy-rust.yaml")` (per-side LDS templates — the LDS payload carries the HCM whose per-side field-set divergence [`generate_request_id`/`request_headers_to_remove`] is established fixture convention; a missing per-side file is a hard error naming the expected filename).
  - **kv maps:** `if needs_lds`, push `("LDS_PATH", upstream::LDS_CONTAINER_PATH.to_string())` into the upstream kv map and `("LDS_PATH", subject_lds_path_str.clone())` into the subject kv map (the subject host temp path, computed up-front like `subject_cds_path`).
  - **Render/write:** render `lds-envoy.yaml` through the upstream kv map and `lds-envoy-rust.yaml` through the subject kv map; `residual_marker` fail-fast on both; `write_temp` both; retain the rendered upstream LDS string for the scans.
  - **Scans:** `backend_scan_sources` grows to 4 sources (`[&upstream_template, &subject_template, cds_scan, lds_scan]` where `lds_scan` is the upstream LDS template or `""`); `uses_host_gateway` generalizes from `(upstream_main: &str, upstream_cds: Option<&str>)` to a slice signature `uses_host_gateway(sources: &[&str])` covering main + CDS + LDS renditions (update the existing call site + tests).
  - **`upstream::start` call site:** thread the rendered upstream LDS file path as the new parameter.
- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p differential --lib` (non-Docker unit tests). Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add tests/differential/src/
git commit -m "phase 19 Task 6: harness per-side LDS-template rendering/mounting ({{LDS_PATH}}) + combined-source scan extension [ADR-0050]"
```

---

### Task 7: Fixture `0027-xds-file-based-lds` + Docker-gated wrapper

**Files:**
- Create: `tests/fixtures/0027-xds-file-based-lds/{envoy.yaml,envoy-rust.yaml,lds-envoy.yaml,lds-envoy-rust.yaml,cds.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/xds_file_based_lds.rs`

- [ ] **Step 1: Write the Docker-gated wrapper** (mirror `tests/differential/tests/xds_file_based_cds.rs`):

```rust
//! Phase 19 (ADR-0050 SPEC) differential acceptance test for fixture
//! 0027-xds-file-based-lds — Envoy's documented canonical LDS+CDS
//! filesystem-dynamic-config topology, bilaterally asserted. The bootstrap
//! carries ZERO static listeners; the listener exists ONLY because each proxy
//! loaded its dynamic_resources.lds_config.path_config_source.path at boot.
//! Probe 1 (GET /static → the static cluster) discriminates LDS-loaded from
//! not-loaded independently of CDS; probe 2 (GET /dynamic → the CDS cluster)
//! proves the §5.7 composition (a request whose listener AND cluster both
//! exist only in dynamic-resource files).

use std::path::PathBuf;

#[tokio::test]
async fn xds_file_based_lds_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0027-xds-file-based-lds");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
```

- [ ] **Step 2: Write `envoy.yaml`** (the upstream main config — node + admin + dynamic_resources with BOTH configs + one static cluster + ZERO listeners):

```yaml
node: { id: envoy-rust-phase-19-fixture-0027, cluster: envoy-rust-phase-19 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: {{ADMIN_PORT}} } } }
dynamic_resources:
  lds_config:
    resource_api_version: V3
    path_config_source:
      path: {{LDS_PATH}}
  cds_config:
    resource_api_version: V3
    path_config_source:
      path: {{CDS_PATH}}
static_resources:
  # ZERO static listeners (L2: the listener arrives from the LDS file).
  clusters:
    - name: static_backend
      type: STRICT_DNS
      dns_lookup_family: V4_ONLY
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: static_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP1_BACKEND_PORT}} }
```

- [ ] **Step 3: Write `envoy-rust.yaml`** (identical shape; the per-side kv map gives it host-perspective values — no Envoy-only fields exist in the main configs for this fixture, so the two main templates are structurally identical):

```yaml
node: { id: envoy-rust-phase-19-fixture-0027, cluster: envoy-rust-phase-19 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: {{ADMIN_PORT}} } } }
dynamic_resources:
  lds_config:
    resource_api_version: V3
    path_config_source:
      path: {{LDS_PATH}}
  cds_config:
    resource_api_version: V3
    path_config_source:
      path: {{CDS_PATH}}
static_resources:
  clusters:
    - name: static_backend
      type: STRICT_DNS
      dns_lookup_family: V4_ONLY
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: static_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP1_BACKEND_PORT}} }
```

- [ ] **Step 4: Write `lds-envoy.yaml`** (the Envoy-side LDS template — carries the Envoy-only header-stripping fields; NO `validate_clusters` per L6):

```yaml
# The Envoy-side LDS file (L1: bare `resources:` envelope, @type-tagged Listener).
# Carries the Envoy-only HCM fields (generate_request_id / request_headers_to_remove)
# that keep the echo body byte-exact bilaterally — the same per-side field-set
# divergence the main configs of fixtures 0008/0026 carry (Correction 5).
# L6: NO validate_clusters — LDS-delivered route_configs skip Envoy's inline
# cluster validation entirely (verified §6.2 item 6).
resources:
  - "@type": type.googleapis.com/envoy.config.listener.v3.Listener
    name: dynamic_listener
    address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
    filter_chains:
      - filters:
          - name: envoy.filters.network.http_connection_manager
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
              stat_prefix: ingress_http1
              codec_type: HTTP1
              generate_request_id: false
              route_config:
                name: local_route
                request_headers_to_remove:
                  - x-forwarded-for
                  - x-forwarded-proto
                  - x-request-id
                  - x-envoy-expected-rq-timeout-ms
                  - x-envoy-internal
                  - x-envoy-external-address
                virtual_hosts:
                  - name: backend_vh
                    domains: ["*"]
                    routes:
                      - match: { prefix: "/static" }
                        route: { cluster: static_backend }
                      - match: { prefix: "/dynamic" }
                        route: { cluster: dynamic_backend }
              http_filters:
                - name: envoy.filters.http.router
                  typed_config:
                    "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
```

- [ ] **Step 5: Write `lds-envoy-rust.yaml`** (the subject-side LDS template — same listener WITHOUT the Envoy-only fields; binds 127.0.0.1):

```yaml
# The envoy-rust-side LDS file: the same listener without the Envoy-only HCM
# fields (envoy-rust's deny_unknown_fields parser rejects generate_request_id +
# request_headers_to_remove; envoy-rust injects none of those headers, so
# omission keeps the echoed bodies byte-equal — the fixture-0008/0026 pattern).
resources:
  - "@type": type.googleapis.com/envoy.config.listener.v3.Listener
    name: dynamic_listener
    address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
    filter_chains:
      - filters:
          - name: envoy.filters.network.http_connection_manager
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
              stat_prefix: ingress_http1
              codec_type: HTTP1
              route_config:
                name: local_route
                virtual_hosts:
                  - name: backend_vh
                    domains: ["*"]
                    routes:
                      - match: { prefix: "/static" }
                        route: { cluster: static_backend }
                      - match: { prefix: "/dynamic" }
                        route: { cluster: dynamic_backend }
              http_filters:
                - name: envoy.filters.http.router
                  typed_config:
                    "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
```

- [ ] **Step 6: Write `cds.yaml`** (the shared CDS template — fixture 0026's shape verbatim, renamed cluster):

```yaml
# Shared CDS template (the fixture-0026 shape verbatim): the dynamic_backend
# cluster both proxies load from their CDS file. Rendered per side through the
# side's kv map ({{BACKEND_HOST}}/{{HTTP1_BACKEND_PORT}} resolve per side).
resources:
  - "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
    name: dynamic_backend
    type: STRICT_DNS
    dns_lookup_family: V4_ONLY
    lb_policy: ROUND_ROBIN
    load_assignment:
      cluster_name: dynamic_backend
      endpoints:
        - lb_endpoints:
            - endpoint:
                address:
                  socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP1_BACKEND_PORT}} }
```

- [ ] **Step 7: Write `expectations.yaml`** (the `Driver::Http1KeepAlive` shape — 2 probes + stats + admin scrapes):

```yaml
# Phase 19 fixture-0027 expectations: file-based LDS (ADR-0050) — Envoy's
# canonical LDS+CDS filesystem-dynamic-config topology. The bootstrap has ZERO
# static listeners; a proxy that ignored lds_config would have nothing bound on
# {{PORT}} and the probes would fail at connect (the load-bearing discriminator).
#
# Probe 1 (GET /static): routed through the STATIC cluster — discriminates
#   LDS-loaded-and-serving independently of CDS.
# Probe 2 (GET /dynamic): routed through the CDS-supplied cluster — the §5.7
#   composition (listener AND cluster both from dynamic-resource files).
#
# expected_stats: the L3 6-name listener_manager subset + the phase-18 6-name
# cluster_manager subset (cluster_added/active_clusters = 2: 1 static + 1
# dynamic cluster) + per-cluster + HCM counters.
#
# NOT asserted (Envoy-only, per L3): listener_create_success (PER-WORKER —
# value is core-count-dependent), listener_create_failure, listener_modified/
# removed/stopped/in_place_updated, total_listeners_warming/draining,
# total_filter_chains_draining, workers_started, lds.update_time/version/
# version_text/update_duration/init_fetch_timeout.
driver:
  kind: http1_keep_alive
  requests:
    - method: GET
      path: /static
      host: static_backend
      expected_status: 200
      expected_body: { kind: byte_exact, body: "method: GET\npath: /static\nheaders:\n  host: static_backend\nbody: \n" }
      require_header_present: x-envoy-upstream-service-time
    - method: GET
      path: /dynamic
      host: dynamic_backend
      expected_status: 200
      expected_body: { kind: byte_exact, body: "method: GET\npath: /dynamic\nheaders:\n  host: dynamic_backend\nbody: \n" }
      require_header_present: x-envoy-upstream-service-time
  settle_ms: 200
  expected_stats:
    # LDS load (L3 — the conditional listener_manager subset):
    - { name: listener_manager.lds.update_attempt,  value: 1 }
    - { name: listener_manager.lds.update_success,  value: 1 }
    - { name: listener_manager.lds.update_failure,  value: 0 }
    - { name: listener_manager.lds.update_rejected, value: 0 }
    - { name: listener_manager.listener_added,      value: 1 }
    - { name: listener_manager.total_listeners_active, value: 1 }
    # CDS load (the phase-18 subset; cluster_added/active = 2 — 1 static + 1 dynamic):
    - { name: cluster_manager.cds.update_attempt,  value: 1 }
    - { name: cluster_manager.cds.update_success,  value: 1 }
    - { name: cluster_manager.cds.update_failure,  value: 0 }
    - { name: cluster_manager.cds.update_rejected, value: 0 }
    - { name: cluster_manager.cluster_added,       value: 2 }
    - { name: cluster_manager.active_clusters,     value: 2 }
    # Data plane through BOTH clusters (one GET each):
    - { name: cluster.static_backend.upstream_rq_total,  value: 1 }
    - { name: cluster.static_backend.upstream_cx_total,  value: 1 }
    - { name: cluster.dynamic_backend.upstream_rq_total, value: 1 }
    - { name: cluster.dynamic_backend.upstream_cx_total, value: 1 }
    # HCM downstream (stat_prefix ingress_http1; 2 GETs over one keep-alive conn):
    - { name: http.ingress_http1.downstream_rq_total, value: 2 }
    - { name: http.ingress_http1.downstream_rq_2xx,   value: 2 }
  admin_scrapes:
    # /config_dump (L5): ListenersConfigDump at configs[2] (after Clusters at [1])
    # on BOTH sides; the dynamic listener's name anchors bilaterally.
    - path: /config_dump
      expected_status: 200
      expected_content_type: application/json
      expected_body_rule:
        kind: json_shape
        required_keys: ["configs"]
        required_subtree:
          path: configs.2.dynamic_listeners.0.name
          expected: dynamic_listener
        value_may_differ_keys: ["configs"]
    # /config_dump (fixture-0026 compatibility lock): ClustersConfigDump STAYS
    # at configs[1] with both LDS+CDS configured (L5 — Clusters before Listeners).
    - path: /config_dump
      expected_status: 200
      expected_content_type: application/json
      expected_body_rule:
        kind: json_shape
        required_keys: ["configs"]
        required_subtree:
          path: configs.1.dynamic_active_clusters.0.cluster.name
          expected: dynamic_backend
        value_may_differ_keys: ["configs"]
    # /listeners (the D3-site-ii migration's differential observable): the
    # LDS-supplied listener appears bilaterally; per-side address shapes differ
    # (container 0.0.0.0 vs host 127.0.0.1) — prefix-matched.
    - path: /listeners
      expected_status: 200
      expected_content_type: "text/plain"
      expected_body_rule:
        kind: text_lines
        required_lines: []
        required_line_prefixes:
          - "dynamic_listener::"
        allowlist_envoy_only_lines: []
        allowlist_envoy_rust_only_lines: []
        allowlist_envoy_only_line_prefixes:
          - "dynamic_listener::0.0.0.0:"
        allowlist_envoy_rust_only_line_prefixes:
          - "dynamic_listener::127.0.0.1:"
```

  (Adapt the exact `text_lines` field set to the `BodyRule::TextLines` struct shape at `lib.rs:466+` — fixture 0014's `/listeners` case at `tests/fixtures/0014-admin-config-dump-server-info/expectations.yaml:157-171` is the template. Adapt `expected_content_type` values to what `assert_admin_scrape_case` compares.)

- [ ] **Step 8: Write `README.md`** — fixture purpose (the canonical topology), the three bilateral observables (data-plane connect+route through an LDS-only listener; the LDS/CDS stat subsets; the ListenersConfigDump + /listeners scrapes), the L3 Envoy-only stat enumeration (esp. the per-worker `listener_create_success` exclusion), the L6 no-validate_clusters note, the per-side-LDS-template rationale (Correction 5), and the Envoy-only-fields divergence note (mirror fixture 0026's README structure).
- [ ] **Step 9: Run the Docker-gated fixture locally.** Run: `cargo test -p differential --test xds_file_based_lds -- --nocapture` (requires Docker; pre-build `tests/helpers/*` binaries first per `project_flaky_access_log_fixture_0012` — run `cargo build -p http1-echo-server` or the equivalent helper-build step the harness README names). Expected: PASS bilaterally. If the Envoy side fails: diff the rendered configs in the temp dir against the §6.2 verification's working configs (`/tmp/lds-verify/` shapes quoted in the PLAN §6.2 lock-ins).
- [ ] **Step 10: Run the full pre-existing Docker-gated suite for regression** (the §7.5 (b) check — at minimum fixtures 0011, 0014, 0026 [the conditional-emission witnesses] + 0008 [the router baseline]). Run: `cargo test -p differential --test admin_stats_prometheus --test admin_config_dump_server_info --test xds_file_based_cds --test http1_router_upstream`. Expected: PASS (zero behavior change — inertness).
- [ ] **Step 11: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add tests/fixtures/0027-xds-file-based-lds/ tests/differential/tests/xds_file_based_lds.rs
git commit -m "phase 19 Task 7: fixture 0027-xds-file-based-lds (canonical LDS+CDS topology) + Docker wrapper [ADR-0050]"
```

---

### Task 8: In-process backstop (happy path + 4 negative paths + the inertness witness)

**Files:**
- Create: `crates/envoy-bin/tests/xds_file_based_lds.rs`
- Test: itself (this IS a test file)

- [ ] **Step 1: Copy the helper block** from `crates/envoy-bin/tests/xds_file_based_cds.rs` (`reserve_port`/`wait_ready`/`http1_oneshot`/`scrape_admin_stats`/`spawn_backend`/`spawn_envoy_bin`/`write_file` — lines 57-373; the M18-9 extract-a-test-support-crate item remains open, so copying is the established pattern; note the duplication in the file header comment).
- [ ] **Step 2: Write the happy-path test** `happy_path_dynamic_listener_serves_and_reports()`: write temp LDS file (the `lds-envoy-rust.yaml` shape: 1 listener on a reserved port, routes `/static` → `static_backend`, `/dynamic` → `dynamic_backend`) + temp CDS file (the fixture-0026 envoy-rust shape: `dynamic_backend` → the spawned backend) + a bootstrap with admin + 1 static cluster (`static_backend` → the same backend) + ZERO static listeners + both `dynamic_resources` configs → spawn envoy-bin → `wait_ready` → assert: GET `/static` → 200; GET `/dynamic` → 200; `scrape_admin_stats` shows `listener_manager.lds.update_attempt == 1`, `update_success == 1`, `update_failure == 0`, `update_rejected == 0`, `listener_added == 1`, `total_listeners_active == 1`; GET `/config_dump` body contains `"ListenersConfigDump"` and `"dynamic_listener"` with the Listeners entry at `configs[2]` (after Clusters at `[1]`); GET `/listeners` body contains `dynamic_listener::`.
- [ ] **Step 3: Write the negative-path tests** (each spawns envoy-bin and asserts it EXITS non-zero with the expected error on stderr — the L4 all-fatal posture; mirror `missing_cds_file_is_fatal`): (a) `missing_lds_file_is_fatal()` — `lds_config.path` → nonexistent path → stderr contains `reading LDS file`; (b) `malformed_lds_file_is_fatal()` — LDS file contains `resources: [unclosed` → stderr contains `parsing LDS file`; (c) `lds_route_to_unknown_cluster_is_fatal()` — the LDS listener routes to cluster `nope` (in neither list) → stderr contains the `UnknownCluster` rendering (the L6 recorded divergence: envoy-rust fails startup where Envoy would warn-and-serve a 503 route); (d) `static_dynamic_listener_collision_static_wins()` — a bootstrap with 1 STATIC listener (on port A, routing to `static_backend`) + an LDS file defining a listener with the SAME NAME (on port B) → envoy-bin starts (no error), port A serves (GET → 200), port B refuses connections, stats show `listener_added == 1` (the static one only — the collision-skipped dynamic listener does not count).
- [ ] **Step 4: Write the inertness witness** `no_lds_config_is_inert()`: the fixture-0026 topology (CDS configured, NO `lds_config`, 1 static listener) → start, wait ready → `scrape_admin_stats` has ZERO names starting with `listener_manager.lds.` and NO `listener_manager.listener_added`; `/config_dump` body does NOT contain `"ListenersConfigDump"` (the critical fixture-0026 compatibility witness per SPEC §5.2).
- [ ] **Step 5: Run, verify pass.** Run: `cargo test -p envoy-bin --test xds_file_based_lds`. Expected: PASS (6 tests). Note: per `project_flaky_access_log_fixture_0012`, do NOT run this concurrently with other cargo builds (the dyld-stall + helper-rebuild flake classes).
- [ ] **Step 6: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-bin/tests/xds_file_based_lds.rs
git commit -m "phase 19 Task 8: in-process LDS backstop (happy + 4 negative paths + inertness witness) [ADR-0050]"
```

---

### Task 9: Fuzz seed `dynamic_resources_lds.yaml` (corpus 29 → 30)

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/dynamic_resources_lds.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (the allow-list)
- Modify: `crates/envoy-config/src/bootstrap.rs` (the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array `:4040-4065`)

- [ ] **Step 1: Write the seed** — a bootstrap exercising the NEW schema surface (both dynamic_resources configs + zero static listeners + admin; `parse_bootstrap` is pure so the referenced files need not exist):

```yaml
# 19 Task 9: fuzz seed for the dynamic_resources.lds_config schema surface
# (parse_bootstrap never reads the referenced files — parse-and-validate only;
# the NoRuntime gate defers on lds_configured_but_unloaded).
node: { id: fuzz-lds, cluster: fuzz }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
dynamic_resources:
  lds_config:
    resource_api_version: V3
    path_config_source:
      path: /etc/envoy/lds.yaml
  cds_config:
    resource_api_version: V3
    path_config_source:
      path: /etc/envoy/cds.yaml
static_resources:
  clusters:
    - name: static_backend
      type: STRICT_DNS
      dns_lookup_family: V4_ONLY
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: static_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 8124 }
```

- [ ] **Step 2: The atomic three-way edit** (the 09→18 lesson — seed file + `.gitignore` allow-list + SUCCESS array in ONE commit): add `dynamic_resources_lds.yaml` to `crates/envoy-config/fuzz/.gitignore` (the `!`-prefixed allow-list, after `dynamic_resources_cds.yaml`) AND to the SUCCESS array in `fuzz_corpus_seeds_parse_or_reject_cleanly` (after the `dynamic_resources_cds.yaml` entry, with a `// 19 Task 9` comment). Corpus arithmetic: 30 tracked = 30 allow-list = 26 SUCCESS + 3 REJECT + 1 minimal.
- [ ] **Step 3: Run, verify pass.** Run: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly`. Expected: PASS. Then the short fuzz sanity run (requires the nightly toolchain pin in the fuzz subcrate): `cd crates/envoy-config/fuzz && cargo +nightly fuzz run parse_bootstrap -- -runs=10000 -timeout=10`. Expected: clean (no crashes).
- [ ] **Step 4: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-config/fuzz/ crates/envoy-config/src/bootstrap.rs
git commit -m "phase 19 Task 9: fuzz seed dynamic_resources_lds.yaml (corpus 29 -> 30) [ADR-0050]"
```

---

### Task 10: BEHAVIOR_CONTRACT extensions (stat rows + xDS-section LDS extension + ListenersConfigDump row)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`

- [ ] **Step 1: Add the "19 entries (file-based LDS)" stat table** to the "Stat-name mapping" section (after the "18 entries" block): the 6 rows of the §2.1/L3 subset (lds.update_attempt/success/failure/rejected [the failure/rejected rows note "structurally unreachable non-zero in envoy-rust — the L4 all-fatal posture"], listener_added [noting the count includes static listeners + the conditional-registration narrowing], total_listeners_active [noting it is the pre-existing 08.2 unconditional registration, tightened to bilateral assertion on fixture 0027]). Include the L3 Envoy-only enumeration paragraph (the 15 unasserted names, esp. the per-worker `listener_create_success` caveat) + the §5.2 conditional-registration narrowing paragraph (the recorded divergence: Envoy emits base `listener_manager.*` names unconditionally; envoy-rust narrows `listener_added` to LDS-configured).
- [ ] **Step 2: Extend the "xDS wire state machine → Filesystem transport" section** with the LDS subsection: **(a)** the LDS envelope (L1: same dual-envelope + `@type`-required + always-YAML posture as CDS, with the Listener type URL); **(b)** initial-load/readiness ordering (L2: Envoy's cds→clusters→lds→workers ordering; envoy-rust's sync-load mirror; the §5.7 merge-ordering invariant); **(c)** the negative-path disposition table (L4: the same 3-way Envoy split as CDS [missing→fatal / parse→`lds.update_failure` warn-and-serve / semantic→`lds.update_rejected` warn-and-serve] vs envoy-rust's all-fatal posture — the recorded divergence rows); **(d)** static-wins listener name collision (L7, bilateral); **(e)** the LDS-route validation divergence (L6: Envoy skips cluster-reference validation for LDS-delivered route_configs entirely [no `validate_clusters` needed]; envoy-rust enforces via defer-then-revalidate post-merge — an LDS route to a cluster in neither list fails envoy-rust startup vs Envoy's runtime-503; per ADR-0050/SPEC §5.7); **(f)** the L10 conditionality narrowing (Envoy emits ListenersConfigDump + base listener_manager names unconditionally; envoy-rust gates both on `lds_config`).
- [ ] **Step 3: Add the ListenersConfigDump row** to the "Admin endpoint body shapes" table (mirror the ClustersConfigDump row): conditional emission (only when `lds_config` configured); position `configs[2]` (AFTER Clusters at `[1]` — Envoy's verified order); the `dynamic_listeners[].{name, active_state.{listener, last_updated}}` nesting (NO `version_info` — L5 ✧); empty-key omission for `static_listeners`; the bilateral anchor (`configs.2.dynamic_listeners.0.name == dynamic_listener`); the LDS-only-bootstrap index caveat (envoy-rust `[1]` vs Envoy `[2]` — no current fixture exercises it). Also annotate the existing `/listeners` row: LDS-supplied listeners appear in the output (the `all_listeners()` migration); per-side address shapes prefix-matched.
- [ ] **Step 4: Verify consistency.** Re-read the new sections against the PLAN §6.2 lock-ins; confirm no value contradicts fixture 0027's expectations.yaml (Task 7) or the backstop assertions (Task 8). Run: `cargo build --workspace` (docs-only — vacuous; just confirms no stray file damage).
- [ ] **Step 5: Commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 19 Task 10: BEHAVIOR_CONTRACT LDS rows (stats + xDS filesystem-transport extension + ListenersConfigDump) [ADR-0050]"
```

---

### Task 11: State-4 phase-done verification + STATE advance to state-5-next

**Files:**
- Modify: `docs/envoy-rust/phases/19-xds-file-based-lds/PROGRESS.md` (the Task-11 §7.5 evidence)
- Modify: `docs/envoy-rust/STATE.md`

- [ ] **Step 1: Run the §7.5 (e) stable-toolchain gates** (quote every output into PROGRESS Task 11):

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
```

- [ ] **Step 2: Run the 4 standalone-crate builds** (per `project_isolated_crate_build_blindspot`): `cargo build -p envoy-config && cargo build -p envoy-cluster && cargo build -p envoy-http1 && cargo build -p envoy-http2`. Quote outputs.
- [ ] **Step 3: Run the fuzz short-budget gate** (§7.5 (d)): `cd crates/envoy-config/fuzz && cargo +nightly fuzz run parse_bootstrap -- -runs=200000 -timeout=10`. Expected: clean on the 30-seed corpus. Quote the summary line.
- [ ] **Step 4: Push and capture the Docker-gated CI anchor run** (§7.5 (a)+(b)+(c)): push the branch; `gh run watch` the CI run; ALL 27 Docker-gated fixtures (0001–0027) + the h2spec ≥95% gate must be green in ONE run. Quote the run ID + conclusion into PROGRESS Task 11 (the phase-18 lesson: the CI-evidence check is load-bearing — it caught phase 18's only escaped Critical). If a known-flake fixture fails (0011/0012/0022 readiness family per `project_flaky_access_log_fixture_0012`), re-run; a 0027 failure is NOT a flake until proven otherwise — debug it.
- [ ] **Step 5: Update STATE.md** to state-4-complete / state-5-next (prepend the Active-phase status; rewrite `## Next expected skill` to the state-5 review; update `## Last commit` + `## Last updated`).
- [ ] **Step 6: Commit + push.**

```bash
git add docs/envoy-rust/
git commit -m "phase 19 Task 11: state-4 phase-done verification + STATE advance to state-5-next [ADR-0050]"
git push
```

---

## Self-review

- **Spec coverage:** D1 → Task 1; D2 → Task 2; D3 → Tasks 1+3 (the 5-site sweep split: gates at Task 1, loop+spawn+admin at Task 3); D4 → Task 4; D5 → Task 5; D6 → Task 6; D7 → Task 7; D8 → Tasks 8+9+10. SPEC §1 acceptance (a)–(e) → Task 11; (f) is state 5. The §2.1 stat subset → Tasks 4+7+10. The §2.2 contract extension → Task 10. The §5.7 ordering invariant → Task 3 (test d). The §6.3 both-paths backstop → Task 8. No gaps.
- **§6.2 lock-in coverage:** L1 → Tasks 2/6/7; L2 → Tasks 3/7; L3 → Tasks 4/7/10 (the per-worker caveat excluded from all assertions); L4 → Tasks 2/3/8/10; L5 → Tasks 5/7/10; L6 → Tasks 3/7/8/10 (no validate_clusters in the LDS templates; the unresolved-route fatal proof); L7 → Tasks 3/8; L8 → Task 7 (standard probe shape); L9 → Tasks 6/7 ({{PORT}} in the LDS templates); L10 → Tasks 4/5/8 (the inertness witnesses); L11 → not applicable (recorded; the TooManyListeners gate covers it).
- **Placeholder scan:** every code step shows actual code; the "adapt to the actual shape" notes name the exact file:line to read — they are anchor references, not placeholders.
- **Type consistency:** `parse_lds_file(path: &str, contents: &str) -> Result<Vec<Listener>, ConfigError>` (Tasks 2/3); `all_listeners() -> impl Iterator<Item = &Listener>` (Tasks 1/3/4/5); `lds_configured_but_unloaded()` (Tasks 1/3); `register_lds_stats(&Bootstrap, &StatsRegistry) -> Result<(), ListenerError>` (Task 4); `LdsFileError { path, source }` / `LdsParseError { path, message }` (Tasks 1/2/3/8); `LDS_CONTAINER_PATH` (Tasks 6/7). Consistent throughout.
- **Regression safety:** every task's Step "run, verify pass" includes the relevant pre-existing test suite; Task 7 Step 10 runs the conditional-emission witness fixtures; the §5.2 inertness is structurally guaranteed (conditional registration + conditional emission + per-side template detection only when `{{LDS_PATH}}` present).
