# Phase 20 (`20-xds-file-based-rds`) — SPEC

- **Phase id:** `20`
- **Slug:** `20-xds-file-based-rds`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `9fc072928`, the phase-19 state-6 deterministic close-out commit; the "xDS / dynamic config family" §9 table carries exactly TWO rows — `18` and `19`, both `done`). **This SPEC's landing commit adds the THIRD concrete row beneath the "xDS / dynamic config family" heading**, with `status: planned` — the family's second continuation phase, completing the core data-plane dynamic-resource triad (clusters + listeners + routes) over the filesystem transport.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"xDS / dynamic config family — ADS, delta xDS, LDS, CDS, RDS, EDS, SDS, RTDS, reconnection, initial-fetch timeout."* This phase lands the family's **filesystem-transport RDS member**: a `HttpConnectionManagerConfig.rds.config_source.path_config_source` — route configurations loaded from a local file at startup, observable via the per-HCM `http.<stat_prefix>.rds.<route_config_name>.*` stat tree, the `RoutesConfigDump` admin section, and (most importantly) the data plane: an HCM whose route table exists ONLY in the RDS file routes traffic bilaterally. With phases 18 (CDS) + 19 (LDS) already done, this phase completes **Envoy's core data-plane dynamic-resource triad over the filesystem transport** — listeners, their routes, and clusters all sourced from files — bilaterally proven by fixture 0028.
- **Position in the project:** the **twelfth post-MVP-trunk feature-family phase** and the **third concrete xDS-family phase**. The MVP trunk 00→08, the three HTTP-filter-family phases (09/10/11), the six Upstream-robustness-family phases (12/13/14/15/16/17), and the two xDS-family phases (18 CDS, 19 LDS) all stand `done`. The **27-Docker-gated-fixture regression baseline** established at phase-19 close (`0001-tcp-echo` through `0027-xds-file-based-lds`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **depends-on:** `01 02 04 06 08 18` — phase `01` (the `envoy-config` bootstrap loader the `rds` field extends), phase `02` (the listener + cluster runtime), phase `04` (the HCM + `RouteConfiguration` + router whose route table this phase makes dynamically loadable), phase `06` (the `envoy-stats` foundation the per-HCM `rds.*` stats register against), phase `08` (the admin `/config_dump` endpoint + `ConfigDumpEntry` enum the `RoutesConfigDump` section extends), and phase `18` (the `dynamic_resources`/`ConfigSource`/`PathConfigSource` schema, the `cds.rs`/`lds.rs` `@type`-tagged envelope-parser pattern, `load_dynamic_resources`, and the harness dynamic-file rendering/mounting machinery). **Phase `19` (LDS) is a reuse source but NOT a hard dependency** — RDS anchors on a STATIC listener (the minimum-viable fixture), exactly as fixture 0026 anchored CDS on a static listener; the LDS+RDS composition is a deferral (§4).
- **Brainstorm narrative:** see the "Phase-20 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the continuation-pick rationale and the alternatives weighed (CDS/LDS file watching/hot reload [the ledger's nominal prime follow-up — rejected again on the three stacked ADR-0050 risks: the `ClusterManager`/listener-manager mutability refactor, the watch-convergence timing sensitivity, and the macOS-Docker-Desktop §6.2-verification blocker recorded in ADR-0049's Provenance — its ROI strictly improves by landing RDS first]; file-based EDS [strong runner-up — deeper schema surgery: `ClusterType::Eds` + making the REQUIRED `Cluster.load_assignment` field optional]; the gRPC family [still blocked on H2 trailers — re-verified at HEAD `9fc072928`: `crates/envoy-http1/src/client.rs:239-248` discards trailers, `envoy-http2` exposes no trailer API]; the Load-balancing / Observability / HTTP-3+QUIC / WASM-host / Network-filters / Runtime families [the phase-18 ADR-0048 rejection analysis carries unchanged]). The scoping decision is ratified in **ADR-0051** (landed at this brainstorm commit).

---

## 0. Critical scoping findings (READ FIRST) — RDS reuses the filesystem-transport machinery but introduces the first HCM-scoped dynamic resource

Phases 18 + 19 built the filesystem xDS transport for two BOOTSTRAP-scoped resource types (clusters under `dynamic_resources.cds_config`, listeners under `dynamic_resources.lds_config`). The state-1 brainstorm identified four findings that make file-based RDS a **single, bounded phase** — reusing the proven envelope/merge/harness machinery, but with three genuinely-new surfaces that make it modestly heavier than the LDS phase:

1. **The config-source machinery and the envelope parser are reused; the new schema shape is the HCM `route_config` XOR `rds` mutual exclusivity — the FIRST HCM-scoped dynamic resource.** The `ConfigSource`/`PathConfigSource` structs (`crates/envoy-config/src/bootstrap.rs:94-123`) are resource-type-agnostic and reused verbatim — `Rds` embeds a `config_source: ConfigSource`. The `@type`-tagged envelope parser (`crates/envoy-config/src/cds.rs:42-57` + `crates/envoy-config/src/lds.rs:36-53`) generalizes to a RouteConfiguration-resource variant (`@type: type.googleapis.com/envoy.config.route.v3.RouteConfiguration`) — the per-resource payload is exactly the `RouteConfiguration` struct `envoy-config` already parses for the inline `route_config` (`bootstrap.rs:1017-1030`: `name`, `virtual_hosts`, `validate_clusters` — everything an RDS payload carries). **The novelty: RDS is configured ON THE HCM (`rds:` replacing the inline `route_config:`), NOT under bootstrap-level `dynamic_resources`** (`HttpConnectionManagerConfig` at `bootstrap.rs:524-549` has `route_config: RouteConfiguration` as a REQUIRED field today). Phase 20 makes `route_config` optional and adds a sibling `rds: Option<Rds>`, validated exactly-one-of. This is the first *HCM-scoped* (vs bootstrap-scoped) config source — the topology EDS (`eds_cluster_config` on a cluster) and SDS (`sds_config` on a transport socket) also use.

2. **The merge-into-effective-`route_config` design needs NO HCM runtime refactor.** Rather than threading an RDS-aware branch through the HCM dispatch path, `route_config` becomes `Option<RouteConfiguration>` and the RDS file is loaded at config-load time: for each HCM whose `rds` is configured, read the file, select the `RouteConfiguration` matching `route_config_name`, and POPULATE the effective `route_config = Some(loaded)`. Downstream HCM dispatch (`crates/envoy-http1/src/hcm.rs:200` `clone_route_config(&cfg.route_config)`; the virtual-host match at `hcm.rs:1177`; the H2 mirror) reads a populated `route_config` exactly as today — the only consumer change is an Option-unwrap guarded by the post-load invariant (every HCM has a resolved `route_config` after load, whether static or RDS-supplied). This mirrors the phase-18/19 "merge dynamic into the effective list at config-load time, downstream sees a uniform shape" design (ADR-0048 finding 2 / ADR-0050 finding 2). **No runtime route-table mutability, no locks, no watch tasks** — the RDS file is read once, synchronously, at startup.

3. **RDS stats are PER-HCM-SCOPED (`http.<stat_prefix>.rds.<route_config_name>.*`) — a new registration topology, distinct from the manager-level CDS/LDS families.** The phase-18 `cluster_manager.cds.*` and phase-19 `listener_manager.lds.*` families are process-level singletons. RDS's stat names embed BOTH the HCM's `stat_prefix` AND the `route_config_name`, registered per-HCM at the site that already consumes `stat_prefix` (`crates/envoy-http1/src/hcm.rs:71-97` `HCMStats::register(registry, stat_prefix)`). This is the genuinely-new surface and the reason phase 20 is modestly heavier than phase 19 (the conditional registration is keyed on a per-HCM field rather than a process-level `dynamic_resources` predicate). The conditional-registration TECHNIQUE is reused (the phase-18 template at `crates/envoy-cluster/src/cluster.rs:1060-1097`), but the predicate is `HttpConnectionManagerConfig.rds.is_some()`.

4. **No pre-existing allow-listed Envoy-side RDS surface — the §0-finding-3 head start does NOT apply.** Phases 18 + 19 each inherited a head start: fixture 0011 already allow-listed the `cluster_manager.*` / `listener_manager.*` names as Envoy-only, so those phases only TIGHTENED assertions. Fixture 0011 configures NO RDS (its HCM carries an inline `route_config`), so Envoy emits no `http.<prefix>.rds.*` stats there — the RDS fixture (0028) drives its bilateral assertions from scratch. This is a bounded cost (the fixture authors the `rds.*` stat assertions directly) — but it means the §6.2 verification of the exact per-HCM stat-name set is load-bearing (there is no fixture-0011 enumeration to crib from). **In exchange, RDS completes the core data-plane dynamic-resource triad** (CDS clusters + LDS listeners + RDS routes over the filesystem) AND introduces the **named/scoped config-source idiom** (`route_config_name` selects a resource by name from the file) that EDS (`service_name`) and SDS (secret name) reuse — making RDS the architecturally-correct next step.

**Consequence:** phase 20 needs **NO new crate, NO new top-level Cargo dep, NO new harness driver, NO new helper binary, and NO concurrency/timing machinery** — the RDS file load is synchronous at startup (the phase-18/19 `std::fs` posture), so the fixture is deterministic and timing-robust (readiness implies loaded). Projected surface is **modestly larger than phase 19** (~1100–1450 LoC vs phase 19's ~1250–1450) because the HCM `route_config`→`Option` schema surgery, the per-HCM stat topology, and the effective-`route_config` threading are each a first build rather than a second instantiation — but still comfortably a single un-split phase under the §6.1 gate.

These findings are ratified in **ADR-0051** (landed at this brainstorm commit).

---

## 1. Goal and acceptance signal

Phase 20 makes **file-based dynamic route discovery (RDS over the filesystem transport) work end-to-end**. When an HCM configures `rds.route_config_name` + `rds.config_source.path_config_source.path` instead of an inline `route_config`, both upstream Envoy and envoy-rust:

- **load the RouteConfiguration named by `route_config_name` from that file at startup** (initial load; before serving traffic),
- **route data-plane traffic through it** exactly as if it had been defined inline (the full route-match + router + upstream machinery applies),
- **expose the load observably**: the per-HCM `http.<stat_prefix>.rds.<route_config_name>.*` stat subset (§6.2-verified) and the `/config_dump` `RoutesConfigDump` section listing the dynamically-loaded route configuration.

**Differential surface added by phase 20:**

- **Fixture `0028-xds-file-based-rds`** — an HCM whose route table is RDS-supplied, bilaterally asserted. Both proxies receive identical bootstraps whose `static_resources` carries **one static listener (HTTP/1.1 HCM, `stat_prefix: ingress_http`, `rds: { route_config_name: local_route, config_source: { path_config_source: { path: <RDS_PATH> } } }`, NO inline `route_config`), one static cluster (`static_backend`)**, and ALSO `dynamic_resources.cds_config` pointing at a `cds.yaml` (one cluster `dynamic_backend`, reusing the proven phase-18 machinery — so the fixture exercises the merge-ordering invariant §5.7). The RDS file defines one RouteConfiguration (`local_route`) with two routes. Probes (all via the existing `Driver::Http1KeepAlive`):
  1. **Data plane, RDS isolation (the load-bearing probe):** `GET /static` → **200** + the `http1-echo-server` echo body **byte-exact** bilaterally + `x-envoy-upstream-service-time` present, routed through the **static** cluster via the RDS-supplied route. Without the RDS load, there is NO route table — the probe discriminates loaded-from-not-loaded.
  2. **Data plane, RDS+CDS composition (the merge-ordering probe):** `GET /dynamic` → **200** + echo body byte-exact bilaterally, routed through the **CDS-supplied** cluster — a request whose route AND cluster both exist only in dynamic-resource files (exercising §5.7: dynamic clusters merge BEFORE the RDS-supplied route's references are re-validated).
  3. **Stats:** `http.ingress_http.rds.local_route.update_success: 1` + `…update_failure: 0` (+ the §6.2-verified subset, incl. `…update_attempt` and the per-cluster discriminators `cluster.static_backend.upstream_rq_total: 1` / `cluster.dynamic_backend.upstream_rq_total: 1`) asserted via the named-stat scrape. The phase-18 `cluster_manager.cds.*` names are asserted too (they come for free on this topology).
  4. **Admin scrape:** `/config_dump` `RoutesConfigDump` entry naming `local_route` (the §6.2-item-5-verified JSON shape), alongside the existing `ClustersConfigDump` assertion shape from fixture 0026.

  The discriminating differential observables are the **route-table-from-file data-plane success + the RDS update counters** — a proxy that ignored `rds` would have an empty/absent route table (404/`no route`/startup failure) and emit no `rds.*` stats. Probe shape, exact stat names, and the config_dump assertion are §6.2-verified projections (§6.2 items 1–5).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0028-xds-file-based-rds` green at Docker-gated CI.
- **(b)** All **27 pre-existing differential fixtures** (`0001` through `0027`) **remain green simultaneously** at the same CI run (regression-equivalence per §7.5 (b)). The RDS machinery is inert when no HCM configures `rds`: making `route_config` optional keeps every existing inline-`route_config` fixture parsing identically (deserializes to `Some`); the new `rds.*` stats register ONLY when an HCM's `rds` is present (the phase-15/17/18/19 conditional-registration discipline); the `RoutesConfigDump` entry is emitted ONLY when some HCM uses `rds` (fixtures 0014 + 0026 + 0027 untouched); fixture 0011's Prometheus set-diff sees zero new envoy-rust names.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline). Phase 20 does not touch HTTP framing.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (new seed `hcm_rds_route_config.yaml`; git-tracked curated corpus 30 → 31; the new-seed atomic edit: fuzz `.gitignore` allow-list + the SUCCESS-array together — the 09→19 lesson; the corpus is fully consistent entering this phase per the phase-19 carryforward-disposition-1 closure).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` (run PER TASK in the state-3 arc, per `project_state3_arc_skips_clippy`), `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean — plus the 4 standalone-crate builds (`-p envoy-config` / `-p envoy-cluster` / `-p envoy-http1` / `-p envoy-http2`) per `project_isolated_crate_build_blindspot`.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent).

> **NOTE — single phase projected (see §6.1).** Phase 20's surface (the `rds` schema + the `route_config`→`Option` migration + RDS-file parsing + the effective-`route_config` merge + ordering + the per-HCM `rds.*` stats + the `RoutesConfigDump` section + the harness `{{RDS_PATH}}` generalization + fixture 0028 + in-process backstop + fuzz seed + BEHAVIOR_CONTRACT rows) is projected at **~1100–1450 LoC / ~11–13 tasks** — under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task split gate, with LESS margin than phase 19 carried (the HCM `Option` surgery + the per-HCM stat topology + the effective-`route_config` threading are first builds). The recommended split seam if the §6.2-refined estimate fires the gate anyway: **`20.1`** (the `rds` schema + the `route_config`→`Option` migration + RDS-file parsing + the effective-`route_config` merge + ordering + in-process backstop + fuzz seed — the foundation slice, regression-equivalence acceptance) / **`20.2`** (the per-HCM `rds.*` stats + `RoutesConfigDump` + harness extension + fixture 0028 + parent-20 close). The split ADR would be ADR-0053 (§7).

---

## 2. Behavior-contract scope for phase 20

Phase 20 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x→19 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — RDS / per-HCM subset (projected; §6.2-verified)

New rows, mirroring upstream Envoy v1.33's documented per-HCM RDS stat tree. **Minimum-viable subset** (the 14.1/15/16/17/18/19 namespace-subset precedent): emit the names Envoy emits for the behavior envoy-rust implements; allow-list the rest. **Note the per-HCM scoping** — every name is prefixed `http.<stat_prefix>.rds.<route_config_name>.` (fixture 0028: `http.ingress_http.rds.local_route.`).

| Stat name (relative to `http.<stat_prefix>.rds.<route_config_name>.`) | Kind | Equivalence (projected; §6.2-verified) | Rationale |
|---|---|---|---|
| `update_attempt` | counter | value-exact | +1 per RDS update attempt. At initial-load-only scope, exactly `1` after startup. §6.2 item 3 verifies (the phase-18/19 `update_attempt` precedent). |
| `update_success` | counter | value-exact | +1 per successful RDS update. Fixture 0028: `1`. |
| `update_failure` | counter | value-exact (0-case) | +1 per failed RDS update. Fixture 0028 asserts `0`. Structurally unreachable non-zero in envoy-rust if the all-fatal posture mirrors CDS/LDS (ADR-0049 decision 2 / ADR-0050 precedent; §6.2 item 4 verifies Envoy's RDS negative-path split). |
| `update_rejected` | counter | value-exact (0-case) | +1 per semantically-rejected RDS update. Fixture 0028 asserts `0`. Same posture note. |
| `config_reload` | counter | value-exact (projected; §6.2 item 3 verifies presence + value) | +1 per route-config version applied. Fixture 0028: projected `1` at initial load. §6.2 item 3 confirms whether Envoy ticks this at initial load (vs only on re-load) — if it does NOT tick deterministically at initial-load-only scope, it drops from the subset. |

**Conditional registration (the §5.2 invariant):** the `rds.*` names register ONLY when the owning HCM's `rds` is configured (the per-HCM predicate per §0 finding 3). This is a deliberate, BEHAVIOR_CONTRACT-recorded narrowing vs Envoy (which emits the per-HCM RDS family whenever an HCM uses RDS): the inline-`route_config` HCMs emit no `rds.*` names. All 27 existing fixtures (whose HCMs carry inline `route_config`) see zero new envoy-rust names, preserving the regression baseline with zero edits.

### 2.2 "xDS wire state machine" section — RDS extension of the filesystem-transport subsection

The BEHAVIOR_CONTRACT's "Filesystem transport (`path_config_source`)" subsection (first populated at phase 18, extended for LDS at phase 19) gains RDS rows: (a) the RDS file envelope shape Envoy accepts (§6.2 item 1's finding — projected to mirror the CDS/LDS envelope with `@type: type.googleapis.com/envoy.config.route.v3.RouteConfiguration`), (b) the `rds`-on-HCM config shape (`route_config_name` + `config_source`; §6.2 item 1b), (c) the initial-load/readiness ordering for route tables (§6.2 item 2), (d) the missing/malformed-RDS-file + `route_config_name`-mismatch disposition + whether envoy-rust's all-fatal posture diverges (§6.2 item 4/6; the ADR-0049 decision-2 recorded-divergence pattern), and (e) the RDS+CDS composition ordering semantics (§6.2 item 7). The `RoutesConfigDump` shape lands in the "Admin endpoint body shapes" section as a new row (§6.2 item 5 supplies the JSON shape + the `configs[]` index).

### 2.3 DECISIONS.md amendment at SPEC time — ADR-0051 (the scoping ADR)

Like phases 15 (ADR-0042), 16 (ADR-0044), 17 (ADR-0046), 18 (ADR-0048), and 19 (ADR-0050), phase 20's brainstorm DOES land an ADR: **ADR-0051** records (a) the **continuation pick** (file-based RDS over CDS/LDS file watching [three stacked risks], file-based EDS [deeper schema surgery], the gRPC family [still blocked on H2 trailers — re-verified at HEAD], and the other §9 families [the phase-18 rejection analysis carries]) with the alternatives weighed, (b) the four §0 findings, and (c) the minimum-viable scope boundary — deliver file-based RDS initial load + the per-HCM `rds.*` stat subset + `RoutesConfigDump` + fixture 0028; defer file watching, scoped_routes/SRDS/VHDS, EDS/SDS/RTDS, the gRPC/ADS transport, delta xDS, the LDS+RDS composition showcase, and the ADR-0014 protos supersession. Conditional §6.2-reconciliation + split ADRs are enumerated in §7.

---

## 3. Deliverables

Phase 20's scope is enumerated as deliverables `D1`–`D8` below. **The state-2 PLAN-writer organizes deliverables into tasks AND evaluates the §6.1 split gate** (projected NOT to fire, with less margin than phase 19). Deliverables are LISTED roughly in execution order; the SPEC constrains the surface, not the task organization.

### D1 — `envoy-config` schema extension (`rds` on the HCM; `route_config` → `Option`)

`crates/envoy-config/src/bootstrap.rs` `HttpConnectionManagerConfig` (at `bootstrap.rs:524-549`) changes:

```rust
// route_config becomes optional (was: pub route_config: RouteConfiguration)
#[serde(default, skip_serializing_if = "Option::is_none")]
pub route_config: Option<RouteConfiguration>,
// new sibling — mutually exclusive with route_config
#[serde(default, skip_serializing_if = "Option::is_none")]
pub rds: Option<Rds>,
```

with a new `Rds` struct:

```rust
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rds {
    pub route_config_name: String,
    pub config_source: ConfigSource, // reused verbatim from phase 18
}
```

The validator gains an **exactly-one-of** check: an HCM with neither `route_config` nor `rds`, or with BOTH, is a `ConfigError` (projected variants `MissingRouteSource` / `AmbiguousRouteSource`; the exact set is a PLAN-write decision informed by §6.2). `ConfigSource`'s `api_config_source`/`ads`/`watched_directory` remain rejected by `deny_unknown_fields` (deferred per §4). New `ConfigError` variants for the RDS file/parse failures: projected ~2-3 (`RdsFileError { path, source }`, `RdsParseError`, `RdsRouteConfigNotFound { name }` — mirroring the phase-18/19 file/parse pair plus the named-resource-not-found case; the exact set is a PLAN-write decision informed by §6.2 item 4/6).

> **CARRY-FORWARD WARNING for the state-3 executor (D1):** making `route_config` an `Option` is a workspace-compile-affecting change. YAML fixtures with inline `route_config:` still parse (→ `Some`), but every Rust site that READS `cfg.route_config` (the HCM consumers at `crates/envoy-http1/src/hcm.rs:200`/`1177`, the H2 mirror, the admin config_dump serializer) and every Rust test that CONSTRUCTS `HttpConnectionManagerConfig { route_config: rc, .. }` literally must adapt in the same commit. The phase-18/19 `Bootstrap`/`RouteConfiguration` struct-literal sweep is the precedent (phase 18 hit 2 `Bootstrap` + 26 `RouteConfiguration` literal sites). The PLAN-write's SPEC-correction pass enumerates the exact literal-construction + field-read sites.

### D2 — RDS file parsing (`envoy-config`)

Either a new `crates/envoy-config/src/rds.rs` module (the `cds.rs`/`lds.rs` sibling-module shape) or — preferred if it stays under the LoC budget — a generalization of `cds.rs`+`lds.rs` into a resource-type-parametric `xds_file.rs` (the PLAN-writer's call; the gRPC/ADS phase will need resource-type dispatch anyway, and RDS makes the THIRD copy-paste sibling — the phase-19 REVIEW M19-1 "a generic `parse_xds_file<T>` would dedupe; pays off at a 3rd resource type" item, which is exactly NOW): `parse_rds_file(path, contents) -> Result<Vec<RouteConfiguration>, ConfigError>` parsing the §6.2-item-1-verified envelope shape (projected: the phase-18/19 envelope with `@type: type.googleapis.com/envoy.config.route.v3.RouteConfiguration` per resource; both the bare `resources:` list and the full `DiscoveryResponse` shape accepted; always-YAML parsing per the ADR-0049 decision-1 posture). The named-resource selection (`route_config_name` → the matching `RouteConfiguration`) happens at merge time (D3), not in the parser. Per-RouteConfiguration validation reuses the existing validator functions — dynamic route configs pass through the SAME validation gauntlet as inline ones (virtual-host shape, route-cluster references — deferred per §5.7, matcher shapes).

### D3 — Effective-`route_config` merge + ordering (config-load-time; the §5.4 ownership boundary)

`load_dynamic_resources` (`crates/envoy-config/src/lib.rs:571-656`) gains the RDS pass: **walk every HCM filter across every (static + dynamic) listener's filter chains**; for each HCM whose `rds` is `Some`, read its RDS file → select the `RouteConfiguration` whose `name == rds.route_config_name` (`RdsRouteConfigNotFound` if absent) → **populate that HCM's effective `route_config = Some(selected)`**. **Ordering invariant (§5.7): the CDS merge completes BEFORE the RDS route-reference re-validation runs**, so an RDS-supplied route's cluster references can resolve against dynamic clusters (the fixture-0028 composition). The post-merge re-validation (`bootstrap::validate` at `lib.rs:653`) runs ONCE against the full effective state, after the CDS + LDS + RDS merges. **Consumer-migration sweep — the `route_config` field-read + construction sites** (the brainstorm survey + the PLAN-write's SPEC-correction pass confirm the exact set): the HCM consumers (`crates/envoy-http1/src/hcm.rs:200` `clone_route_config(&cfg.route_config)` + `:1177` vhost match; the `crates/envoy-http2` mirror) unwrap the now-`Option` field under the post-load "every HCM has a resolved route_config" invariant (§5.3); the admin config_dump bootstrap serializer must not emit a raw `rds` block where Envoy emits the resolved/loaded route (a §6.2-item-5 capture). **Interaction with LDS (§5.7 caveat):** if a future phase composes RDS under an LDS-supplied listener, the HCM walk must cover `bootstrap.dynamic_listeners` too — phase 20 anchors on a static listener, but the walk is written over `all_listeners()` so it is composition-ready.

### D4 — Per-HCM `rds.*` stats (conditional registration; the new topology)

The §2.1 stat subset: the `rds.{update_attempt,update_success,update_failure,update_rejected,config_reload}` counters, registered ONLY for HCMs whose `rds` is configured (the §5.2 invariant), keyed on `http.<stat_prefix>.rds.<route_config_name>.`. **Registration site:** the existing per-HCM `HCMStats::register(registry, stat_prefix)` (`crates/envoy-http1/src/hcm.rs:71-97`) — which already builds the `http.<stat_prefix>.*` namespace — gains a conditional `rds.*` sub-registration when the HCM config carries `rds` (passing `route_config_name` for the second name segment). The conditional-registration TECHNIQUE follows the phase-18 template (`crates/envoy-cluster/src/cluster.rs:1060-1097`), but the predicate is per-HCM (`cfg.rds.is_some()`) rather than the process-level `dynamic_resources` predicate. The H2 HCM path (`crates/envoy-http2/src/hcm.rs`) gets the sibling registration. **The increments fire inside `load_dynamic_resources` at load time** (update_attempt/update_success), so the stat handles must be threaded from the registration site to the load site — a PLAN-write threading decision (candidates: register-then-increment-at-load with the registry passed to the loader, or a deferred-increment recorded on the merged config and replayed at HCM construction).

### D5 — `/config_dump` `RoutesConfigDump` section (conditional emission)

`crates/envoy-admin/src/endpoint.rs` `ConfigDumpEntry` enum (at `:301-338`, currently `Bootstrap` + `Clusters` + `Listeners` variants) gains a `Routes` variant rendering the §6.2-item-5-verified shape (projected: `{"@type": ".../RoutesConfigDump", "dynamic_route_configs": [{"route_config": {"@type": ".../RouteConfiguration", "name": "local_route", ...}}], "static_route_configs": [...]}` — §6.2 item 5 captures the exact nesting + whether a `version_info` key appears). Emitted ONLY when some HCM uses `rds` (fixtures 0014 + 0026 + 0027 untouched). The entry-ordering within the `configs` array is a §6.2-item-5 capture — Envoy's verified order through phase 19 is Bootstrap[0], Clusters[1], Listeners[2]; the `RoutesConfigDump` slots at whatever index §6.2 reveals (projected AFTER Listeners per Envoy's documented config_dump map order; fixture 0026's `configs[1]` ClustersConfigDump index assertion and fixture 0027's `configs[2]` ListenersConfigDump assertion must not break — §5.5, since neither configures `rds`).

### D6 — Harness RDS-file rendering + container mounting

`tests/differential/src/lib.rs` generalizes the phase-18/19 dynamic-file machinery (the `{{CDS_PATH}}` handling at `lib.rs:2187-2211`, the `{{LDS_PATH}}` handling at `lib.rs:2217-2249`) to a third file: when a fixture directory carries an RDS template, render it per-side with the same substitution maps, write to temp, mount the upstream rendition into the Envoy container (a path ending in `.yaml` per the ADR-0049 decision-1 constraint), and substitute `{{RDS_PATH}}` into each side's main config. **Whether the RDS template is shared or per-side** (`rds.yaml` vs `rds-envoy.yaml`/`rds-envoy-rust.yaml`) is a §6.2 finding: the phase-19 LDS template went per-side because the HCM payload carried Envoy-only fields (`generate_request_id`, `request_headers_to_remove`) rejected by envoy-rust's `deny_unknown_fields`; an RDS file carries only a `RouteConfiguration` (name + virtual_hosts), which is more likely shareable — but §6.2 item 8 confirms whether Envoy's RouteConfiguration requires any field envoy-rust rejects. The combined-source backend-detection + `uses_host_gateway` scans (the phase-18 carryforward-disposition-2 bug-class lesson: **scan ALL rendered sources**, and the phase-19 M19-5 fail-safe-symmetry note) gain the RDS rendition as a scan source.

### D7 — Fixture 0028 + Docker wrapper

`tests/fixtures/0028-xds-file-based-rds/` carrying `envoy.yaml` + `envoy-rust.yaml` (admin + `node` + one static cluster `static_backend` + one static listener whose HCM uses `rds: { route_config_name: local_route, config_source: { path_config_source: { path: {{RDS_PATH}} } } }` + NO inline `route_config` + `dynamic_resources.cds_config` for the composition probe + `validate_clusters` carried per the ADR-0049 L12 / ADR-0050 L6 precedent — §6.2 item 7 verifies whether the RDS-route context requires it) + the RDS template (shared `rds.yaml` or per-side per §6.2 item 8: RouteConfiguration `local_route`, routes `/static` → `static_backend`, `/dynamic` → `dynamic_backend`) + `cds.yaml` (the CDS-file template: `dynamic_backend`, reusing the fixture-0026 shape verbatim) + `expectations.yaml` (the §1 probe list) + `README.md`. Docker-gated wrapper test at `tests/differential/tests/xds_file_based_rds.rs`.

### D8 — In-process backstop + fuzz seed + BEHAVIOR_CONTRACT extensions

In-process backstop at `crates/envoy-bin/tests/xds_file_based_rds.rs` (start envoy-rust with temp RDS + CDS files; assert the data-plane 200s through both probes + the `rds.*` stats + the config_dump entry; plus the negative paths: missing RDS file / malformed RDS file / `route_config_name` matching no resource in the file / an RDS route referencing a nonexistent cluster / both `route_config` AND `rds` configured / neither configured — per the §6.2-item-4/6-verified dispositions). **Reuse note:** the backstop helper block is copied from `crates/envoy-bin/tests/xds_file_based_lds.rs` — the M18-9 extract-a-test-support-crate item is now N≥4 (CDS, LDS, RDS backstops + the per-fixture `handler_from_bootstrap`/backend-cluster consts); record the duplication in the file header (the extraction stays a future hardening-phase task per the phase-19 carryforward disposition). Fuzz seed `hcm_rds_route_config.yaml` (git-tracked curated corpus 30 → 31). BEHAVIOR_CONTRACT: the §2.1 stat rows + the §2.2 xDS-section RDS extension + the `RoutesConfigDump` admin-body-shapes row.

---

## 4. Out of scope (deferred non-goals)

Each deferred item below is rejected by `#[serde(deny_unknown_fields)]` today (a bootstrap configuring it fails parse loudly — nothing is silently under-implemented). This extends the xDS family's deferred-surface ledger:

- **File WATCHING / hot reload** (for the CDS, LDS, AND RDS files; inotify/poll; route-table/cluster/listener add-update-remove at runtime; the mutability refactors it requires; cluster warming; `*.update_*`/`config_reload` on re-load). **Owner: the family's prime follow-up phase — now with even better ROI** (one watching phase lights up hot reload for all THREE file-based resource types). NOTE (carried from ADR-0049 Provenance): that phase's §6.2 verification MUST run on Linux CI — macOS Docker Desktop's virtiofs/inotify limitation makes file-watch behavior unobservable locally.
- **`scoped_routes` / SRDS** (scoped route discovery) + **VHDS** (virtual-host discovery) — the route-config-sharding surfaces layered on top of RDS; each a future phase.
- **The LDS+RDS composition showcase** (an LDS-supplied listener whose HCM uses RDS). Phase 20 anchors on a STATIC listener (the minimum-viable fixture, mirroring how fixture 0026 anchored CDS on a static listener). The D3 HCM walk is written over `all_listeners()` so it is composition-ready, but the bilateral fixture proving the full LDS+RDS+CDS topology defers.
- **Multiple `rds`-configured HCMs in one bootstrap** (each with its own `route_config_name` / file). Phase 20's fixture has exactly one RDS HCM; the per-HCM stat keying makes N HCMs natural, but the bilateral fixture defers.
- **Multiple RouteConfigurations selected from one RDS file by different HCMs.** Phase 20's RDS file carries the one RouteConfiguration matched by `route_config_name`; the named-selection logic supports more, but the bilateral fixture defers.
- **File-based EDS** (`eds_cluster_config` — needs `ClusterType::Eds` + the REQUIRED `Cluster.load_assignment` field made optional) + **SDS** (secrets) + **RTDS** (runtime) — each a future family phase, in whatever order later brainstorms pick.
- **The gRPC xDS transport** (`api_config_source`/`ads_config`; tonic + envoy-protos/prost; the ADS state machine; an in-harness control plane; **the ADR-0014 protos supersession**) + **delta xDS** + **`initial_fetch_timeout`** + **REST xDS** — all carried unchanged from the phase-18/19 ledger.

---

## 5. Architectural invariants

### 5.1 No new crate, no new top-level Cargo dep

File I/O = `std::fs` (the phase-18/19 sync-load posture — envoy-config keeps zero async deps and the fuzz target stays pure); YAML parsing = `serde_yaml` (existing); the RDS envelope = serde structs (existing pattern).

### 5.2 Inert-when-unconfigured (the foundation-slice discipline)

No HCM with `rds` in the bootstrap → zero new stats registered, zero new config_dump entries, zero behavior change. All 27 existing fixtures are byte-identical in expectations and wire behavior (their HCMs carry inline `route_config`, which still parses to `Some`). The `route_config`→`Option` change is purely additive at the wire level: existing configs deserialize identically. (The phase-15/17/18/19 conditional-registration precedent; fixtures 0026 + 0027 — which configure `cds_config`/`lds_config` but NO `rds` — are the critical inertness witnesses: they must see no `rds.*` names and no `RoutesConfigDump` entry, and their `configs[1]`/`configs[2]` index assertions must hold.)

### 5.3 Dynamic route configs are full RouteConfigurations; every HCM resolves to one

Every downstream subsystem — the route matcher, the router, stats, access logs — reads a populated `route_config` regardless of whether it was inline or RDS-supplied, because the merge happens at config-load time BEFORE envoy-bin constructs the HCM. The post-load invariant: **every HCM has `route_config: Some(_)`** (an HCM with `rds` got it populated by the D3 merge; an inline HCM had it from the start). No HCM dispatch path carries an "is this route config dynamic?" branch (the only RDS-aware consumers are the `rds.*` stats and the config_dump renderer).

### 5.4 Load-at-config-time ownership boundary

RDS file parsing lives in `envoy-config` (it produces `Vec<RouteConfiguration>` configs). The named selection + the merge into the owning HCM's effective `route_config` happens inside `load_dynamic_resources` at config-load time. No runtime route-table mutability, no locks, no watch tasks.

### 5.5 config_dump separation + fixture-0026/0027 stability

RDS-supplied route configs appear in the `RoutesConfigDump` entry, NOT as a raw `rds` block inside `BootstrapConfigDump.bootstrap`. The `RoutesConfigDump` entry's insertion must not break fixture 0026's existing `configs[1]` ClustersConfigDump index assertion or fixture 0027's `configs[2]` ListenersConfigDump index assertion (neither configures `rds`, so neither emits a Routes entry — §6.2 item 5 captures Envoy's entry ordering and confirms the index stability).

### 5.6 One-shot load; zero timing sensitivity

The RDS file is read exactly once, synchronously within startup, before the HCM binds. Readiness implies loaded on both proxies. The fixture needs no settle window beyond the existing readiness probe.

### 5.7 Merge ordering: clusters before route re-validation (extended to RDS routes)

`load_dynamic_resources` merges dynamic CLUSTERS (CDS) and dynamic LISTENERS (LDS), then populates RDS-supplied `route_config`s, then runs the post-merge re-validation ONCE against the full effective state — so an RDS-supplied route's cluster references may resolve against a dynamic cluster (the fixture-0028 composition), a static cluster, or any mix. A route to a cluster in NEITHER list still fails envoy-rust startup (the ADR-0049 decision-4 / ADR-0050 §5.7 defer-then-revalidate posture, now covering RDS-supplied routes too).

### 5.8 Exactly-one-of route source

An HCM declares its route table via EXACTLY ONE of `route_config` (inline) or `rds` (file). Neither → `ConfigError` (a route-less HCM cannot route). Both → `ConfigError` (ambiguous). This is enforced at validation time, before any load (so a malformed bootstrap fails fast, before the RDS file is even read).

---

## 6. Implementation signposts for the planner

### 6.1 Split-gate evaluation (split projected NOT to fire; less margin than phase 19)

Projected surface: D1 schema + `route_config`→`Option` migration ~80 LoC (+~80 tests; the migration touches read+construct sites across crates); D2 RDS parsing ~100 (+~110 tests); D3 HCM-walk merge + named-selection + ordering + consumer migration ~150 (+~120 tests); D4 per-HCM stats + handle threading ~110 (+~80 tests); D5 config_dump ~90 (+~70 tests); D6 harness ~70 (+~30 tests); D7 fixture ~230 (YAML + wrapper); D8 backstop + seed + contract ~190. **Total ~1100–1450 LoC / ~11–13 tasks** — under the ~1500-LoC / ~25-task gate, but with less margin than phase 19 (the three first-build surfaces). If the §6.2-refined estimate fires the gate, split at the §1 NOTE seam (`20.1` foundation slice / `20.2` observability + fixture + close) with ADR-0053.

### 6.2 Empirical verification at state-2 PLAN-write (HEAVY for this phase)

The state-2 PLAN-writer dispatches a single foreground general-purpose subagent (the ADR-0037/0041/0043/0045/0047/0049 methodology) running `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) under Docker with an `rds`-configured HCM (+ a `cds_config` for the composition) + a host backend + admin `/stats` + `/config_dump` scrapes, and verifies:

1. **The RDS file envelope shape Envoy accepts** (the most consequential item — the D2 parser is built to this): `@type: type.googleapis.com/envoy.config.route.v3.RouteConfiguration` per resource? Bare `resources:` list AND full `DiscoveryResponse` both accepted (the CDS/LDS L1 finding's mirror)? Capture the exact minimal working file byte-for-byte. **(1b)** the `rds`-on-HCM config shape: `rds: { route_config_name, config_source: { path_config_source: { path }, resource_api_version? } }` — exact field names + whether `resource_api_version` is required.
2. **Initial-load/readiness ordering:** is the RDS-supplied route table active by the time `/ready` returns 200? Does Envoy serve the route at first request without a warm-up window?
3. **The exact per-HCM `http.<stat_prefix>.rds.<route_config_name>.*` stat names + values after a successful initial RDS load** (update_attempt/update_success? config_reload? version? the exact relative names). The §2.1 subset is locked from this enumeration. **Confirm `config_reload` ticks at INITIAL load** (vs only on re-load) — if not, it drops from the subset. Cross-check which names exist WITHOUT `rds` (an inline-route HCM — the conditionality carve for §5.2).
4. **Missing/malformed RDS file behavior:** does Envoy hard-exit on a missing path (the CDS/LDS L4 bootstrap-failure mirror)? Warn-and-serve on a parse error (ticking `update_failure`)? On a semantic error (`update_rejected`)? This locks envoy-rust's negative-path disposition — projected: envoy-rust mirrors its phase-18/19 all-fatal posture (ADR-0049 decision 2 / ADR-0050), with the divergence recorded.
5. **The `/config_dump` shape with a dynamic route config:** the exact `RoutesConfigDump` JSON (the `dynamic_route_configs[].route_config` nesting? `version_info`? `last_updated`? `name`?); the entry ORDERING within `configs[]` (where does Routes land relative to Bootstrap/Clusters/Listeners? — fixture 0026's `configs[1]` + fixture 0027's `configs[2]` index assertions depend on Routes NOT displacing them); whether the entry appears when no HCM uses `rds`.
6. **`route_config_name` mismatch:** an RDS file that does NOT contain a RouteConfiguration named by the HCM's `route_config_name` — does Envoy hard-exit, warn-and-serve (ticking which counter), or 404 at runtime? (Locks the `RdsRouteConfigNotFound` disposition.)
7. **The RDS+CDS composition + `validate_clusters`:** an RDS-supplied route routing to a CDS-supplied cluster — does it work at initial load? Does the RouteConfiguration inside the RDS file require `validate_clusters: false` (the ADR-0049 L12 finding's context was a STATIC inline route_config; the LDS-supplied context did NOT per ADR-0050 L6 — the RDS-supplied context may differ again)? Does the `node.id`+`node.cluster` requirement apply identically?
8. **Route-through-RDS wire shape + RouteConfiguration field tolerance:** a GET through an RDS-supplied route — identical to the inline-route shape (200 + echo body + `x-envoy-upstream-service-time` + the standard header allow-list)? Any new response header or access-log flag? AND: does Envoy's RouteConfiguration in the RDS file require any field envoy-rust's parser rejects (deciding the D6 shared-vs-per-side template question — the phase-19 LDS per-side lesson)?
9. **Both/neither route source:** an HCM with BOTH `route_config` and `rds`, and an HCM with NEITHER — what does Envoy do (PGV reject at config-load? which message?). Locks the §5.8 exactly-one-of disposition.
10. **Stat conditionality cross-check:** does an inline-`route_config` HCM emit ANY `http.<prefix>.rds.*` names (the §5.2 inertness witness — projected NO; the fixture-0011/0026/0027 topologies confirm).
11. **(Opportunistic) `version_info` / `config_reload_time_ms`:** does the initial RDS load carry a version string in stats/config_dump? (Informs whether the BEHAVIOR_CONTRACT records a version-presence divergence; no deliverable depends on it.)

If item 1, 4, or 5 diverges materially from the projections → land **ADR-0052** at the PLAN-write commit (mirrors ADR-0037/0041/0043/0045/0047/0049).

### 6.3 In-process backstop assertions (heeds the 14.2→19 both-paths lesson)

The backstop covers BOTH the happy path (valid RDS + CDS files → both probes 200 + stats + config_dump) AND the negative paths (missing RDS file; malformed RDS file; `route_config_name` matching no resource; an RDS route to a cluster in neither list; both route sources; neither route source) per the §6.2-verified dispositions — the paths the differential fixture cannot exercise.

### 6.4 The 06.x stats convention + the inert-when-unconfigured discipline

Stat handles are `Arc<Counter>`/`Arc<Gauge>` registered once at construction; increments at single sites. Conditional registration per §5.2 — the phase-18 template at `cluster.rs:1060-1097` is the technique; the per-HCM keying (`stat_prefix` + `route_config_name`) is the new wrinkle. The increment-at-load-time handle threading (D4) is the one non-obvious wiring decision.

### 6.5 Pre-state-4 fmt + clippy discipline (heeds `project_state3_arc_skips_clippy`)

`cargo clippy --workspace --all-targets --all-features -- -D warnings` runs PER TASK in the state-3 arc. The D1 `Option` migration (`needless_borrow`/`single_match` on the unwrap sites), the D3 HCM walk (iterator-lint candidates), and the D5 enum extension (`collapsible_if`) are the likely lint sites.

### 6.6 State-4 evidence-discipline (continues per 05.3 → … → 19 chain)

Per-gate command outputs quoted into PROGRESS Task-N; a single Docker-gated CI run as the anchor. The phase-18 lesson (carryforward disposition 2): the CI-evidence check is load-bearing. Pre-build `tests/helpers/*` before `cargo test --workspace` (the cold-helper-compile flake class per `project_flaky_access_log_fixture_0012` — extends to any backstop that `cargo run`s a helper, incl. 0028's).

### 6.7 Isolated-crate build discipline (heeds `project_isolated_crate_build_blindspot`)

The state-4 verification MUST run `cargo build -p envoy-config -p envoy-cluster -p envoy-http1 -p envoy-http2` standalone in addition to the workspace build (the `route_config`→`Option` change ripples through envoy-http1/envoy-http2).

### 6.8 Cargo.lock cadence

No new top-level deps projected → no Cargo.lock churn beyond version bumps already in flight.

### 6.9 PLAN.md + PROGRESS.md skeleton + Task 1 preamble land alongside at state-2

The 06.2→19 standalone-PLAN cadence: one pre-Task-1 docs-only commit (PLAN + PROGRESS skeleton + Task 1 preamble + ROADMAP flip + STATE advance + any §6.2 ADR).

### 6.10 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The state-3 arc dispatches PLAN tasks to fresh subagents SERIALLY (`feedback_serial_subagent_dispatch`), each with two-stage review (spec-compliance THEN code-quality), TDD per task, one code commit + one PROGRESS commit per task.

### 6.11 The `xds_file.rs` generalization opportunity (phase-19 REVIEW M19-1)

RDS makes the `@type`-tagged envelope parser a THIRD copy-paste sibling (`cds.rs`, `lds.rs`, + RDS). The phase-19 REVIEW flagged M19-1: "a generic `parse_xds_file<T>` would dedupe; pays off at a 3rd resource type." That threshold is NOW. The PLAN-writer should weigh consolidating `cds.rs`+`lds.rs`+RDS into a resource-type-parametric `xds_file.rs` (D2) against the LoC budget — the generalization is a net simplification and is forward-useful for the gRPC/ADS phase's resource dispatch. If consolidation lands, it is in-scope refactoring (the brainstorming-skill "improve code you're working in" discipline), recorded in the PLAN.

---

## 7. Conditional ADRs (projected; land at PLAN-write or in-execution if they fire)

- **ADR-0051 (the scoping ADR) — LANDS AT THIS BRAINSTORM COMMIT.** The continuation pick + the §0 findings + the minimum-viable scope boundary + the deferral ledger. (The ADR-0042/0044/0046/0048/0050 brainstorm-time cadence.)
- **ADR-0052 (§6.2 empirical-verification reconciliation) — PLAUSIBLE.** Fires if §6.2 item 1 (the RDS file envelope / `rds`-on-HCM shape), item 4 (the missing/malformed-RDS-file or `route_config_name`-mismatch disposition — note Envoy's RDS negative-path split may differ from its CDS/LDS split), or item 5 (the `RoutesConfigDump` shape / `configs[]` ordering — a fixture-0026/0027 compatibility trigger) diverges materially from the projections. Lands at the state-2 PLAN-write commit. Mirrors ADR-0037/0041/0043/0045/0047/0049.
- **ADR-0053 (phase split) — POSSIBLE (projected NOT to fire, with less margin than phase 19).** Fires only if the §6.2-refined estimate exceeds ~1500 LoC / ~25 tasks. Seam per §1 NOTE / §6.1. Mirrors ADR-0036/0038/0040.

---

## 8. Summary

Phase 20 continues the **xDS / dynamic config family** at its next increment: **file-based RDS**. An HCM pointing `rds.config_source.path_config_source` at a YAML file (instead of carrying an inline `route_config`) gets its route table loaded at startup, routing traffic, and observable via the per-HCM `http.<stat_prefix>.rds.<route_config_name>.*` stats and `/config_dump`'s `RoutesConfigDump` — bilaterally verified by fixture `0028-xds-file-based-rds`, which composes the RDS-supplied route with a phase-18 CDS-supplied cluster (exercising the cluster-before-route-revalidation merge ordering). With phases 18 (CDS) + 19 (LDS) done, RDS completes Envoy's core data-plane dynamic-resource triad over the filesystem transport, and introduces the named/scoped config-source idiom (`route_config_name`) that EDS and SDS reuse — all with zero concurrency, zero timing sensitivity, zero new crates, and zero new dependencies. The three genuinely-new surfaces (the HCM `route_config` XOR `rds` schema surgery, the per-HCM stat topology, and the effective-`route_config` threading) keep it modestly heavier than the LDS phase but comfortably single-phase; the hard xDS surfaces (file watching — whose ROI this phase strictly improves, scoped_routes/SRDS/VHDS, EDS/SDS/RTDS, the gRPC/ADS state machine, delta) remain cleanly deferred with named owners.
