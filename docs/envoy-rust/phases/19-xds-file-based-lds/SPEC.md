# Phase 19 (`19-xds-file-based-lds`) — SPEC

- **Phase id:** `19`
- **Slug:** `19-xds-file-based-lds`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `2044a6bf5`, the phase-18 state-6 deterministic close-out commit; the "xDS / dynamic config family" §9 table carries exactly ONE row — `18`, `done`). **This SPEC's landing commit adds the SECOND concrete row beneath the "xDS / dynamic config family" heading**, with `status: planned` — the family's first continuation phase after its opener.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"xDS / dynamic config family — ADS, delta xDS, LDS, CDS, RDS, EDS, SDS, RTDS, reconnection, initial-fetch timeout."* This phase lands the family's **filesystem-transport LDS member**: `dynamic_resources.lds_config.path_config_source` — listeners loaded from a local file at startup, observable via the `listener_manager.lds.*` stat tree, the `ListenersConfigDump` admin section, and (most importantly) the data plane: a listener that exists ONLY in the LDS file accepts and serves traffic bilaterally. Together with phase 18's file-based CDS, this completes **Envoy's documented canonical "dynamic configuration from the filesystem" topology** (an LDS file supplying the listener + a CDS file supplying the cluster it routes to — the Envoy quickstart shape), bilaterally proven by fixture 0027.
- **Position in the project:** the **eleventh post-MVP-trunk feature-family phase** and the **second concrete xDS-family phase**. The MVP trunk 00→08, the three HTTP-filter-family phases (09/10/11), the six Upstream-robustness-family phases (12/13/14/15/16/17), and the xDS-family opener (18) all stand `done`. The **26-Docker-gated-fixture regression baseline** established at phase-18 close (`0001-tcp-echo` through `0026-xds-file-based-cds`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **depends-on:** `01 02 04 06 08 18` — phase `01` (the `envoy-config` bootstrap loader the `lds_config` field extends), phase `02` (the listener + cluster runtime), phase `04` (the HCM whose config rides inside the LDS-supplied listener), phase `06` (the `envoy-stats` foundation the `listener_manager.*` stats register against), phase `08` (the admin `/config_dump` endpoint + `ConfigDumpEntry` enum the `ListenersConfigDump` section extends), and phase `18` (the `dynamic_resources`/`ConfigSource` schema, the `cds.rs` envelope-parser pattern, `load_dynamic_resources`, the harness dynamic-file rendering/mounting machinery, and the CDS-file cluster fixture 0027's composition probe routes to).
- **Brainstorm narrative:** see the "Phase-19 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the continuation-pick rationale and the alternatives weighed (CDS file watching/hot reload [the ledger's nominal prime follow-up — rejected for this phase on three stacked risks: the `ClusterManager` mutability refactor, the watch-convergence timing sensitivity, and the macOS-Docker-Desktop §6.2-verification blocker recorded in ADR-0049's Provenance]; file-based RDS / EDS [strong runners-up — more schema surgery, no pre-existing allow-listed Envoy-side surface]; the gRPC family [still blocked on H2 trailers — re-verified at HEAD]; the Load-balancing / Observability / HTTP-3+QUIC / WASM-host / Network-filters / Runtime families [the phase-18 rejection analysis carries unchanged]). The scoping decision is ratified in **ADR-0050** (landed at this brainstorm commit).

---

## 0. Critical scoping findings (READ FIRST) — phase 18's machinery generalizes to LDS with no new architecture

Phase 18 built the filesystem xDS transport for one resource type (Cluster). The state-1 brainstorm identified four findings that make the LDS continuation a **small, low-risk, single phase** — materially smaller than phase 18 itself:

1. **Every phase-18 extension point is a single-variant/single-field reuse.** The `ConfigSource`/`PathConfigSource` structs (`crates/envoy-config/src/bootstrap.rs:63-79`) are resource-type-agnostic — `DynamicResources` gains `lds_config: Option<ConfigSource>` with zero new config-source machinery. The CDS envelope parser (`crates/envoy-config/src/cds.rs`: `CdsFile { resources: Vec<CdsResource> }` + the `@type`-tagged `CdsResource` enum at `cds.rs:31-46`) generalizes to a Listener-resource variant (`@type: type.googleapis.com/envoy.config.listener.v3.Listener`) — the per-resource payload is exactly the `Listener` struct `envoy-config` already parses for `static_resources.listeners` (`bootstrap.rs:284-298`: name, address, filter_chains, listener_filters — everything an LDS payload carries). `load_dynamic_resources` (`crates/envoy-config/src/lib.rs:538`, called from `crates/envoy-bin/src/main.rs:54`) gains a parallel LDS branch. The harness `{{CDS_PATH}}` per-side rendering/mounting machinery (`tests/differential/src/lib.rs:2184-2242`) generalizes to `{{LDS_PATH}}` by the same three-step extension its phase-18 author designed it for.

2. **Initial-load-only LDS is compatible with envoy-bin's existing single-listener spawn — no listener-manager refactor.** `envoy-bin` spawns exactly one traffic listener today: `bootstrap.static_resources.listeners.first()` (`crates/envoy-bin/src/main.rs:216`), and the validator enforces it (`ConfigError::TooManyListeners` for `len() > 1` at `bootstrap.rs:1934`). The fixture-0027 topology (zero static listeners + one LDS-supplied listener) merges the dynamic listener into the effective listener list at config-load time, and `first()` picks it — no multi-listener spawning, no runtime listener add/remove, no mutability anywhere. The `TooManyListeners` gate migrates to the MERGED list (static + dynamic together ≤ 1 — the fixture's 0 + 1 passes; the single-listener limitation is preserved, not lifted, §4). Critically, **a zero-static-listeners bootstrap is ALREADY valid in envoy-rust** when an admin listener is configured: the validator rejects empty listeners only when admin is ALSO absent (`bootstrap.rs:1939`).

3. **The Envoy-side differential surface already exists as allow-listed unasserted names — the third application of this pattern.** Fixture 0011's Prometheus expectations allow-list **12 `listener_manager.*` names as Envoy-only** (`tests/fixtures/0011-admin-stats-prometheus/expectations.yaml:213-224`: `listener_added`, `listener_create_success`, `listener_create_failure`, `total_listeners_active`, `total_listeners_warming`, `workers_started`, …). The `listener_manager.lds.*` sub-family appears on the Envoy side when `lds_config` is configured (mirroring how `cluster_manager.cds.*` appeared for phase 18). Phase 19 moves the relevant subset from "Envoy-only, unasserted" to **bilaterally asserted** on the LDS-configured fixture. Note: envoy-rust already registers `listener_manager.total_listeners_active` **unconditionally** (phase 08.2 D14, `crates/envoy-listener/src/lib.rs:184-186`) — that gauge needs only assertion-tightening, not new registration.

4. **The LDS+CDS composition is Envoy's documented canonical filesystem topology — and it exercises real new machinery.** Envoy's own "configuration: dynamic from filesystem" quickstart is exactly: an LDS file supplying a listener whose HCM routes to a cluster supplied by a CDS file. Fixture 0027 realizes this topology. It is not a vanity probe: the composition exercises the **merge-ordering invariant** (dynamic CLUSTERS must merge before the dynamic LISTENER's route references are re-validated — §5.7), which is genuinely new validation machinery this phase builds on top of phase 18's defer-then-revalidate design (ADR-0049 L12).

**Consequence:** phase 19 needs **NO new crate, NO new top-level Cargo dep, NO new harness driver, NO new helper binary, and NO concurrency/timing machinery** — the LDS file load is synchronous at startup (the phase-18 `std::fs` posture), so the fixture is deterministic and timing-robust (readiness implies loaded). Projected surface is **smaller than phase 18** (~900–1300 LoC vs phase 18's ~1480–1600 actual) because every pattern is a second instantiation rather than a first build.

These findings are ratified in **ADR-0050** (landed at this brainstorm commit).

---

## 1. Goal and acceptance signal

Phase 19 makes **file-based dynamic listener discovery (LDS over the filesystem transport) work end-to-end**. When a bootstrap configures `dynamic_resources.lds_config.path_config_source.path`, both upstream Envoy and envoy-rust:

- **load the listeners defined in that file at startup** (initial load; before serving traffic),
- **accept and serve data-plane traffic on those listeners** exactly as if they had been defined statically (the full HCM + filter-chain + router + upstream machinery applies),
- **expose the load observably**: the `listener_manager.lds.*` stat tree + the `listener_manager.listener_added` family (§6.2-verified subset) and the `/config_dump` `ListenersConfigDump` section listing the dynamically-loaded listeners.

**Differential surface added by phase 19:**

- **Fixture `0027-xds-file-based-lds`** — the canonical Envoy filesystem-dynamic-config topology, bilaterally asserted. Both proxies receive identical bootstraps whose `static_resources` carries **one static cluster (`static_backend`) and ZERO listeners**, and whose `dynamic_resources` configures BOTH `lds_config` (pointing at a shared `lds.yaml` template) and `cds_config` (pointing at a shared `cds.yaml` template, reusing the proven phase-18 machinery). The LDS file defines one listener (`dynamic_listener`, HTTP/1.1 HCM) with two routes; the CDS file defines one cluster (`dynamic_backend`). Probes (all via the existing `Driver::Http1KeepAlive`):
  1. **Data plane, LDS isolation (the load-bearing probe):** `GET /static` → **200** + the `http1-echo-server` echo body **byte-exact** bilaterally + `x-envoy-upstream-service-time` present, routed through the **static** cluster. Without the LDS load, there is NO listener to connect to — the probe discriminates loaded-from-not-loaded independently of CDS.
  2. **Data plane, LDS+CDS composition (the canonical-quickstart probe):** `GET /dynamic` → **200** + echo body byte-exact bilaterally, routed through the **CDS-supplied** cluster — a request whose listener AND cluster both exist only in dynamic-resource files.
  3. **Stats:** `listener_manager.lds.update_success: 1` + `listener_manager.lds.update_failure: 0` (+ the §6.2-verified subset, incl. `listener_manager.listener_added: 1` and the per-cluster discriminators `cluster.static_backend.upstream_rq_total: 1` / `cluster.dynamic_backend.upstream_rq_total: 1`) asserted via the named-stat scrape. The phase-18 `cluster_manager.cds.*` names are asserted too (they come for free on this topology).
  4. **Admin scrape:** `/config_dump` `ListenersConfigDump` entry naming `dynamic_listener` (the §6.2-item-5-verified JSON shape), alongside the existing `ClustersConfigDump` assertion shape from fixture 0026; plus the admin `/listeners` endpoint listing `dynamic_listener` bilaterally (the phase-08.1 endpoint's `static_resources.listeners` iteration migrates to `all_listeners()` — D3 site ii; §6.2 item 5 verifies Envoy lists LDS-supplied listeners there).

  The discriminating differential observables are the **connect-to-dynamic-listener data-plane success + the LDS update counters** — a proxy that ignored `lds_config` would refuse the TCP connection (no listener bound) and emit no LDS stats. Probe shape, exact stat names, and the config_dump assertion are §6.2-verified projections (§6.2 items 1–5).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0027-xds-file-based-lds` green at Docker-gated CI.
- **(b)** All **26 pre-existing differential fixtures** (`0001` through `0026`) **remain green simultaneously** at the same CI run (regression-equivalence per §7.5 (b)). The LDS machinery is inert when `lds_config` is unconfigured: no existing fixture configures it; the new stats register ONLY when `dynamic_resources.lds_config` is present (the phase-15/17/18 conditional-registration discipline); the `ListenersConfigDump` entry is emitted ONLY when `lds_config` is configured (fixtures 0014 + 0026 untouched); fixture 0011's Prometheus set-diff sees zero new envoy-rust names.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline). Phase 19 does not touch HTTP framing.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (new seed `dynamic_resources_lds.yaml`; corpus 29 → 30; the new-seed atomic edit: fuzz `.gitignore` allow-list + the SUCCESS-array together — the 09→18 lesson; the corpus is fully consistent entering this phase per the phase-18 carryforward-disposition-1 closure).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` (run PER TASK in the state-3 arc, per `project_state3_arc_skips_clippy`), `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean — plus the 4 standalone-crate builds (`-p envoy-config` / `-p envoy-cluster` / `-p envoy-http1` / `-p envoy-http2`) per `project_isolated_crate_build_blindspot`.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent).

> **NOTE — single phase projected (see §6.1).** Phase 19's surface (the `lds_config` schema + LDS-file parsing + the listener-list merge + ordering + the `listener_manager.*` stats + the `ListenersConfigDump` section + the harness `{{LDS_PATH}}` generalization + fixture 0027 + in-process backstop + fuzz seed + BEHAVIOR_CONTRACT rows) is projected at **~900–1300 LoC / ~10–12 tasks** — comfortably under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task split gate, with MORE margin than phases 16/17/18 carried (every deliverable is a second instantiation of a phase-18 pattern). The recommended split seam if the §6.2-refined estimate fires the gate anyway: **`19.1`** (schema + LDS-file parsing + listener-list merge + ordering + in-process backstop + fuzz seed — the foundation slice, regression-equivalence acceptance) / **`19.2`** (the `listener_manager.*` stats + `ListenersConfigDump` + harness extension + fixture 0027 + parent-19 close). The split ADR would be ADR-0052 (§7).

---

## 2. Behavior-contract scope for phase 19

Phase 19 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x→18 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — LDS / listener-manager subset (projected; §6.2-verified)

New rows, mirroring upstream Envoy v1.33's documented stat tree. **Minimum-viable subset** (the 14.1/15/16/17/18 namespace-subset precedent): emit the names Envoy emits for the behavior envoy-rust implements; allow-list the rest.

| Stat name | Kind | Equivalence (projected; §6.2-verified) | Rationale |
|---|---|---|---|
| `listener_manager.lds.update_attempt` | counter | value-exact | +1 per LDS update attempt. At initial-load-only scope, exactly `1` after startup. §6.2 item 3 verifies (the phase-18 `cds.update_attempt` precedent — Envoy ticked it deterministically). |
| `listener_manager.lds.update_success` | counter | value-exact | +1 per successful LDS update. Fixture 0027: `1`. |
| `listener_manager.lds.update_failure` | counter | value-exact (0-case) | +1 per failed LDS update. Fixture 0027 asserts `0`. Structurally unreachable non-zero in envoy-rust if the all-fatal posture mirrors CDS (ADR-0049 decision 2 precedent; §6.2 item 4 verifies Envoy's LDS negative-path split). |
| `listener_manager.lds.update_rejected` | counter | value-exact (0-case) | +1 per semantically-rejected LDS update. Fixture 0027 asserts `0`. Same posture note. |
| `listener_manager.listener_added` | counter | value-exact (projected; §6.2 item 3 verifies conditionality) | +1 per listener added. Fixture 0027: `1` (the single dynamic listener; zero static listeners makes the values comparable — the phase-18 `cluster_added` conditionality lesson). |
| `listener_manager.total_listeners_active` | gauge | value-exact | Already registered **unconditionally** by envoy-rust (08.2 D14). Phase 19 tightens it from allow-listed to bilaterally asserted on fixture 0027 (`1`). §6.2 item 3 verifies Envoy's value on the all-dynamic topology. |

**Conditional registration (the §5.2 invariant):** the four `listener_manager.lds.*` names + `listener_manager.listener_added` register ONLY when `dynamic_resources.lds_config` is configured (`listener_manager.total_listeners_active` is the pre-existing unconditional exception). This is a deliberate, BEHAVIOR_CONTRACT-recorded narrowing vs Envoy (which emits the `listener_manager.*` base family unconditionally): the unconfigured-side names stay Envoy-only unasserted (fixture 0011's existing posture), preserving the 26-fixture regression baseline with zero edits.

### 2.2 "xDS wire state machine" section — LDS extension of the filesystem-transport subsection

The BEHAVIOR_CONTRACT's "Filesystem transport (`path_config_source`) — phase 18" subsection (first populated at phase 18) gains LDS rows: (a) the LDS file envelope shape Envoy accepts (§6.2 item 1's finding — projected to mirror the CDS envelope with `@type: type.googleapis.com/envoy.config.listener.v3.Listener`), (b) the initial-load/readiness ordering for listeners (§6.2 item 2), (c) the missing/malformed-LDS-file disposition + whether envoy-rust's all-fatal posture diverges (§6.2 item 4; the ADR-0049 decision-2 recorded-divergence pattern), (d) the static/dynamic listener name-collision rule (§6.2 item 7; the ADR-0049 decision-3 static-wins precedent), and (e) the LDS+CDS composition ordering semantics (§6.2 item 6). The `ListenersConfigDump` shape lands in the "Admin endpoint body shapes" section as a new row (§6.2 item 5 supplies the JSON shape).

### 2.3 DECISIONS.md amendment at SPEC time — ADR-0050 (the scoping ADR)

Like phases 15 (ADR-0042), 16 (ADR-0044), 17 (ADR-0046), and 18 (ADR-0048), phase 19's brainstorm DOES land an ADR: **ADR-0050** records (a) the **continuation pick** (file-based LDS over CDS file watching [three stacked risks], file-based RDS/EDS [more schema surgery, weaker pre-existing surface], the gRPC family [still blocked on H2 trailers — re-verified at HEAD], and the other §9 families [the phase-18 rejection analysis carries]) with the alternatives weighed, (b) the four §0 findings, and (c) the minimum-viable scope boundary — deliver file-based LDS initial load + the `listener_manager.lds.*` stat subset + `ListenersConfigDump` + fixture 0027; defer file watching, RDS/EDS/SDS/RTDS, the gRPC/ADS transport, delta xDS, multi-listener spawning, and the ADR-0014 protos supersession. Conditional §6.2-reconciliation + split ADRs are enumerated in §7.

---

## 3. Deliverables

Phase 19's scope is enumerated as deliverables `D1`–`D8` below. **The state-2 PLAN-writer organizes deliverables into tasks AND evaluates the §6.1 split gate** (projected NOT to fire). Deliverables are LISTED roughly in execution order; the SPEC constrains the surface, not the task organization.

### D1 — `envoy-config` schema extension (`lds_config`)

`crates/envoy-config/src/bootstrap.rs` `DynamicResources` (at `bootstrap.rs:63-79`) gains:

```rust
pub lds_config: Option<ConfigSource>,
```

reusing the existing `ConfigSource`/`PathConfigSource` structs verbatim (zero new config-source machinery). `ads_config` / `api_config_source` / `watched_directory` remain NOT fields (deny_unknown_fields rejects them; deferred per §4). New `ConfigError` variants (in `crates/envoy-config/src/lib.rs`): projected ~2 (`LdsFileError { path, source }`, `LdsParseError` — mirroring the phase-18 `CdsFileError`/`CdsParseError` pair; the exact set is a PLAN-write decision informed by §6.2 item 4).

### D2 — LDS file parsing (`envoy-config`)

Either a new `crates/envoy-config/src/lds.rs` module (the sibling-module shape) or a generalization of `cds.rs` into a resource-type-parametric `xds_file.rs` (the PLAN-writer's call — the generalization is preferred if it stays under the same LoC budget, because the gRPC/ADS phase will need resource-type dispatch anyway): `parse_lds_file(path, contents) -> Result<Vec<Listener>, ConfigError>` parsing the §6.2-item-1-verified envelope shape (projected: the phase-18 envelope with `@type: type.googleapis.com/envoy.config.listener.v3.Listener` per resource; both the bare `resources:` list and the full `DiscoveryResponse` shape accepted; always-YAML parsing per the ADR-0049 decision-1 posture). Per-listener validation reuses the existing validator functions — dynamic listeners pass through the SAME validation gauntlet as static listeners (HCM shape, route-cluster references, filter-chain checks, the `Http2ClusterFromHttp1Listener` gate).

### D3 — Effective-listener-list merge + ordering (config-load-time; the §5.4 ownership boundary)

`load_dynamic_resources` (`crates/envoy-config/src/lib.rs:538`) gains the LDS branch: parse the LDS file → store `bootstrap.dynamic_listeners: Option<Vec<Listener>>` (the phase-18 `dynamic_clusters` side-field pattern) → **ordering invariant (§5.7): the CDS merge + the LDS merge both complete BEFORE the post-merge re-validation runs**, so a dynamic listener's HCM routes can reference a dynamic cluster (the fixture-0027 composition). A `Bootstrap::all_listeners()` accessor (the `all_clusters()` pattern) feeds every downstream consumer. **The consumer-migration sweep — 4 known sites** (the brainstorm survey + controller verification; smaller than phase 18's 7-site cluster sweep; the PLAN-write's SPEC-correction pass confirms completeness): (i) the envoy-bin spawn site `main.rs:216` (`static_resources.listeners.first()` → `all_listeners().first()`); (ii) the admin `/listeners` endpoint `crates/envoy-admin/src/endpoint.rs:578-595` (`render_listeners` iterates `static_resources.listeners` — the dynamic listener must appear in `/listeners` output, a bonus differential observable; §6.2 item 5 verifies Envoy lists LDS-supplied listeners there); (iii) the `TooManyListeners` validator gate `bootstrap.rs:1934` (migrates to the merged list per §0 finding 2); (iv) the per-listener validation loop `bootstrap.rs:1994` (`for listener in &mut static_resources.listeners` — HCM/TLS/route invariants; must cover dynamic listeners per §5.3, which interacts with the side-field representation: the PLAN-writer either runs the loop twice [static + dynamic] or merges-then-validates).

### D4 — `listener_manager.*` stats (conditional registration)

The §2.1 stat subset: the four `listener_manager.lds.*` counters + `listener_manager.listener_added`, registered ONLY when `dynamic_resources.lds_config` is configured (the §5.2 invariant), following the phase-18 conditional-registration template (`crates/envoy-cluster/src/cluster.rs:1060-1097` — the predicate + `mk` closure shape). **Registration site is a PLAN-write decision:** unlike clusters (which have `ClusterManager::from_bootstrap` as the natural site), there is no ListenerManager struct — the candidates are `envoy-listener`'s `Listener::bind` (which already registers the per-listener + `total_listeners_active` stats, `crates/envoy-listener/src/lib.rs:145-195`) or an envoy-bin startup hook. `listener_manager.total_listeners_active` keeps its existing unconditional registration and gains bilateral assertion on fixture 0027.

### D5 — `/config_dump` `ListenersConfigDump` section (conditional emission)

`crates/envoy-admin/src/endpoint.rs` `ConfigDumpEntry` enum (at `:303-323`, currently `Bootstrap` + `Clusters` variants) gains a `Listeners` variant rendering the §6.2-item-5-verified shape (projected: `{"@type": ".../ListenersConfigDump", "dynamic_listeners": [{"name": ..., "active_state": {"listener": {...}}}], "static_listeners": [...]}` — note Envoy's LDS dump nests the listener under `active_state`, a different shape from the CDS dump's flat `dynamic_active_clusters`; §6.2 item 5 captures the exact nesting). Emitted ONLY when `dynamic_resources.lds_config` is configured (fixtures 0014 + 0026 untouched). The entry-ordering within the `configs` array (Bootstrap, Listeners, Clusters?) is a §6.2-item-5 capture — fixture 0026's `configs[1]` ClustersConfigDump index assertion must not break (§5.5).

### D6 — Harness LDS-file rendering + container mounting

`tests/differential/src/lib.rs` generalizes the phase-18 dynamic-file machinery (`lib.rs:2184-2242`) to a second file: when a fixture directory carries `lds.yaml`, render it per-side with the same substitution maps, write to temp, mount the upstream rendition into the Envoy container (a path ending in `.yaml` per the ADR-0049 decision-1 constraint), and substitute `{{LDS_PATH}}` into each side's main config. The combined-source backend-detection + `uses_host_gateway` scans (the phase-18 carryforward-disposition-2 bug-class lesson: **scan ALL rendered sources**) gain the LDS rendition as a third scan source. The dynamic listener's traffic port substitutes inside the LDS file via the existing `{{PORT}}` marker (the phase-18 in-file substitution precedent).

### D7 — Fixture 0027 + Docker wrapper

`tests/fixtures/0027-xds-file-based-lds/` carrying `envoy.yaml` + `envoy-rust.yaml` (admin + `node` + one static cluster `static_backend` + **ZERO static listeners** + `dynamic_resources.{lds_config,cds_config}` + `validate_clusters: false` carried per the ADR-0049 L12 precedent — §6.2 item 6 verifies whether the LDS-route context still requires it) + `lds.yaml` (the shared LDS-file template: one listener, HTTP/1.1 HCM, routes `/static` → `static_backend`, `/dynamic` → `dynamic_backend`) + `cds.yaml` (the shared CDS-file template: `dynamic_backend`, reusing the fixture-0026 shape verbatim) + `expectations.yaml` (the §1 probe list) + `README.md`. Docker-gated wrapper test at `tests/differential/tests/xds_file_based_lds.rs`.

### D8 — In-process backstop + fuzz seed + BEHAVIOR_CONTRACT extensions

In-process backstop at `crates/envoy-bin/tests/xds_file_based_lds.rs` (start envoy-rust with temp LDS + CDS files; assert the data-plane 200s through both probes + the lds stats + the config_dump entry; plus the negative paths: missing LDS file / malformed LDS file / static-dynamic listener name collision / an LDS listener routing to a nonexistent cluster — per the §6.2-item-4/6/7-verified dispositions). Fuzz seed `dynamic_resources_lds.yaml` (corpus 29 → 30). BEHAVIOR_CONTRACT: the §2.1 stat rows + the §2.2 xDS-section LDS extension + the `ListenersConfigDump` admin-body-shapes row.

---

## 4. Out of scope (deferred non-goals)

Each deferred item below is rejected by `#[serde(deny_unknown_fields)]` today (a bootstrap configuring it fails parse loudly — nothing is silently under-implemented). This extends the xDS family's deferred-surface ledger:

- **File WATCHING / hot reload** (for BOTH the CDS and LDS files; inotify/poll; listener/cluster add-update-remove at runtime; the `ClusterManager` mutability + listener drain/in-place-update refactors it requires; cluster warming; `lds.update_*`/`cds.update_*` on re-load). **Owner: the family's prime follow-up phase — now with strictly better ROI than before this phase** (one watching phase lights up hot reload for both resource types). NOTE (carried from ADR-0049 Provenance): that phase's §6.2 verification MUST run on Linux CI — macOS Docker Desktop's virtiofs/inotify limitation makes file-watch behavior unobservable locally.
- **File-based RDS** (`rds` on the HCM) + **EDS** (`eds_cluster_config`) + **SDS** (secrets) + **RTDS** (runtime) — each a future family phase, in whatever order later brainstorms pick. RDS is the natural next (the HCM-scoped config-source topology, distinct from the bootstrap-scoped `dynamic_resources` topology this phase repeats).
- **Multi-listener spawning.** envoy-bin spawns `all_listeners().first()` — the pre-existing single-traffic-listener limitation (`main.rs:216`) is NOT lifted by this phase (the fixture needs exactly one listener). A future phase that needs N concurrent traffic listeners lands the spawn-loop + per-listener shutdown plumbing.
- **Listener drain / in-place update semantics** (`listener_manager.listener_modified`/`listener_in_place_updated`/`listener_stopped`/draining stats) — meaningless at initial-load-only scope.
- **`listener_filters` processing** — remains parse-and-ignore (the ADR-0026 posture; `tls_inspector` is the only entry fixtures carry).
- **The gRPC xDS transport** (`api_config_source`/`ads_config`; tonic + envoy-protos/prost; the ADS state machine; an in-harness control plane; **the ADR-0014 protos supersession**) + **delta xDS** + **`initial_fetch_timeout`** + **REST xDS** — all carried unchanged from the phase-18 ledger.

---

## 5. Architectural invariants

### 5.1 No new crate, no new top-level Cargo dep

File I/O = `std::fs` (the phase-18 sync-load posture — envoy-config keeps zero async deps and the fuzz target stays pure); YAML parsing = `serde_yaml` (existing); the LDS envelope = serde structs (existing pattern).

### 5.2 Inert-when-unconfigured (the foundation-slice discipline)

No `lds_config` in the bootstrap → zero new stats registered, zero new config_dump entries, zero behavior change. All 26 existing fixtures are byte-identical in expectations and wire behavior. (The phase-15/17/18 conditional-registration precedent; fixture 0026 — which configures `cds_config` but NOT `lds_config` — is the critical inertness witness: it must see no `listener_manager.lds.*` names and no `ListenersConfigDump` entry.)

### 5.3 Dynamic listeners are full Listeners

Every downstream subsystem — the accept loop, the HCM, filter chains, routing, stats, access logs — treats a dynamically-loaded listener identically to a static one, because the merge happens at config-load time BEFORE envoy-bin spawns the listener. No subsystem carries an "is this listener dynamic?" branch (the only dynamic-aware consumers are the `lds.*` stats and the config_dump renderer).

### 5.4 Load-at-config-time ownership boundary

LDS file parsing lives in `envoy-config` (it produces `Vec<Listener>` configs). The merge into the effective listener list happens inside `load_dynamic_resources` at config-load time. No runtime listener mutability, no locks, no watch tasks.

### 5.5 config_dump separation + fixture-0026 stability

Dynamically-loaded listeners appear in the `ListenersConfigDump` entry, NOT inside `BootstrapConfigDump.bootstrap.static_resources.listeners`. The `ListenersConfigDump` entry's insertion must not break fixture 0026's existing `configs[1]` ClustersConfigDump index assertion (§6.2 item 5 captures Envoy's entry ordering; if Envoy orders Listeners before Clusters, fixture 0026's index assertion needs a PLAN-time amendment — flagged as a §6.2-reconciliation trigger).

### 5.6 One-shot load; zero timing sensitivity

The LDS file is read exactly once, synchronously within startup, before any listener binds. Readiness implies loaded on both proxies. The fixture needs no settle window beyond the existing readiness probe.

### 5.7 Merge ordering: clusters before listener re-validation

`load_dynamic_resources` merges dynamic CLUSTERS and dynamic LISTENERS, then runs the post-merge re-validation ONCE against the full effective state — so a dynamic listener's HCM routes may reference a dynamic cluster (the fixture-0027 composition), a static cluster, or any mix. A route to a cluster in NEITHER list still fails envoy-rust startup (the ADR-0049 decision-4 defer-then-revalidate posture, now covering dynamic-listener routes too).

---

## 6. Implementation signposts for the planner

### 6.1 Split-gate evaluation (split projected NOT to fire)

Projected surface: D1 schema ~40 LoC (+~60 tests); D2 LDS parsing ~100 (+~120 tests); D3 merge + ordering + consumer migration ~120 (+~100 tests); D4 stats ~80 (+~60 tests); D5 config_dump ~90 (+~70 tests); D6 harness ~70 (+~30 tests); D7 fixture ~220 (YAML + wrapper); D8 backstop + seed + contract ~180. **Total ~900–1300 LoC / ~10–12 tasks** — comfortably under the ~1500-LoC / ~25-task gate. If the §6.2-refined estimate fires the gate anyway, split at the §1 NOTE seam (`19.1` foundation slice / `19.2` observability + fixture + close) with ADR-0052.

### 6.2 Empirical verification at state-2 PLAN-write (HEAVY for this phase)

The state-2 PLAN-writer dispatches a single foreground general-purpose subagent (the ADR-0037/0041/0043/0045/0047/0049 methodology) running `envoyproxy/envoy:v1.33.0` under Docker with an LDS+CDS-configured bootstrap + a host backend + admin `/stats` + `/config_dump` scrapes, and verifies:

1. **The LDS file envelope shape Envoy accepts** (the most consequential item — the D2 parser is built to this): `@type: type.googleapis.com/envoy.config.listener.v3.Listener` per resource? Bare `resources:` list AND full `DiscoveryResponse` both accepted (the CDS L1 finding's mirror)? Capture the exact minimal working file byte-for-byte.
2. **Zero-static-listeners validity + initial-load/readiness ordering:** is `static_resources` with clusters but NO listeners + `lds_config` a valid Envoy bootstrap? Is the dynamic listener accepting connections by the time `/ready` returns 200?
3. **The exact `listener_manager.*` stat names + values after a successful initial LDS load** (lds.update_attempt/update_success? listener_added? listener_create_success? total_listeners_active? workers_started?). The §2.1 subset is locked from this enumeration. Cross-check which names exist WITHOUT `lds_config` (the conditionality carve for §5.2).
4. **Missing/malformed LDS file behavior:** does Envoy hard-exit on a missing path (the CDS L4 bootstrap-PGV-failure mirror)? Warn-and-serve on a parse error (ticking `lds.update_failure`)? On a semantic error (`lds.update_rejected`)? This locks envoy-rust's negative-path disposition — projected: envoy-rust mirrors its phase-18 all-fatal posture (ADR-0049 decision 2), with the divergence recorded.
5. **The `/config_dump` shape with dynamic listeners + the `/listeners` admin endpoint:** the exact `ListenersConfigDump` JSON (the `dynamic_listeners[].active_state.listener` nesting? `version_info`? `name`?); the entry ORDERING within `configs[]` (does Listeners come before or after Clusters? — fixture 0026's `configs[1]` index assertion depends on the answer); whether the entry appears when `lds_config` is absent; AND whether Envoy's `/listeners` text/JSON admin endpoint lists LDS-supplied listeners (the D3-site-ii migration's differential observable + the BEHAVIOR_CONTRACT `/listeners` row's dynamic-listener note).
6. **The LDS+CDS composition:** an LDS-supplied listener whose HCM routes to a CDS-supplied cluster — does it work at initial load? Does the route_config inside the LDS file still require `validate_clusters: false` (the ADR-0049 L12 finding's context was a STATIC route_config; the LDS-supplied context may differ)? Does the `node.id`+`node.cluster` requirement apply identically?
7. **Static/dynamic listener name collision:** a listener defined both statically and in the LDS file — static wins (the CDS L9 mirror)? Error? Last-write-wins?
8. **Route-through-dynamic-listener wire shape:** a GET through the LDS-supplied listener — identical to the static-listener shape (200 + echo body + `x-envoy-upstream-service-time` + the standard header allow-list)? Any new response header or access-log flag?
9. **Listener address/port semantics in the LDS file:** does Envoy require `0.0.0.0` binding inside the container (the existing fixture posture)? Does the `{{PORT}}` substitution inside the LDS file behave identically to the main-config substitution?
10. **Stat conditionality cross-check:** which of the §2.1 names does Envoy emit when `lds_config` is ABSENT but `cds_config` is present (fixture 0026's exact topology — the critical §5.2 inertness witness).
11. **(Opportunistic) `listener_create_failure`:** does a dynamic listener whose port is already bound tick `listener_create_failure` and warn-and-serve, or hard-exit? (Informs the backstop's negative-path design; no deliverable depends on it.)

If item 1, 4, or 5 diverges materially from the projections → land **ADR-0051** at the PLAN-write commit (mirrors ADR-0037/0041/0043/0045/0047/0049).

### 6.3 In-process backstop assertions (heeds the 14.2→18 both-paths lesson)

The backstop covers BOTH the happy path (valid LDS + CDS files → both probes 200 + stats + config_dump) AND the negative paths (missing LDS file; malformed LDS file; static/dynamic listener name collision; an LDS listener routing to a cluster in neither list) per the §6.2-verified dispositions — the paths the differential fixture cannot exercise.

### 6.4 The 06.x stats convention + the inert-when-unconfigured discipline

Stat handles are `Arc<Counter>`/`Arc<Gauge>` registered once at construction; increments at single sites. Conditional registration per §5.2 — the phase-18 template at `cluster.rs:1060-1097` is the shape to copy.

### 6.5 Pre-state-4 fmt + clippy discipline (heeds `project_state3_arc_skips_clippy`)

`cargo clippy --workspace --all-targets --all-features -- -D warnings` runs PER TASK in the state-3 arc. The D3 consumer migration + the D5 enum extension are `collapsible_if`/`needless_borrow` candidates.

### 6.6 State-4 evidence-discipline (continues per 05.3 → … → 18 chain)

Per-gate command outputs quoted into PROGRESS Task-N; a single Docker-gated CI run as the anchor. The phase-18 lesson (carryforward disposition 2): the CI-evidence check is load-bearing — the only Critical that escaped phase 18's per-task reviews was caught there.

### 6.7 Isolated-crate build discipline (heeds `project_isolated_crate_build_blindspot`)

The state-4 verification MUST run `cargo build -p envoy-config -p envoy-cluster -p envoy-http1 -p envoy-http2` standalone in addition to the workspace build.

### 6.8 Cargo.lock cadence

No new top-level deps projected → no Cargo.lock churn beyond version bumps already in flight.

### 6.9 PLAN.md + PROGRESS.md skeleton + Task 1 preamble land alongside at state-2

The 06.2→18 standalone-PLAN cadence: one pre-Task-1 docs-only commit (PLAN + PROGRESS skeleton + Task 1 preamble + ROADMAP flip + STATE advance + any §6.2 ADR).

### 6.10 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The state-3 arc dispatches PLAN tasks to fresh subagents SERIALLY (`feedback_serial_subagent_dispatch`), each with two-stage review (spec-compliance THEN code-quality), TDD per task, one code commit + one PROGRESS commit per task.

---

## 7. Conditional ADRs (projected; land at PLAN-write or in-execution if they fire)

- **ADR-0050 (the scoping ADR) — LANDS AT THIS BRAINSTORM COMMIT.** The continuation pick + the §0 findings + the minimum-viable scope boundary + the deferral ledger. (The ADR-0042/0044/0046/0048 brainstorm-time cadence.)
- **ADR-0051 (§6.2 empirical-verification reconciliation) — PLAUSIBLE.** Fires if §6.2 item 1 (the LDS file envelope shape), item 4 (the missing/malformed-LDS-file disposition — note Envoy's LDS negative-path split may differ from its CDS split), or item 5 (the `ListenersConfigDump` shape / `configs[]` ordering — a fixture-0026 compatibility trigger) diverges materially from the projections. Lands at the state-2 PLAN-write commit. Mirrors ADR-0037/0041/0043/0045/0047/0049.
- **ADR-0052 (phase split) — POSSIBLE (projected NOT to fire, with more margin than 16/17/18).** Fires only if the §6.2-refined estimate exceeds ~1500 LoC / ~25 tasks. Seam per §1 NOTE / §6.1. Mirrors ADR-0036/0038/0040.

---

## 8. Summary

Phase 19 continues the **xDS / dynamic config family** at its next-lowest-risk increment: **file-based LDS**. A bootstrap pointing `dynamic_resources.lds_config.path_config_source` at a YAML file gets its listeners loaded at startup, serving traffic, and observable via `listener_manager.lds.*` stats and `/config_dump`'s `ListenersConfigDump` — bilaterally verified by fixture `0027-xds-file-based-lds`, which realizes Envoy's documented canonical filesystem-dynamic-config topology (an LDS-supplied listener routing to both a static cluster and a phase-18 CDS-supplied cluster) with zero concurrency, zero timing sensitivity, zero new crates, and zero new dependencies. Every deliverable is a second instantiation of a phase-18 pattern, making this the lowest-risk phase the family can offer; the hard xDS surfaces (file watching — whose ROI this phase strictly improves, RDS/EDS/SDS/RTDS, the gRPC/ADS state machine, delta) remain cleanly deferred with named owners.
