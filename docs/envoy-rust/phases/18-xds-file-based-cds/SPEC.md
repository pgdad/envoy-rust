# Phase 18 (`18-xds-file-based-cds`) — SPEC

- **Phase id:** `18`
- **Slug:** `18-xds-file-based-cds`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `49ae04414`, the phase-17 state-6 deterministic close-out commit; the "xDS / dynamic config family" §9 heading at that HEAD carries NO concrete rows — it is one of the heading-only families). **This SPEC's landing commit adds the FIRST concrete row beneath the "xDS / dynamic config family" heading**, with `status: planned` — the first new §9 family opened since the HTTP-filters family (phase 09) and the Upstream-robustness family (phase 12).
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"xDS / dynamic config family — ADS, delta xDS, LDS, CDS, RDS, EDS, SDS, RTDS, reconnection, initial-fetch timeout."* This phase lands the family's **filesystem-transport CDS opener**: `dynamic_resources.cds_config.path_config_source` — clusters loaded from a local file at startup, observable via the `cluster_manager.cds.*` stat tree, the `ClustersConfigDump` admin section, and (most importantly) the data plane: a route to a cluster that exists ONLY in the CDS file serves traffic bilaterally. Envoy's xDS protocol supports three transports — filesystem, gRPC (ADS/SotW/delta), and REST; the filesystem transport is a first-class xDS variant (Envoy's own docs use it as the canonical "dynamic resources from disk" example), and it is the only transport that requires neither `envoy-protos`/`prost` codegen nor `tonic` nor an in-harness control plane.
- **Position in the project:** the **tenth post-MVP-trunk feature-family phase** and the **first concrete xDS-family phase**. The MVP trunk 00→08, the three HTTP-filter-family phases (09/10/11), and the six Upstream-robustness-family phases (12/13/14/15/16/17) all stand `done`. The **25-Docker-gated-fixture regression baseline** established at phase-17 close (`0001-tcp-echo` through `0025-upstream-circuit-breaker-retry-budget`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b). The Upstream-robustness family is complete in minimum-viable form; per the phase-17 close-out's stated intent, this brainstorm **deliberately opens a NEW §9 family with a clean deferred-surface ledger**.
- **depends-on:** `01 02 04 06 08` — phase `01` (the `envoy-config` bootstrap loader the `dynamic_resources` field extends), phase `02` (the `envoy-cluster` `ClusterManager` + `Cluster` runtime the dynamic clusters are constructed through), phase `04` (the HCM + router whose route→cluster dispatch the fixture exercises), phase `06` (the `envoy-stats` foundation: `StatsRegistry` + `Counter`/`Gauge` primitives the `cluster_manager.*` stats register against), and phase `08` (the admin `/config_dump` endpoint + `ConfigDumpEntry` enum the `ClustersConfigDump` section extends + the harness `BodyRule::JsonShape` rule).
- **Brainstorm narrative:** see the "Phase-18 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + transport-pick rationale, the non-obvious **filesystem-transport-needs-no-protos finding** (§0) that makes an xDS family opener tractable as a single phase, and the alternatives weighed (gRPC family [blocked on H2 trailers]; Load-balancing family [non-deterministic or exact-hash-risk differentials]; Observability family [thin opener or collector-infra lift]; HTTP/3+QUIC / WASM-host [largest foundation lifts]; Network-filters family; Runtime + hot restart). The scoping decision is ratified in **ADR-0048** (landed at this brainstorm commit).

---

## 0. Critical scoping findings (READ FIRST) — the xDS family can open without gRPC, protos, or a control plane

Opening the xDS family looks, at first read, like a multi-phase foundation lift: `envoy-protos` (prost codegen over the upstream proto tree), `tonic` gRPC transport, an ADS state machine (version/nonce, ACK/NACK, reconnection), and an in-harness xDS control plane for the reference Envoy to talk to. The state-1 brainstorm identified three findings that make a **single-phase family opener** tractable instead:

1. **Envoy's filesystem xDS transport accepts YAML files — the existing YAML-native schema parses the resources directly.** A `path_config_source` file is a `DiscoveryResponse`-shaped document whose `resources` list carries `@type`-tagged resource messages (here: `envoy.config.cluster.v3.Cluster`). Envoy accepts this file in YAML form (file-extension-sniffed). The resource payloads inside the file are exactly the `Cluster` shape `crates/envoy-config/src/bootstrap.rs` already parses for `static_resources.clusters` — the ADR-0014 YAML-native `typed_config` shim extends to the CDS file envelope with **one new envelope struct, zero proto machinery**. (§6.2 item 1 empirically verifies the exact envelope shape Envoy accepts: bare `resources:` list vs full `DiscoveryResponse` with `version_info`, and YAML-vs-JSON acceptance.) The `envoy-protos`/prost supersession that ADR-0014 anticipates remains deferred to the gRPC-xDS phase — this phase deliberately does NOT engage it.

2. **The `ClusterManager`-immutability constraint is COMPATIBLE with initial-load-only scope.** `ClusterManager::from_bootstrap` (`crates/envoy-cluster/src/cluster.rs:721-826`) builds an immutable `HashMap<String, Arc<Cluster>>` with no post-construction mutator — a hard architectural constraint surfaced by the brainstorm's code survey. **Initial-load-only CDS does not need mutability:** the CDS file is read and parsed at config-load time (before `ClusterManager` construction), the dynamic clusters are merged into the effective cluster list, and every downstream consumer — `ClusterManager::from_bootstrap`, `H1PoolManager`/`H2PoolManager`, the upstream-TLS loop, the `envoy-health` scheduler, the `OutlierManager`, the route-reference validators — iterates the merged list exactly as it iterates static clusters today. File WATCHING (hot reload of cluster add/update/remove) is what genuinely requires a mutable cluster map — that is the family's prime follow-up phase, NOT this one (§4).

3. **The Envoy-side differential surface already exists as allow-listed unasserted names.** Envoy v1.33 emits the `cluster_manager.*` stat tree and a `ClustersConfigDump` `/config_dump` entry today — fixture 0014's expectations already allow-list Envoy's extra config_dump entries (`allowlist_envoy_only`), and fixture 0011's Prometheus set-diff already tolerates Envoy's `cluster_manager.*` names. Phase 18 moves exactly those names/sections from "Envoy-only, unasserted" to **bilaterally asserted** for the CDS-configured fixture — the differential surface is already sitting on the other side of the diff waiting to be matched (the phase-17 §0 finding 3 pattern, repeated).

**Consequence:** phase 18 needs **NO new crate, NO new top-level Cargo dep, NO new harness driver, and NO concurrency/timing machinery** — the CDS file load is synchronous at startup, so a fixture's single sequential GET through a dynamically-loaded cluster is deterministic and timing-robust (readiness implies loaded). The genuinely hard xDS surfaces (the ADS state machine, delta, watching, warming) are cleanly deferred with named owners.

These findings are ratified in **ADR-0048** (landed at this brainstorm commit) and are the reason an xDS family opener is tractable as a single un-split phase rather than a protos + transport + control-plane sub-project.

---

## 1. Goal and acceptance signal

Phase 18 makes **file-based dynamic cluster discovery (CDS over the filesystem transport) work end-to-end**. When a bootstrap configures `dynamic_resources.cds_config.path_config_source.path`, both upstream Envoy and envoy-rust:

- **load the clusters defined in that file at startup** (initial load; before listeners serve traffic),
- **route data-plane traffic to those clusters** exactly as if they had been defined statically (pools, health checks, outlier detection, circuit breakers, retries, TLS — every existing subsystem applies),
- **expose the load observably**: the `cluster_manager.cds.*` stat tree (update_success/update_failure + §6.2-verified siblings) and the `/config_dump` `ClustersConfigDump` section listing the dynamically-loaded clusters.

**Differential surface added by phase 18:**

- **Fixture `0026-xds-file-based-cds`** — bilateral assertion that both proxies, given identical bootstraps whose `static_resources` carries a listener + HCM route to cluster `dynamic_backend` but **NO static clusters at all**, and whose `dynamic_resources.cds_config.path_config_source` points at a CDS file (one shared `cds.yaml` template rendered per-side by the harness) defining `dynamic_backend` (type `STRICT_DNS`, single endpoint at the harness `http1-echo-server` backend):
  1. **Data plane (the load-bearing probe):** `GET /` → **200** + the `http1-echo-server` echo body **byte-exact** bilaterally (the fixture-0008 wire shape) + `x-envoy-upstream-service-time` present. Without the CDS load, this request has no cluster to route to — the probe discriminates loaded-from-not-loaded.
  2. **Stats:** `cluster_manager.cds.update_success: 1` + `cluster_manager.cds.update_failure: 0` (+ the §6.2-verified subset — exact names/values are §6.2 items 3/10) asserted via the existing `Driver::Http1KeepAlive` named-stat scrape.

  The discriminating differential observables are the **route-to-dynamic-cluster data-plane success + the CDS update counters** — a proxy that ignored `dynamic_resources` would 503 (no such cluster) and emit no CDS stats. Probe shape, exact stat names, and any `/config_dump` assertion are §6.2-verified projections (§6.2 items 1–5).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0026-xds-file-based-cds` green at Docker-gated CI.
- **(b)** All **25 pre-existing differential fixtures** (`0001` through `0025`) **remain green simultaneously** at the same CI run (regression-equivalence per §7.5 (b)). The CDS machinery is inert when `dynamic_resources` is unconfigured: no existing fixture configures it; the new stats register ONLY when `dynamic_resources.cds_config` is present (the phase-15/17 conditional-registration discipline); the `/config_dump` `ClustersConfigDump` entry is emitted ONLY when `dynamic_resources` is configured (so fixture 0014's JSON-shape assertion is untouched); fixture 0011's Prometheus set-diff sees zero new envoy-rust names.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline). Phase 18 does not touch HTTP framing on either protocol.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (new seed `dynamic_resources_cds.yaml`; corpus 28 → 29; the new-seed atomic edit: fuzz `.gitignore` allow-list + the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array together — the 09→16 lesson).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` (run PER TASK in the state-3 arc, per `project_state3_arc_skips_clippy`), `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean — plus the 4 standalone-crate builds (`-p envoy-config` / `-p envoy-cluster` / `-p envoy-http1` / `-p envoy-http2`) per `project_isolated_crate_build_blindspot`.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent).

> **NOTE — single phase projected (see §6.1).** Phase 18's surface (schema + CDS-file parsing + the cluster-list merge + the `cluster_manager.*` stats + the `ClustersConfigDump` section + the harness CDS-file rendering/mounting + fixture 0026 + in-process backstop + fuzz seed + BEHAVIOR_CONTRACT rows incl. the first population of the "xDS wire state machine" section) is projected at **~1000–1450 LoC / ~11–13 tasks** — under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task split gate, with less margin pressure than phases 16/17 carried. The recommended split seam if the §6.2-refined estimate fires the gate: **`18.1`** (schema + CDS-file parsing + cluster-list merge + in-process backstop + fuzz seed — the foundation slice, regression-equivalence acceptance) / **`18.2`** (the `cluster_manager.*` stats + `ClustersConfigDump` + harness extension + fixture 0026 + parent-18 close). The split ADR would be ADR-0050 (§7).

---

## 2. Behavior-contract scope for phase 18

Phase 18 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x→17 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — CDS / cluster-manager subset (projected; §6.2-verified)

New rows, mirroring upstream Envoy v1.33's documented stat tree. **Minimum-viable subset** (the 14.1/15/16/17 namespace-subset precedent): emit the names Envoy emits for the behavior envoy-rust implements; allow-list the rest.

| Stat name | Kind | Equivalence (projected; §6.2-verified) | Rationale |
|---|---|---|---|
| `cluster_manager.cds.update_success` | counter | value-exact | +1 per successful CDS update. At initial-load-only scope, exactly `1` after startup (the initial file load). §6.2 item 3 verifies the name + value (Envoy may also tick `update_attempt`). |
| `cluster_manager.cds.update_failure` | counter | value-exact (0-case) | +1 per failed CDS update (unreadable/malformed file). Fixture 0026 asserts `0`. §6.2 item 4 verifies whether a malformed file ticks `update_failure` or `update_rejected` (distinct Envoy stats) — the minimum-viable subset may need both names. |
| `cluster_manager.cluster_added` | counter | value-exact (projected; §6.2 item 3 verifies conditionality) | +1 per cluster added by CDS. Fixture 0026: `1`. **Projected to be emitted by Envoy unconditionally** (even for static clusters) — §6.2 item 10 verifies; if Envoy counts static clusters too, the envoy-rust value matches only on the all-dynamic fixture topology, and the row records the conditionality. |
| `cluster_manager.active_clusters` | gauge | value-exact (projected; same conditionality caveat) | Count of active clusters. Fixture 0026: `1` (the single dynamic cluster). Same §6.2 item-10 conditionality note. |

**Conditional registration (the §5.2 invariant):** ALL phase-18 stats register ONLY when `dynamic_resources.cds_config` is configured. This is a deliberate, BEHAVIOR_CONTRACT-recorded narrowing vs Envoy (which emits `cluster_manager.*` unconditionally): the unconfigured-side names stay Envoy-only unasserted (fixture 0011's existing posture), preserving the 25-fixture regression baseline with zero edits. The row rationale records this (the phase-15 `circuit_breakers.*` conditional-registration precedent).

### 2.2 "xDS wire state machine" section — FIRST population (filesystem transport)

The BEHAVIOR_CONTRACT's `xDS wire state machine` section (empty since bootstrap: *"populated when the xDS family begins"*) receives its **first content**: a "Filesystem transport (path_config_source)" subsection recording (a) the CDS file envelope shape Envoy accepts (§6.2 item 1's finding, byte-precise), (b) the initial-load semantics (load completes before listener readiness on both proxies — §6.2 item 2), (c) the missing/malformed-file disposition (§6.2 item 4), and (d) an explicit note that the gRPC/ADS message-sequence state machine remains unpopulated (deferred to the gRPC-xDS phase). The `/config_dump` `ClustersConfigDump` shape lands in the "Admin endpoint body shapes" section as a new row (§6.2 item 5 supplies the JSON shape).

### 2.3 DECISIONS.md amendment at SPEC time — ADR-0048 (the scoping ADR)

Like phases 15 (ADR-0042), 16 (ADR-0044), and 17 (ADR-0046), phase 18's brainstorm DOES land an ADR: **ADR-0048** records (a) the **family pick** (xDS / dynamic config over gRPC [blocked on H2 trailers], Load balancing [weak deterministic differential], Observability, HTTP/3+QUIC, Network filters, Runtime, WASM host) with the alternatives weighed, (b) the non-obvious **filesystem-transport-needs-no-protos finding** + the **initial-load-only-is-compatible-with-ClusterManager-immutability finding** (§0), and (c) the minimum-viable scope boundary — deliver file-based CDS initial load + the `cluster_manager.cds.*` stat subset + `ClustersConfigDump` + fixture 0026; defer file watching/hot-reload, LDS/RDS/EDS/SDS/RTDS, the gRPC/ADS transport, delta xDS, and the ADR-0014 protos supersession. Conditional §6.2-reconciliation + split ADRs are enumerated in §7.

---

## 3. Deliverables

Phase 18's scope is enumerated as deliverables `D1`–`D8` below. **The state-2 PLAN-writer organizes deliverables into tasks AND evaluates the §6.1 split gate** (projected NOT to fire). Deliverables are LISTED roughly in execution order; the SPEC constrains the surface, not the task organization.

### D1 — `envoy-config` schema extension (`dynamic_resources`)

`crates/envoy-config/src/bootstrap.rs` `Bootstrap` (at `bootstrap.rs:8-17`; the xDS-reservation comment at `:19-24` names this exact landing) gains:

```rust
pub dynamic_resources: Option<DynamicResources>,
```

with new structs (all `#[serde(deny_unknown_fields)]`, the established posture):

- `DynamicResources { cds_config: Option<ConfigSource> }` — `lds_config` / `ads_config` are NOT fields (deny_unknown_fields rejects them; deferred per §4 — a bootstrap configuring them fails parse loudly, the honest minimum-viable posture).
- `ConfigSource { path_config_source: PathConfigSource, resource_api_version: Option<String> }` — `api_config_source` (gRPC/REST) / `ads` are NOT fields (rejected; deferred). `resource_api_version` is parse-and-validate (accept `V3` or absent; reject others — §6.2 item 8 verifies Envoy's default).
- `PathConfigSource { path: String }` — `watched_directory` is NOT a field (rejected; deferred with file watching).

New `ConfigError` variants (in `crates/envoy-config/src/lib.rs`, the established location): projected ~3 (`UnsupportedDynamicResourceType` [lds/ads/api_config_source rejections surface via deny_unknown_fields, so this may collapse], `InvalidCdsPath` / `CdsFileError { path, source }`, `UnsupportedResourceApiVersion`, `DuplicateClusterName { name }` for static/dynamic collisions). Exact set is a PLAN-write decision.

### D2 — CDS file parsing (`envoy-config`)

A new `crates/envoy-config/src/cds.rs` module (or a `bootstrap.rs` section — PLAN-writer's call): `parse_cds_file(contents: &str) -> Result<Vec<Cluster>, ConfigError>` parsing the §6.2-item-1-verified envelope shape (projected: `resources:` list of `@type`-tagged `envoy.config.cluster.v3.Cluster` entries — the `@type` discrimination reuses the ADR-0014 tagged-enum pattern; the per-resource payload reuses the existing `Cluster` serde struct verbatim). Per-cluster validation reuses the existing validator functions (`validate_circuit_breakers`, `validate_health_checks`, `validate_outlier_detection`, the cluster-shape checks) — dynamic clusters pass through the SAME validation gauntlet as static clusters.

### D3 — Effective-cluster-list merge (config-load-time; the §5.4 ownership boundary)

The dynamic clusters are loaded and merged at **config-load time**: after `parse_bootstrap` + validation, an async load step (`load_dynamic_clusters(&bootstrap).await` — file I/O via `tokio::fs`, the existing foundation) reads the CDS file and produces the dynamic cluster list. The **effective cluster list** (static + dynamic, collision-checked) is what every downstream consumer iterates: `ClusterManager::from_bootstrap` (`cluster.rs:721-826`), `H1PoolManager::for_bootstrap` / `H2PoolManager::for_bootstrap`, the per-cluster upstream-TLS loop (`main.rs:187-210`), the `envoy-health` scheduler, the `OutlierManager`, and the route-reference / `Http2ClusterFromHttp1Listener` validators. The PLAN-writer picks the concrete mechanism (a `Bootstrap::all_clusters()` accessor over a `dynamic_clusters: Vec<Cluster>` side-field populated post-parse, vs threading a merged `Vec<Cluster>` through `main.rs`) — the invariant is §5.3: dynamic clusters are full Clusters, indistinguishable downstream.

### D4 — `cluster_manager.*` stats (conditional registration)

The §2.1 stat subset, registered against `StatsRegistry` ONLY when `dynamic_resources.cds_config` is configured (the §5.2 invariant). The `cds.update_success` increment fires once at successful initial load; `cds.update_failure` at load failure (which, per the §6.2-item-4 disposition, either fails startup [envoy-rust posture if Envoy also refuses] or warns-and-continues [if Envoy does]). Registration site: `ClusterManager::from_bootstrap` (the existing per-cluster stat registration site, extended with the manager-level family).

### D5 — `/config_dump` `ClustersConfigDump` section (conditional emission)

`crates/envoy-admin/src/endpoint.rs` `ConfigDumpEntry` enum (at `:303-309`; the xDS-deferral comment at `:297-300` names this exact landing) gains a `Clusters` variant rendering the §6.2-item-5-verified shape (projected: `{"@type": ".../ClustersConfigDump", "version_info": ..., "static_clusters": [...], "dynamic_active_clusters": [{"cluster": {...}, "last_updated": ...}]}`). Emitted ONLY when `dynamic_resources` is configured (fixture 0014 untouched). The `AdminHandler` needs access to the dynamic cluster configs — the PLAN-writer threads either the effective Bootstrap or a dynamic-clusters handle through `AdminHandler::new` (widening its signature; the 08.2 D13b precedent).

### D6 — Harness CDS-file rendering + container mounting

`tests/differential/` gains the per-fixture auxiliary-file capability scoped to CDS: when a fixture directory carries `cds.yaml`, the harness renders it TWICE with the same per-side substitution maps used for the main configs (`{{BACKEND_HOST}}`/`{{HTTP1_BACKEND_PORT}}` etc.), writes each rendition to a temp file, copies the upstream side's rendition into the Envoy container (the TLS-PKI `with_copy_to` pattern at `upstream.rs:104-113`), and substitutes a new `{{CDS_PATH}}` key into each side's main config (container path for Envoy; host temp path for envoy-rust). No new Driver variant — fixture 0026 uses the existing `Driver::Http1KeepAlive` (data-plane probes + named-stat scrape).

### D7 — Fixture 0026 + Docker wrapper

`tests/fixtures/0026-xds-file-based-cds/` carrying `envoy.yaml` + `envoy-rust.yaml` (static listener + HCM route → `dynamic_backend`; `dynamic_resources.cds_config.path_config_source.path: {{CDS_PATH}}`; **zero static clusters**) + `cds.yaml` (the shared CDS-file template defining `dynamic_backend`) + `expectations.yaml` (the §1 probe list) + `README.md`. Docker-gated wrapper test at `tests/differential/tests/xds_file_based_cds.rs`.

### D8 — In-process backstop + fuzz seed + BEHAVIOR_CONTRACT extensions

In-process backstop at `crates/envoy-bin/tests/xds_file_based_cds.rs` (start envoy-rust with a temp CDS file; assert the data-plane 200 through the dynamic cluster + the cds stats + the config_dump entry; plus the negative paths: missing file / malformed file / static-dynamic name collision per the §6.2-item-4-verified disposition). Fuzz seed `dynamic_resources_cds.yaml` (corpus 28 → 29). BEHAVIOR_CONTRACT: the §2.1 stat rows + the §2.2 xDS-section first population + the `ClustersConfigDump` admin-body-shapes row.

---

## 4. Out of scope (deferred non-goals)

Each deferred item below is rejected by `#[serde(deny_unknown_fields)]` today (a bootstrap configuring it fails parse loudly — nothing is silently under-implemented). This is the xDS family's opening deferred-surface ledger:

- **CDS file WATCHING / hot reload** (inotify/poll on the `path_config_source` file; cluster add/update/remove at runtime; the `ClusterManager` mutability refactor it requires; cluster warming + `warming_clusters`; `cds.update_*` on re-load; the `watched_directory` field). **Owner: the family's prime follow-up phase** (`19-xds-file-watch` or similar).
- **File-based LDS** (`dynamic_resources.lds_config`) + **RDS** (`rds` on the HCM) + **EDS** (`eds_cluster_config`) + **SDS** (secrets) + **RTDS** (runtime) — each a future family phase, in whatever order later brainstorms pick.
- **The gRPC xDS transport** (`api_config_source` / `ads_config`; `tonic` + `envoy-protos`/prost codegen; the ADS state machine: version/nonce tracking, ACK/NACK, reconnection, initial-fetch timeout; an in-harness xDS control plane). **This is the phase where the ADR-0014 YAML-shim → protos supersession fires** — explicitly NOT this phase.
- **Delta xDS** (incremental discovery).
- **`initial_fetch_timeout`** (meaningless for a synchronous startup file read; becomes meaningful with the gRPC transport).
- **REST xDS** (Envoy's legacy REST transport — likely never; would need its own ADR).
- **Multi-priority / locality-weighted dynamic endpoints** (the LoadAssignment subset stays exactly what static clusters support today).

---

## 5. Architectural invariants

### 5.1 No new crate, no new top-level Cargo dep

File I/O = `tokio::fs` (existing foundation); YAML parsing = `serde_yaml` (existing); the CDS envelope = serde structs (existing pattern). D-3.2's "xDS state machine written from scratch" requirement is honored trivially — there is no state machine at filesystem-transport scope (a one-shot file read), and the gRPC-phase state machine will be written from scratch when that phase lands.

### 5.2 Inert-when-unconfigured (the foundation-slice discipline)

No `dynamic_resources` in the bootstrap → zero new stats registered, zero new config_dump entries, zero behavior change. All 25 existing fixtures are byte-identical in expectations and wire behavior. (The phase-15/17 conditional-registration precedent.)

### 5.3 Dynamic clusters are full Clusters

Every downstream subsystem — pools, upstream TLS, active health checks, outlier detection, circuit-breaker budgets, retries, route validation — treats a dynamically-loaded cluster identically to a static one, because the merge happens at config-load time BEFORE any consumer iterates the cluster list. No subsystem carries a "is this cluster dynamic?" branch (the only dynamic-aware consumers are the `cds.*` stats and the config_dump renderer).

### 5.4 Load-at-config-time ownership boundary (ADR-0048)

CDS file parsing lives in `envoy-config` (it produces `Vec<Cluster>` configs — config-domain objects). The merge into the effective cluster list happens at config-load time in the startup path. `ClusterManager` remains **immutable post-construction** (the existing architecture) — this phase adds no mutability, no locks, no watch tasks.

### 5.5 config_dump separation

Dynamically-loaded clusters appear in the `ClustersConfigDump` entry (`dynamic_active_clusters`), NOT inside `BootstrapConfigDump.bootstrap.static_resources.clusters`. The BootstrapConfigDump renders the bootstrap as parsed from disk (with its `dynamic_resources` block visible and its empty static cluster list) — the §6.2 item-5 verification confirms Envoy's exact split.

### 5.6 One-shot load; zero timing sensitivity

The CDS file is read exactly once, synchronously within startup, before listeners bind. Readiness implies loaded on both proxies (§6.2 item 2 verifies Envoy's ordering). The fixture needs no settle window, no polling, no sleeps beyond the existing readiness probe.

---

## 6. Implementation signposts for the planner

### 6.1 Split-gate evaluation (split projected NOT to fire)

Projected surface: D1 schema ~200 LoC (+~150 tests); D2 CDS parsing ~120 (+~120 tests); D3 merge + call-site migration ~80 (+~80 tests); D4 stats ~80 (+~60 tests); D5 config_dump ~80 (+~60 tests); D6 harness ~120 (+~40 tests); D7 fixture ~200; D8 backstop + seed + contract ~150. **Total ~1000–1450 LoC / ~11–13 tasks** — under the ~1500-LoC / ~25-task gate. If the §6.2-refined estimate fires the gate anyway, split at the §1 NOTE seam (`18.1` foundation slice / `18.2` observability + fixture + close) with ADR-0050.

### 6.2 Empirical verification at state-2 PLAN-write (HEAVY for this phase)

The state-2 PLAN-writer dispatches a single foreground general-purpose subagent (the ADR-0037/0041/0043/0045/0047 methodology) running `envoyproxy/envoy:v1.33.0` under Docker with a CDS-configured bootstrap + a sibling backend container + admin `/stats` + `/config_dump` scrapes, and verifies:

1. **The CDS file envelope shape Envoy accepts** (the most consequential item — the D2 parser is built to this): bare `resources:` list vs full `DiscoveryResponse` (`version_info` + `resources`)? YAML accepted (by extension sniffing)? Is the `@type` field required per resource? Capture the exact minimal working file byte-for-byte.
2. **Initial-load/readiness ordering:** with a valid CDS file present at startup, is the dynamic cluster routable by the time `/ready` returns 200? Does Envoy block worker/listener start on the initial CDS load (the documented initial-fetch semantics) for path sources?
3. **The exact `cluster_manager.cds.*` + `cluster_manager.*` stat names + values after a successful initial load** (update_success? update_attempt? version? config_reload? cluster_added/active_clusters/warming_clusters?). The §2.1 subset is locked from this enumeration.
4. **Missing/malformed CDS file behavior:** does Envoy fail startup, or warn-and-serve (503 on the route), or block readiness? Which stat ticks (update_failure vs update_rejected)? This locks envoy-rust's negative-path disposition (D8 backstop) — projected: envoy-rust mirrors Envoy.
5. **The `/config_dump` shape with dynamic clusters:** the exact `ClustersConfigDump` JSON (version_info, static_clusters, dynamic_active_clusters[].{cluster, last_updated}); whether the entry appears when `dynamic_resources` is absent (it does today per fixture 0014's allow-list — confirm the with-CDS delta).
6. **Route-to-dynamic-cluster wire shape:** a GET through a dynamically-loaded cluster — identical to the static-cluster shape (200 + echo body + `x-envoy-upstream-service-time`)? Any new response header or access-log flag?
7. **Listener/cluster coexistence:** `static_resources.listeners` + `dynamic_resources.cds_config` with ZERO static clusters is a valid Envoy bootstrap (the fixture topology depends on this)? Or does Envoy require `static_resources.clusters: []` explicitly?
8. **`resource_api_version`:** required or defaulted to V3 on the config source?
9. **Static/dynamic name collision:** a cluster defined both statically and in the CDS file — does Envoy reject at startup, last-write-wins, or first-write-wins? (envoy-rust projects reject-at-validation; the BEHAVIOR_CONTRACT records Envoy's posture.)
10. **Stat conditionality:** which of the §2.1 names does Envoy emit when `dynamic_resources` is ABSENT (the inertness cross-check for fixture 0011 — confirms the conditional-registration carve is recorded accurately)?
11. **(Opportunistic) File hot-reload latency:** modify the CDS file mid-run — does Envoy pick it up, and how fast? (Confirms the deferred-watch scope is a real deferral and informs the follow-up phase's SPEC; no phase-18 deliverable depends on this.)

If item 1, 4, or 5 diverges materially from the projections → land **ADR-0049** at the PLAN-write commit (mirrors ADR-0037/0041/0043/0045/0047).

### 6.3 In-process backstop assertions (heeds the 14.2/15/16/17 both-paths lesson)

The backstop covers BOTH the happy path (valid file → 200 + stats + config_dump) AND the negative paths (missing file; malformed file; static/dynamic name collision) per the §6.2-item-4/9-verified dispositions — the paths the differential fixture cannot exercise (a deliberately-broken Envoy-side fixture is not a thing this project does).

### 6.4 The 06.x stats convention + the inert-when-unconfigured discipline

Stat handles are `Arc<Counter>`/`Arc<Gauge>` registered once at construction; increments at single sites (one source of truth). Conditional registration per §5.2.

### 6.5 Pre-state-4 fmt + clippy discipline (heeds `project_state3_arc_skips_clippy`)

`cargo clippy --workspace --all-targets --all-features -- -D warnings` runs PER TASK in the state-3 arc. The D3 call-site migration (N consumers iterating a merged list) is a `needless_borrow`/iterator-lint candidate.

### 6.6 State-4 evidence-discipline (continues per 05.3 → … → 17 chain)

Per-gate command outputs quoted into PROGRESS Task-N; a single Docker-gated CI run as the anchor.

### 6.7 Isolated-crate build discipline (heeds `project_isolated_crate_build_blindspot`)

The state-4 verification MUST run `cargo build -p envoy-config -p envoy-cluster -p envoy-http1 -p envoy-http2` standalone in addition to the workspace build (feature unification hides missing per-crate features; envoy-config gains `tokio::fs` usage this phase — a candidate for exactly this blind spot).

### 6.8 Cargo.lock cadence

No new top-level deps projected → no Cargo.lock churn beyond version bumps already in flight. If `tokio`'s `fs` feature is not yet enabled on `envoy-config`'s dep line, that feature addition is part of D2/D3 (not a new dep).

### 6.9 PLAN.md + PROGRESS.md skeleton + Task 1 preamble land alongside at state-2

The 06.2→17 standalone-PLAN cadence: one pre-Task-1 docs-only commit (PLAN + PROGRESS skeleton + Task 1 preamble + ROADMAP flip + STATE advance + any §6.2 ADR).

### 6.10 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The state-3 arc dispatches PLAN tasks to fresh subagents SERIALLY (`feedback_serial_subagent_dispatch`), each with two-stage review (spec-compliance THEN code-quality), TDD per task, one code commit + one PROGRESS commit per task.

---

## 7. Conditional ADRs (projected; land at PLAN-write or in-execution if they fire)

- **ADR-0048 (the scoping ADR) — LANDS AT THIS BRAINSTORM COMMIT.** The family pick + the §0 findings + the minimum-viable scope boundary + the deferral ledger. (The ADR-0042/0044/0046 brainstorm-time cadence.)
- **ADR-0049 (§6.2 empirical-verification reconciliation) — PLAUSIBLE.** Fires if §6.2 item 1 (the CDS file envelope shape — the most likely trigger: the parser is built to this), item 4 (missing/malformed-file disposition), or item 5 (the ClustersConfigDump shape) diverges materially from the projections. Lands at the state-2 PLAN-write commit. Mirrors ADR-0037/0041/0043/0045/0047.
- **ADR-0050 (phase split) — POSSIBLE (projected NOT to fire).** Fires only if the §6.2-refined estimate exceeds ~1500 LoC / ~25 tasks. Seam per §1 NOTE / §6.1. Mirrors ADR-0036/0038/0040.

---

## 8. Summary

Phase 18 opens the **xDS / dynamic config family** — the first new §9 family since Upstream-robustness — at its lowest-risk entry point: **file-based CDS**. A bootstrap pointing `dynamic_resources.cds_config.path_config_source` at a YAML file gets its clusters loaded at startup, routable on the data plane, and observable via `cluster_manager.cds.*` stats and `/config_dump`'s `ClustersConfigDump` — bilaterally verified by fixture `0026-xds-file-based-cds` with zero concurrency, zero timing sensitivity, zero new crates, and zero new dependencies. The hard xDS surfaces (file watching, LDS/RDS/EDS/SDS/RTDS, the gRPC/ADS state machine, delta) are cleanly deferred with named owners, and the ADR-0014 YAML-shim → protos supersession explicitly waits for the gRPC-xDS phase.
