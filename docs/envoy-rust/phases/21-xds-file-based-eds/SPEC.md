# Phase 21 (`21-xds-file-based-eds`) — SPEC

- **Phase id:** `21`
- **Slug:** `21-xds-file-based-eds`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `2495f0451`, the phase-20 state-6 deterministic close-out commit; the "xDS / dynamic config family" §9 table carries exactly THREE rows — `18`, `19`, `20`, all `done`). **This SPEC's landing commit adds the FOURTH concrete row beneath the "xDS / dynamic config family" heading**, with `status: planned` — the family's third continuation phase, extending the filesystem-transport surface from the CDS+LDS+RDS data-plane triad down to the **endpoint** layer (EDS).
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"xDS / dynamic config family — ADS, delta xDS, LDS, CDS, RDS, EDS, SDS, RTDS, reconnection, initial-fetch timeout."* This phase lands the family's **filesystem-transport EDS member**: a `Cluster.eds_cluster_config.eds_config.path_config_source` — a cluster's endpoints (a `ClusterLoadAssignment`) loaded from a local file at startup, observable via the per-cluster `cluster.<name>.{update_attempt,update_success,…}` EDS-subscription stats, the `EndpointsConfigDump` admin section, and (most importantly) the data plane: a cluster whose endpoint list exists ONLY in the EDS file serves traffic bilaterally. With phases 18 (CDS) + 19 (LDS) + 20 (RDS) already done, this phase extends **Envoy's filesystem-dynamic-config surface down to the endpoint layer** — clusters, their endpoints, listeners, and routes all sourced from files — bilaterally proven by fixture 0029.
- **Position in the project:** the **thirteenth post-MVP-trunk feature-family phase** and the **fourth concrete xDS-family phase**. The MVP trunk 00→08, the three HTTP-filter-family phases (09/10/11), the six Upstream-robustness-family phases (12/13/14/15/16/17), and the three xDS-family phases (18 CDS, 19 LDS, 20 RDS) all stand `done`. The **28-Docker-gated-fixture regression baseline** established at phase-20 close (`0001-tcp-echo` through `0028-xds-file-based-rds`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **depends-on:** `01 02 04 06 08 18` — phase `01` (the `envoy-config` bootstrap loader the `eds_cluster_config` field extends), phase `02` (the listener + cluster + round-robin-LB runtime whose endpoint list this phase makes dynamically loadable), phase `04` (the HCM + router whose data-plane probe exercises the EDS-supplied endpoint), phase `06` (the `envoy-stats` foundation the per-cluster `cluster.<name>.*` EDS stats register against), phase `08` (the admin `/config_dump` endpoint + `ConfigDumpEntry` enum the `EndpointsConfigDump` section extends), and phase `18` (the `dynamic_resources`/`ConfigSource`/`PathConfigSource` schema, the `cds.rs`/`lds.rs`/`rds.rs` `@type`-tagged envelope-parser pattern, `load_dynamic_resources`, and the harness dynamic-file rendering/mounting machinery). **Phase `20` (RDS) is a reuse source but NOT a hard dependency** — EDS anchors on a STATIC cluster (the minimum-viable fixture), exactly as fixture 0026 anchored CDS on a static listener and fixture 0028 anchored RDS on a static listener; the EDS-cluster-supplied-by-CDS composition is a deferral (§4).
- **Brainstorm narrative:** see the "Phase-21 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the continuation-pick rationale and the alternatives weighed (CDS/LDS/RDS file watching/hot reload [the ledger's nominal prime follow-up — rejected again on the three stacked ADR-0050 risks: the `ClusterManager`/listener-manager/HCM-route-table mutability refactor, the watch-convergence timing sensitivity, and the macOS-Docker-Desktop §6.2-verification blocker recorded in ADR-0049's Provenance — its ROI strictly improves by landing EDS first, so ONE watching phase lights up CDS+LDS+RDS+EDS]; file-based EDS [chosen — the runner-up at phases 19+20, now picked: it completes the file-transport surface down to the endpoint layer, the last static file-based resource type before the watching capstone]; the gRPC family [still blocked on H2 trailers — re-verified at HEAD `2495f0451`: `crates/envoy-http1/src/client.rs` discards trailers, `envoy-http2` exposes no trailer API]; the Load-balancing / Observability / HTTP-3+QUIC / WASM-host / Network-filters / Runtime families [the phase-18 ADR-0048 rejection analysis carries unchanged]). The scoping decision is ratified in **ADR-0053** (landed at this brainstorm commit).

---

## 0. Critical scoping findings (READ FIRST) — EDS reuses the filesystem-transport machinery but introduces the first CLUSTER-scoped dynamic resource

Phases 18 + 19 + 20 built the filesystem xDS transport for three resource types: clusters (BOOTSTRAP-scoped, under `dynamic_resources.cds_config`), listeners (BOOTSTRAP-scoped, under `dynamic_resources.lds_config`), and routes (HCM-scoped, under `rds` on the HCM). The state-1 brainstorm identified four findings that make file-based EDS a **single, bounded phase** — reusing the proven envelope/merge/harness machinery, but with genuinely-new surfaces of comparable weight to the RDS phase:

1. **The config-source machinery and the envelope parser are reused; the new schema shape is `ClusterType::EDS` + `eds_cluster_config` ON THE CLUSTER + making the REQUIRED `Cluster.load_assignment` field optional — the FIRST CLUSTER-scoped dynamic resource.** The `ConfigSource`/`PathConfigSource` structs (`crates/envoy-config/src/bootstrap.rs:107-123`) are resource-type-agnostic and reused verbatim — `EdsClusterConfig` embeds an `eds_config: ConfigSource`. The `@type`-tagged envelope parser (`crates/envoy-config/src/cds.rs:41-46` + `lds.rs:35-40` + `rds.rs:34-38`) generalizes to a ClusterLoadAssignment-resource variant (`@type: type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment`) — the per-resource payload is exactly the `LoadAssignment` struct `envoy-config` already parses for the inline `load_assignment` (`bootstrap.rs:262-328`: `cluster_name` + `endpoints` of `LocalityLbEndpoints`→`LbEndpoint`→`Endpoint`→`Address`). **The novelty: EDS is configured ON THE CLUSTER (`type: EDS` + `eds_cluster_config:` replacing the inline `load_assignment:`), NOT under bootstrap-level `dynamic_resources` and NOT on the HCM.** `Cluster.load_assignment` is a REQUIRED non-`Option` field today (`bootstrap.rs:181`, survey-confirmed); `ClusterType` carries exactly two variants today (`Static`, `StrictDns` — `bootstrap.rs:225-239`). Phase 21 adds `ClusterType::Eds`, makes `load_assignment` an `Option`, and adds a sibling `eds_cluster_config: Option<EdsClusterConfig>`, validated exactly-one-of-and-consistent-with-`cluster_type`. This is the **cluster-scoped** config-source topology (the SDS `sds_config`-on-transport-socket pattern is its closest sibling).

2. **The merge-into-effective-`load_assignment` design needs NO cluster runtime refactor.** Rather than threading an EDS-aware branch through `ClusterManager::from_bootstrap`'s endpoint-build path, `load_assignment` becomes `Option<LoadAssignment>` and the EDS file is loaded at config-load time: for each EDS cluster, read the file, select the `ClusterLoadAssignment` matching `service_name`-or-cluster-name, and POPULATE the effective `load_assignment = Some(loaded)`. Downstream endpoint construction (`crates/envoy-cluster/src/cluster.rs:734-797` — the STATIC `SocketAddr::from_str` / STRICT_DNS `lookup_host` dispatch over `cfg.load_assignment.endpoints`) reads a populated `load_assignment` exactly as today — the only consumer change is an Option-unwrap guarded by the post-load invariant (every cluster has a resolved `load_assignment` after load, whether inline or EDS-supplied). This mirrors the phase-18/19/20 "merge dynamic into the effective config at config-load time, downstream sees a uniform shape" design (ADR-0048 finding 2 / ADR-0050 finding 2 / ADR-0051 finding 2). **No runtime endpoint mutability, no locks, no watch tasks, no cluster warming machinery** — the EDS file is read once, synchronously, at startup.

3. **EDS stats are PER-CLUSTER-scoped (`cluster.<name>.{update_attempt,update_success,…}`) — extending an EXISTING per-cluster namespace, distinct from the manager-level CDS/LDS families and the per-HCM RDS family.** The phase-18 `cluster_manager.cds.*` and phase-19 `listener_manager.lds.*` families are process-level singletons; the phase-20 `http.<stat_prefix>.rds.<route_config_name>.*` family is per-HCM. EDS's subscription stats embed the cluster name (`cluster.<name>.update_*`) and register at the site that already builds the `cluster.<name>.*` namespace and already owns `membership_healthy` (`crates/envoy-cluster/src/cluster.rs:817-884`, from phase 12.1). This is genuinely-new in that the `update_*` warming-subscription family on a cluster is new, but it extends an EXISTING per-cluster registration site rather than inventing a topology — making EDS's stat surface modestly LIGHTER than RDS's (which had to build a brand-new per-HCM keying). The conditional-registration TECHNIQUE is reused (the phase-18 template at `cluster.rs:1060-1097`), with the predicate being per-cluster (`cluster_type == Eds`).

4. **Cluster warming is the EDS-specific disposition question, and there is no pre-existing allow-listed Envoy-side EDS surface.** Phases 18/19/20 inherited a head start from fixture 0011's allow-list of `cluster_manager.*` / `listener_manager.*` names; the per-cluster `cluster.<name>.update_*` EDS family is NOT among them (the existing fixtures' clusters are STATIC, so Envoy emits no EDS-subscription stats there), so the EDS fixture (0029) drives its bilateral assertions from scratch (the §6.2 stat-name verification is load-bearing). **The genuinely-new semantic: an EDS cluster with no assignment yet "warms" in Envoy** (it is held out of the active set until its first `ClusterLoadAssignment` arrives, or `initial_fetch_timeout` fires). For file-based EDS at initial load the file is read synchronously, so the cluster warms immediately — BUT the **missing-file / missing-resource disposition** (does Envoy keep the cluster warming and serve `no healthy upstream`/503 for routes to it, or does it fatal-exit?) is the load-bearing §6.2 question (the EDS analogue of the RDS `route_config_name`-mismatch question; §6.2 items 2 + 4). envoy-rust's projected posture: the phase-18/19/20 all-fatal posture (ADR-0049 decision 2), with the divergence recorded. **In exchange, EDS extends the filesystem-dynamic-config surface to the endpoint layer** AND reuses the named/scoped config-source idiom RDS introduced (`route_config_name` → here `service_name` selecting a `ClusterLoadAssignment` by name) — making EDS the architecturally-coherent next step.

**Consequence:** phase 21 needs **NO new crate, NO new top-level Cargo dep, NO new harness driver, NO new helper binary, and NO concurrency/timing machinery** — the EDS file load is synchronous at startup (the phase-18/19/20 `std::fs` posture), so the fixture is deterministic and timing-robust (readiness implies loaded). Projected surface is **comparable to phase 20** (~1100–1450 LoC) because the `ClusterType::Eds` + `load_assignment`→`Option` schema surgery, the per-cluster EDS `update_*` stat family, and the effective-`load_assignment` threading are each a first build (offset by the per-cluster stat site already existing) — comfortably a single un-split phase under the §6.1 gate. **CRITICAL ENVIRONMENT NOTE:** unlike the deferred file-WATCHING phase, file-based EDS at initial load has **NO virtiofs/inotify dependency** — the §6.2 empirical verification runs LOCALLY on macOS Docker (the phase-18/19/20 methodology), since the EDS file is read once at startup, not watched.

These findings are ratified in **ADR-0053** (landed at this brainstorm commit).

---

## 1. Goal and acceptance signal

Phase 21 makes **file-based dynamic endpoint discovery (EDS over the filesystem transport) work end-to-end**. When a cluster sets `type: EDS` + `eds_cluster_config.eds_config.path_config_source.path` instead of an inline `load_assignment`, both upstream Envoy and envoy-rust:

- **load the `ClusterLoadAssignment` named by `service_name`-or-cluster-name from that file at startup** (initial load; before serving traffic),
- **route data-plane traffic to its endpoints** exactly as if they had been defined inline (the full LB-pick + connection-pool + upstream machinery applies),
- **expose the load observably**: the per-cluster `cluster.<name>.{update_attempt,update_success,…}` EDS-subscription stat subset (§6.2-verified) and the `/config_dump` `EndpointsConfigDump` section listing the dynamically-loaded endpoint assignment.

**Differential surface added by phase 21:**

- **Fixture `0029-xds-file-based-eds`** — a cluster whose endpoint list is EDS-supplied, bilaterally asserted. Both proxies receive identical bootstraps whose `static_resources` carries **one static listener (HTTP/1.1 HCM, `stat_prefix: ingress_http`, inline `route_config` with one route `/` → cluster `eds_backend`), one static cluster `eds_backend` with `type: EDS` + `eds_cluster_config: { eds_config: { path_config_source: { path: <EDS_PATH> } } }` + NO inline `load_assignment`**. The EDS file defines one `ClusterLoadAssignment` (`eds_backend`) with one endpoint → the `http1-echo-server` backend. Probes (all via the existing `Driver::Http1KeepAlive`):
  1. **Data plane, EDS isolation (the load-bearing probe):** `GET /` → **200** + the `http1-echo-server` echo body **byte-exact** bilaterally + `x-envoy-upstream-service-time` present, routed to the **EDS-supplied endpoint**. Without the EDS load, the cluster has NO endpoints — the probe discriminates loaded-from-not-loaded (a proxy that ignored `eds_cluster_config` would either fail startup or serve `no healthy upstream`/503).
  2. **Stats:** `cluster.eds_backend.update_success: 1` + `cluster.eds_backend.update_failure: 0` (+ the §6.2-verified subset, incl. `…update_attempt` and the membership discriminators `cluster.eds_backend.membership_healthy` / `…membership_total`) + `cluster.eds_backend.upstream_rq_total: 1`, asserted via the named-stat scrape.
  3. **Admin scrape:** `/config_dump` `EndpointsConfigDump` entry naming `eds_backend` (the §6.2-item-5-verified JSON shape + `configs[]` index), alongside the existing `ClustersConfigDump`/`ListenersConfigDump`/`RoutesConfigDump` assertion shapes (which this fixture does NOT configure → must remain absent/stable).

  The discriminating differential observables are the **endpoints-from-file data-plane success + the EDS update counters** — a proxy that ignored `eds_cluster_config` would have an empty endpoint list (503/`no healthy upstream`/startup failure) and emit no `cluster.<name>.update_*` stats. Probe shape, exact stat names, the warming/missing-resource disposition, and the config_dump assertion are §6.2-verified projections (§6.2 items 1–5).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0029-xds-file-based-eds` green at Docker-gated CI.
- **(b)** All **28 pre-existing differential fixtures** (`0001` through `0028`) **remain green simultaneously** at the same CI run (regression-equivalence per §7.5 (b)). The EDS machinery is inert when no cluster sets `type: EDS`: making `load_assignment` optional keeps every existing inline-`load_assignment` fixture parsing identically (deserializes to `Some`); the new `cluster.<name>.update_*` EDS stats register ONLY for an EDS cluster (the phase-15/17/18/19/20 conditional-registration discipline); the `EndpointsConfigDump` entry is emitted ONLY when some cluster is EDS (fixtures 0014 + 0026 + 0027 + 0028 untouched); fixture 0011's Prometheus set-diff sees zero new envoy-rust names.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline). Phase 21 does not touch HTTP framing.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (new seed `cluster_eds.yaml`; git-tracked curated corpus 31 → 32; the new-seed atomic edit: fuzz `.gitignore` allow-list + the SUCCESS-array together — the 09→20 lesson; the corpus is fully consistent entering this phase per the phase-20 carryforward-disposition-1 closure).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` (run PER TASK in the state-3 arc, per `project_state3_arc_skips_clippy`), `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean — plus the 4 standalone-crate builds (`-p envoy-config` / `-p envoy-cluster` / `-p envoy-http1` / `-p envoy-http2`) per `project_isolated_crate_build_blindspot`.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent).

> **NOTE — single phase projected (see §6.1).** Phase 21's surface (the `ClusterType::Eds` + `eds_cluster_config` schema + the `load_assignment`→`Option` migration + EDS-file parsing + the effective-`load_assignment` merge + ordering + the per-cluster EDS `update_*` stats + the `EndpointsConfigDump` section + the harness `{{EDS_PATH}}` generalization + fixture 0029 + in-process backstop + fuzz seed + BEHAVIOR_CONTRACT rows) is projected at **~1100–1450 LoC / ~11–13 tasks** — under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task split gate, comparable margin to phase 20. The recommended split seam if the §6.2-refined estimate fires the gate anyway: **`21.1`** (the `ClusterType::Eds` + `eds_cluster_config` schema + the `load_assignment`→`Option` migration + EDS-file parsing + the effective-`load_assignment` merge + ordering + in-process backstop + fuzz seed — the foundation slice, regression-equivalence acceptance) / **`21.2`** (the per-cluster `update_*` stats + `EndpointsConfigDump` + harness extension + fixture 0029 + parent-21 close). The split ADR would be ADR-0055 (§7).

---

## 2. Behavior-contract scope for phase 21

Phase 21 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x→20 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — EDS / per-cluster subset (projected; §6.2-verified)

New rows, mirroring upstream Envoy v1.33's documented per-cluster EDS-subscription stat tree. **Minimum-viable subset** (the 14.1/15/16/17/18/19/20 namespace-subset precedent): emit the names Envoy emits for the behavior envoy-rust implements; allow-list the rest. **Note the per-cluster scoping** — every name is prefixed `cluster.<name>.` (fixture 0029: `cluster.eds_backend.`); these are SIBLINGS of the existing `cluster.<name>.membership_healthy`/`upstream_rq_total` names.

| Stat name (relative to `cluster.<name>.`) | Kind | Equivalence (projected; §6.2-verified) | Rationale |
|---|---|---|---|
| `update_attempt` | counter | value-exact | +1 per EDS update attempt. At initial-load-only scope, exactly `1` after startup. §6.2 item 3 verifies (the phase-18/19/20 `update_attempt` precedent). |
| `update_success` | counter | value-exact | +1 per successful EDS update. Fixture 0029: `1`. |
| `update_failure` | counter | value-exact (0-case) | +1 per failed EDS update. Fixture 0029 asserts `0`. Structurally unreachable non-zero in envoy-rust if the all-fatal posture mirrors CDS/LDS/RDS (ADR-0049 decision 2; §6.2 item 4 verifies Envoy's EDS negative-path split). |
| `update_empty` | counter | value-exact (0-case; presence §6.2-verified) | +1 per EDS update that resolved to zero endpoints. Fixture 0029 asserts `0` (one endpoint). §6.2 item 3 confirms presence; if Envoy does not emit it deterministically at this scope it drops from the subset. |
| `membership_healthy` / `membership_total` | gauge | value-exact | the existing phase-12.1 per-cluster membership gauges, now driven by an EDS-supplied endpoint set. Fixture 0029: `1` / `1`. |

(The exact subset — whether `update_rejected`, `version_info`, `assignment_timeout_received`, or the warming-state gauges belong — is locked at the §6.2 verification; the table is the projection.)

**Conditional registration (the §5.2 invariant):** the EDS `update_*` names register ONLY for clusters whose `cluster_type == Eds` (the per-cluster predicate per §0 finding 3). This is a deliberate, BEHAVIOR_CONTRACT-recorded narrowing vs Envoy (which emits the per-cluster EDS family for every EDS cluster): the STATIC/STRICT_DNS clusters emit no `update_*` names. All 28 existing fixtures (whose clusters are STATIC/STRICT_DNS) see zero new envoy-rust names, preserving the regression baseline with zero edits.

### 2.2 "xDS wire state machine" section — EDS extension of the filesystem-transport subsection

The BEHAVIOR_CONTRACT's "Filesystem transport (`path_config_source`)" subsection (first populated at phase 18, extended for LDS at 19 and RDS at 20) gains EDS rows: (a) the EDS file envelope shape Envoy accepts (§6.2 item 1 — projected to mirror the CDS/LDS/RDS envelope with `@type: type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment`), (b) the `eds_cluster_config`-on-cluster config shape (`eds_config` + optional `service_name`; §6.2 item 1b), (c) the initial-load/readiness + cluster-warming ordering for endpoints (§6.2 item 2), (d) the missing/malformed-EDS-file + missing-`ClusterLoadAssignment` disposition + whether envoy-rust's all-fatal posture diverges from Envoy's warming posture (§6.2 item 4; the ADR-0049 decision-2 recorded-divergence pattern), and (e) the `service_name`-vs-cluster-name selection semantics (§6.2 item 8). The `EndpointsConfigDump` shape lands in the "Admin endpoint body shapes" section as a new row (§6.2 item 5 supplies the JSON shape + the `configs[]` index).

### 2.3 DECISIONS.md amendment at SPEC time — ADR-0053 (the scoping ADR)

Like phases 15 (ADR-0042), 16 (ADR-0044), 17 (ADR-0046), 18 (ADR-0048), 19 (ADR-0050), and 20 (ADR-0051), phase 21's brainstorm DOES land an ADR: **ADR-0053** records (a) the **continuation pick** (file-based EDS over CDS/LDS/RDS file watching [three stacked risks, ROI improved by deferral], the gRPC family [still blocked on H2 trailers — re-verified at HEAD], and the other §9 families [the phase-18 rejection analysis carries]) with the alternatives weighed, (b) the four §0 findings, and (c) the minimum-viable scope boundary — deliver file-based EDS initial load + the per-cluster `cluster.<name>.update_*` stat subset + `EndpointsConfigDump` + fixture 0029; defer file watching, multi-endpoint/locality-weighted/priority EDS, endpoint health-status-in-EDS, the EDS-cluster-supplied-by-CDS composition, SDS/RTDS, scoped_routes/SRDS, the gRPC/ADS transport, delta xDS, and the ADR-0014 protos supersession. Conditional §6.2-reconciliation + split ADRs are enumerated in §7.

---

## 3. Deliverables

Phase 21's scope is enumerated as deliverables `D1`–`D8` below. **The state-2 PLAN-writer organizes deliverables into tasks AND evaluates the §6.1 split gate** (projected NOT to fire, comparable margin to phase 20). Deliverables are LISTED roughly in execution order; the SPEC constrains the surface, not the task organization.

### D1 — `envoy-config` schema extension (`ClusterType::Eds` + `eds_cluster_config` on the cluster; `load_assignment` → `Option`)

`crates/envoy-config/src/bootstrap.rs` changes:

```rust
// ClusterType gains a third variant (was: Static, StrictDns)
pub enum ClusterType { Static, StrictDns, Eds }

// Cluster.load_assignment becomes optional (was: pub load_assignment: LoadAssignment)
#[serde(default, skip_serializing_if = "Option::is_none")]
pub load_assignment: Option<LoadAssignment>,
// new sibling — present iff cluster_type == Eds
#[serde(default, skip_serializing_if = "Option::is_none")]
pub eds_cluster_config: Option<EdsClusterConfig>,
```

with a new `EdsClusterConfig` struct:

```rust
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EdsClusterConfig {
    pub eds_config: ConfigSource,            // reused verbatim from phase 18
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,        // selects the ClusterLoadAssignment by name; defaults to cluster name (§6.2 item 8)
}
```

The validator gains an **exactly-one-of-and-consistent** check: a `type: EDS` cluster MUST carry `eds_cluster_config` and MUST NOT carry `load_assignment`; a `type: STATIC`/`STRICT_DNS` cluster MUST carry `load_assignment` and MUST NOT carry `eds_cluster_config` (projected variants `MissingEndpointSource` / `EdsConfigOnNonEdsCluster` / `LoadAssignmentOnEdsCluster` / `MissingEdsClusterConfig`; the exact set is a PLAN-write decision informed by §6.2 item 6). `ConfigSource`'s `api_config_source`/`ads`/`watched_directory` remain rejected by `deny_unknown_fields` (deferred per §4). New `ConfigError` variants for the EDS file/parse/selection failures: projected ~2-3 (`EdsFileError { path, source }`, `EdsParseError`, `EdsClusterLoadAssignmentNotFound { name }` — mirroring the phase-18/19/20 file/parse pair plus the named-resource-not-found case; the exact set is a PLAN-write decision informed by §6.2 item 4).

> **CARRY-FORWARD WARNING for the state-3 executor (D1):** making `load_assignment` an `Option` is a workspace-compile-affecting change. YAML fixtures with inline `load_assignment:` still parse (→ `Some`), but every Rust site that READS `cfg.load_assignment` (the endpoint-build loop at `crates/envoy-cluster/src/cluster.rs:734-797`; the `LoadAssignmentNameMismatch` validator at `lib.rs:131-133`; the admin config_dump cluster serializer) and every Rust test that CONSTRUCTS `Cluster { load_assignment: la, .. }` literally must adapt in the same commit. The phase-18/19/20 `Bootstrap`/`RouteConfiguration`/`HttpConnectionManagerConfig` struct-literal sweep is the precedent (phase 20 hit 11 `HttpConnectionManagerConfig` literal sites + production reads). The PLAN-write's SPEC-correction pass enumerates the exact literal-construction + field-read sites (the `Cluster` struct-literal sweep across `envoy-config`/`envoy-cluster` tests is expected to be the largest).

### D2 — EDS file parsing (`envoy-config`)

Either a new `crates/envoy-config/src/eds.rs` module (the `cds.rs`/`lds.rs`/`rds.rs` sibling-module shape) or — the consolidation pressure is now at its STRONGEST (this is the FOURTH `@type`-tagged copy-paste sibling) — a generalization of `cds.rs`+`lds.rs`+`rds.rs`+EDS into a resource-type-parametric `xds_file.rs` (the M19-1 / M20-T6-a item, DEFERRED at phase 20 by deliberate decision per its carryforward disposition 4 — the PLAN-writer re-weighs it now): `parse_eds_file(path, contents) -> Result<Vec<ClusterLoadAssignment>, ConfigError>` parsing the §6.2-item-1-verified envelope shape (projected: the phase-18/19/20 envelope with `@type: type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment` per resource; both the bare `resources:` list and the full `DiscoveryResponse` shape accepted; always-YAML parsing per the ADR-0049 decision-1 posture). The per-resource payload is the existing `LoadAssignment` struct (`cluster_name` + `endpoints`), reused verbatim. The named-resource selection (`service_name`-or-cluster-name → the matching `ClusterLoadAssignment`) happens at merge time (D3), not in the parser.

### D3 — Effective-`load_assignment` merge + ordering (config-load-time; the §5.4 ownership boundary)

`load_dynamic_resources` (`crates/envoy-config/src/lib.rs:623-764`) gains the EDS pass: **walk every cluster across the merged (static + CDS-dynamic) cluster set**; for each cluster whose `cluster_type == Eds`, read its EDS file → select the `ClusterLoadAssignment` whose `cluster_name == eds_cluster_config.service_name.unwrap_or(cluster.name)` (`EdsClusterLoadAssignmentNotFound` if absent) → **populate that cluster's effective `load_assignment = Some(selected)`**. **Ordering invariant (§5.7): the CDS merge completes BEFORE the EDS pass runs**, so a cluster supplied by CDS that is ALSO `type: EDS` gets its endpoints loaded too (the composition is a deferral per §4, but the walk covers `all_clusters()` so it is composition-ready). The EDS pass runs BEFORE `ClusterManager::from_bootstrap` builds endpoints (`main.rs` call site), since the builder reads `load_assignment`. The post-merge re-validation (`bootstrap::validate`) runs ONCE against the full effective state, after the CDS + LDS + RDS + EDS merges. **Consumer-migration sweep — the `load_assignment` field-read + construction sites** (the brainstorm survey + the PLAN-write's SPEC-correction pass confirm the exact set): the endpoint-build loop (`crates/envoy-cluster/src/cluster.rs:734-797`) unwraps the now-`Option` field under the post-load "every cluster has a resolved load_assignment" invariant (§5.3); the `LoadAssignmentNameMismatch` validator and the admin config_dump cluster serializer adapt.

### D4 — Per-cluster EDS `update_*` stats (conditional registration; extends the existing per-cluster namespace)

The §2.1 stat subset: the `update_{attempt,success,failure,empty}` counters (+ the existing `membership_*` gauges, now EDS-driven), registered ONLY for clusters whose `cluster_type == Eds` (the §5.2 invariant), keyed on `cluster.<name>.`. **Registration site:** the existing per-cluster stat registration (`crates/envoy-cluster/src/cluster.rs:817-884`) — which already builds the `cluster.<name>.*` namespace and owns `membership_healthy` — gains a conditional `update_*` sub-registration when `cluster_type == Eds`. The conditional-registration TECHNIQUE follows the phase-18 template (`cluster.rs:1060-1097`), with the per-cluster predicate. **The increments fire inside `load_dynamic_resources` at load time** (update_attempt/update_success), so the stat handles must be threaded from the registration site to the load site — a PLAN-write threading decision (candidates: register-then-increment-at-load with the registry passed to the loader, or a deferred-increment recorded on the merged config and replayed at cluster construction — the phase-20 D4 threading precedent). No H1/H2-protocol-side change (EDS stats are cluster-side only, unlike RDS's HCM-side registration).

### D5 — `/config_dump` `EndpointsConfigDump` section (conditional emission)

`crates/envoy-admin/src/endpoint.rs` `ConfigDumpEntry` enum (currently `Bootstrap` + `Clusters` + `Listeners` + `Routes` variants) gains an `Endpoints` variant rendering the §6.2-item-5-verified shape (projected: `{"@type": ".../EndpointsConfigDump", "static_endpoint_configs": [...], "dynamic_endpoint_configs": [{"endpoint_config": {"@type": ".../ClusterLoadAssignment", "cluster_name": "eds_backend", ...}}]}` — §6.2 item 5 captures the exact nesting + whether a `version_info`/`last_updated` key appears). Emitted ONLY when some cluster is EDS (fixtures 0014 + 0026 + 0027 + 0028 untouched). The entry-ordering within the `configs` array is a §6.2-item-5 capture — per ADR-0052 Envoy's verified order is Bootstrap[0]/Clusters[1]/Listeners[2]/ScopedRoutes[3]/Routes[4]/Secrets[5]; `EndpointsConfigDump`'s index is §6.2-verified (it must not displace the fixture-0026 `configs[1]` / fixture-0027 `configs[2]` / fixture-0028 `RoutesConfigDump` assertions — §5.5). The ADR-0052 per-side `JsonSubtreeRule` path-override harness mechanism is reused if the index diverges (→ ADR-0054).

### D6 — Harness EDS-file rendering + container mounting

`tests/differential/src/lib.rs` generalizes the phase-18/19/20 dynamic-file machinery (the `{{CDS_PATH}}`/`{{LDS_PATH}}`/`{{RDS_PATH}}` handling — the **M20-T6-a triplication** the phase-20 review flagged as the prime "extract a dynamic-file render helper" item; this phase makes it a QUADRUPLICATION, so the PLAN-writer should weigh extracting the helper here, the harness analogue of the D2 `xds_file.rs` consolidation) to a fourth file: when a fixture directory carries an EDS template, render it per-side with the same substitution maps, write to temp, mount the upstream rendition into the Envoy container (a path ending in `.yaml` per the ADR-0049 decision-1 constraint), and substitute `{{EDS_PATH}}` into each side's main config. **Whether the EDS template is shared or per-side** (`eds.yaml` vs `eds-envoy.yaml`/`eds-envoy-rust.yaml`) is a §6.2 finding (item 9): an EDS file carries only a `ClusterLoadAssignment` (cluster_name + endpoints — no filter/HCM config with Envoy-only fields), so it is projected SHAREABLE (like CDS/RDS, unlike the per-side LDS) — but §6.2 confirms whether Envoy's `ClusterLoadAssignment` requires any field envoy-rust rejects. **CRITICAL — the EDS file carries the BACKEND ENDPOINT marker** (the `socket_address` host:port of the upstream): the combined-source backend-detection + `uses_host_gateway` scans (the phase-18 carryforward-disposition-2 bug-class lesson: **scan ALL rendered sources** — fixture 0026's backend lived ONLY in the CDS file) MUST include the EDS rendition, because fixture 0029's backend endpoint lives ONLY in the EDS file. This is the load-bearing harness correctness point for this phase.

### D7 — Fixture 0029 + Docker wrapper

`tests/fixtures/0029-xds-file-based-eds/` carrying `envoy.yaml` + `envoy-rust.yaml` (admin + `node` + one static listener whose HCM has an inline `route_config` routing `/` → `eds_backend` + one static cluster `eds_backend` with `type: EDS` + `eds_cluster_config: { eds_config: { path_config_source: { path: {{EDS_PATH}} } } }` + NO inline `load_assignment`) + the EDS template (shared `eds.yaml` or per-side per §6.2 item 9: `ClusterLoadAssignment` `eds_backend`, one locality, one `lb_endpoint` → the host backend socket address) + `expectations.yaml` (the §1 probe list) + `README.md`. Docker-gated wrapper test at `tests/differential/tests/xds_file_based_eds.rs`. **Backend topology note:** the fixture reuses the existing `http1-echo-server` helper as the EDS-supplied backend (no new helper — the §0 consequence); the single endpoint keeps the differential deterministic (multi-endpoint distribution needs the deferred LB distinguishable-backend infra — §4).

### D8 — In-process backstop + fuzz seed + BEHAVIOR_CONTRACT extensions

In-process backstop at `crates/envoy-bin/tests/xds_file_based_eds.rs` (start envoy-rust with a temp EDS file; assert the data-plane 200 through the probe + the `cluster.<name>.update_*` stats + the config_dump entry; plus the negative paths: missing EDS file / malformed EDS file / no `ClusterLoadAssignment` matching the `service_name`-or-cluster-name / an EDS cluster with an inline `load_assignment` / a non-EDS cluster with `eds_cluster_config` / an EDS cluster with neither — per the §6.2-item-4/6-verified dispositions). **Reuse note:** the backstop helper block is copied from `crates/envoy-bin/tests/xds_file_based_rds.rs` — the M18-9 extract-a-test-support-crate item is now N≥5 (CDS, LDS, RDS, EDS backstops + the per-fixture `handler_from_bootstrap`/backend-cluster consts); record the duplication in the file header (the extraction stays a future hardening-phase task per the phase-20 carryforward disposition). Fuzz seed `cluster_eds.yaml` (git-tracked curated corpus 31 → 32). BEHAVIOR_CONTRACT: the §2.1 stat rows + the §2.2 xDS-section EDS extension + the `EndpointsConfigDump` admin-body-shapes row.

---

## 4. Out of scope (deferred non-goals)

Each deferred item below is rejected by `#[serde(deny_unknown_fields)]` today (a bootstrap configuring it fails parse loudly — nothing is silently under-implemented). This extends the xDS family's deferred-surface ledger:

- **File WATCHING / hot reload** (for the CDS, LDS, RDS, AND EDS files; inotify/poll; endpoint/cluster/listener/route add-update-remove at runtime; the mutability refactors it requires; cluster warming on re-subscription; `*.update_*` on re-load). **Owner: the family's prime follow-up phase — now with the BEST ROI yet** (one watching phase lights up hot reload for all FOUR file-based resource types CDS+LDS+RDS+EDS). NOTE (carried from ADR-0049 Provenance): that phase's §6.2 verification MUST run on Linux CI — macOS Docker Desktop's virtiofs/inotify limitation makes file-watch behavior unobservable locally. (This phase, file-based EDS at INITIAL load, has NO such limitation — its §6.2 runs locally.)
- **Multi-endpoint / locality-weighted / priority EDS** (a `ClusterLoadAssignment` with >1 `lb_endpoint`, multiple `LocalityLbEndpoints`, `load_balancing_weight`, `priority`, `policy.overprovisioning_factor`). Phase 21's fixture has ONE endpoint (deterministic differential); multi-endpoint distribution needs the deferred Load-balancing-family distinguishable-backend fixture infrastructure (the ADR-0048 option-(b) gap). The parser accepts the full `endpoints` structure (it reuses `LoadAssignment` verbatim), but the bilateral multi-endpoint distribution fixture defers.
- **Endpoint health-status in EDS** (`LbEndpoint.health_status` = HEALTHY/UNHEALTHY/DRAINING/…; EDS-driven endpoint health overriding/composing with the phase-12 active-HC state machine). A future phase.
- **The EDS-cluster-supplied-by-CDS composition showcase** (a cluster that is BOTH CDS-dynamic AND `type: EDS`). Phase 21 anchors EDS on a STATIC cluster (the minimum-viable fixture, mirroring how 0026/0028 anchored CDS/RDS on static parents). The D3 EDS pass walks `all_clusters()` so it is composition-ready, but the bilateral fixture proving the full CDS+EDS topology defers.
- **`service_name` advanced semantics** beyond simple by-name selection (the EDS-service-registry indirection); **multiple EDS clusters / multiple `ClusterLoadAssignment`s selected from one EDS file** (the parser supports N; the bilateral fixture has one).
- **SDS** (secrets — `sds_config` on a transport socket, the other cluster/socket-scoped config source) + **RTDS** (runtime) — each a future family phase, in whatever order later brainstorms pick.
- **`scoped_routes` / SRDS / VHDS** — carried unchanged from the phase-20 ledger.
- **The gRPC xDS transport** (`api_config_source`/`ads_config`; tonic + envoy-protos/prost; the ADS state machine; an in-harness control plane; **the ADR-0014 protos supersession**) + **delta xDS** + **`initial_fetch_timeout`** (the EDS warming-timeout knob — relevant only once watching/gRPC EDS lands) + **REST xDS** — all carried unchanged from the phase-18/19/20 ledger.

---

## 5. Architectural invariants

### 5.1 No new crate, no new top-level Cargo dep

File I/O = `std::fs` (the phase-18/19/20 sync-load posture — envoy-config keeps zero async deps and the fuzz target stays pure); YAML parsing = `serde_yaml` (existing); the EDS envelope = serde structs (existing pattern; the `LoadAssignment` payload is reused verbatim).

### 5.2 Inert-when-unconfigured (the foundation-slice discipline)

No cluster with `type: EDS` in the bootstrap → zero new stats registered, zero new config_dump entries, zero behavior change. All 28 existing fixtures are byte-identical in expectations and wire behavior (their clusters are STATIC/STRICT_DNS with inline `load_assignment`, which still parses to `Some`). The `load_assignment`→`Option` change is purely additive at the wire level: existing configs deserialize identically. (The phase-15/17/18/19/20 conditional-registration precedent; fixtures 0026 + 0027 + 0028 — which configure dynamic resources but NO EDS cluster — are the critical inertness witnesses: they must see no `cluster.<name>.update_*` names and no `EndpointsConfigDump` entry, and their `configs[]` index assertions must hold.)

### 5.3 Dynamic endpoint assignments are full LoadAssignments; every cluster resolves to one

Every downstream subsystem — the LB pick, the connection pool, the health/outlier state machines, stats — reads a populated `load_assignment` regardless of whether it was inline or EDS-supplied, because the merge happens at config-load time BEFORE `ClusterManager::from_bootstrap` builds endpoints. The post-load invariant: **every cluster has `load_assignment: Some(_)`** (an EDS cluster got it populated by the D3 merge; an inline cluster had it from the start). No cluster-build path carries an "is this endpoint set dynamic?" branch (the only EDS-aware consumers are the `update_*` stats and the config_dump renderer).

### 5.4 Load-at-config-time ownership boundary

EDS file parsing lives in `envoy-config` (it produces `Vec<ClusterLoadAssignment>` configs). The named selection + the merge into the owning cluster's effective `load_assignment` happens inside `load_dynamic_resources` at config-load time. No runtime endpoint mutability, no locks, no watch tasks, no cluster-warming state machine.

### 5.5 config_dump separation + fixture-0026/0027/0028 stability

EDS-supplied endpoint assignments appear in the `EndpointsConfigDump` entry, NOT as a raw `eds_cluster_config` block inflated inside the `ClustersConfigDump` cluster (the cluster entry shows its `type: EDS` + `eds_cluster_config` config faithfully, but the resolved endpoints live in `EndpointsConfigDump`). The `EndpointsConfigDump` entry's insertion must not break fixture 0026's `configs[1]` ClustersConfigDump index assertion, fixture 0027's `configs[2]` ListenersConfigDump assertion, or fixture 0028's `RoutesConfigDump` assertion (none configures EDS, so none emits an Endpoints entry — §6.2 item 5 captures Envoy's entry ordering and confirms index stability).

### 5.6 One-shot load; zero timing sensitivity

The EDS file is read exactly once, synchronously within startup, before the cluster binds its endpoints. Readiness implies loaded on both proxies. The fixture needs no settle window beyond the existing readiness probe. (Envoy's cluster-warming state is transient-then-resolved at initial file load; §6.2 item 2 confirms readiness implies the EDS endpoints are active.)

### 5.7 Merge ordering: CDS clusters before the EDS endpoint pass

`load_dynamic_resources` merges dynamic CLUSTERS (CDS) and dynamic LISTENERS (LDS), populates RDS-supplied `route_config`s, then runs the EDS pass over the full (static + CDS) cluster set, then runs the post-merge re-validation ONCE against the full effective state — so an EDS cluster may be static OR CDS-supplied (the composition is deferred per §4, but the walk is composition-ready), and an RDS/static route's reference to an EDS cluster resolves against a cluster whose endpoints are now populated. A `type: EDS` cluster whose `ClusterLoadAssignment` is absent from its file fails envoy-rust startup (the ADR-0049 decision-2 all-fatal posture; §6.2 item 4 verifies whether Envoy instead warms-and-503s — the divergence recorded).

### 5.8 Exactly-one-of endpoint source

A cluster declares its endpoints via EXACTLY ONE of `load_assignment` (inline) or `eds_cluster_config` (file), consistent with `cluster_type`: `type: EDS` ⇒ `eds_cluster_config` present + `load_assignment` absent; `type: STATIC`/`STRICT_DNS` ⇒ `load_assignment` present + `eds_cluster_config` absent. Any other combination → `ConfigError`, enforced at validation time before any load (so a malformed bootstrap fails fast, before the EDS file is even read).

---

## 6. Implementation signposts for the planner

### 6.1 Split-gate evaluation (split projected NOT to fire; comparable margin to phase 20)

Projected surface: D1 schema + `load_assignment`→`Option` migration ~90 LoC (+~90 tests; the migration touches the endpoint-build read site + the largest `Cluster` struct-literal test sweep of the xDS phases); D2 EDS parsing ~90 (+~100 tests); D3 cluster-walk merge + named-selection + ordering + consumer migration ~130 (+~110 tests); D4 per-cluster EDS stats + handle threading ~100 (+~70 tests); D5 config_dump ~90 (+~70 tests); D6 harness ~80 (+~30 tests; +the optional `xds_file.rs`/render-helper extraction); D7 fixture ~220 (YAML + wrapper); D8 backstop + seed + contract ~190. **Total ~1100–1450 LoC / ~11–13 tasks** — under the ~1500-LoC / ~25-task gate, comparable margin to phase 20. If the §6.2-refined estimate fires the gate, split at the §1 NOTE seam (`21.1` foundation slice / `21.2` observability + fixture + close) with ADR-0055.

### 6.2 Empirical verification at state-2 PLAN-write (HEAVY for this phase; RUNS LOCALLY)

The state-2 PLAN-writer dispatches a single foreground general-purpose subagent (the ADR-0037/0041/0043/0045/0047/0049/0052 methodology) running `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) under Docker with a `type: EDS` cluster + a host backend + admin `/stats` + `/config_dump` scrapes, and verifies — **note: file-based EDS at INITIAL load has NO virtiofs/inotify dependency, so this §6.2 runs locally on macOS Docker (unlike the deferred watching phase)**:

1. **The EDS file envelope shape Envoy accepts** (the most consequential item — the D2 parser is built to this): `@type: type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment` per resource? Bare `resources:` list AND full `DiscoveryResponse` both accepted (the CDS/LDS/RDS L1 finding's mirror)? Capture the exact minimal working file byte-for-byte. **(1b)** the `eds_cluster_config`-on-cluster config shape: `type: EDS` + `eds_cluster_config: { eds_config: { path_config_source: { path }, resource_api_version? }, service_name? }` — exact field names + whether `resource_api_version` is required.
2. **Initial-load/readiness + cluster-warming ordering:** is the EDS cluster's endpoint set active (cluster healthy, route serves) by the time `/ready` returns 200? Does Envoy hold the cluster in "warming" until the EDS file is read, and does file-based EDS resolve warming synchronously at startup (so the first request succeeds without a warm-up window)?
3. **The exact per-cluster `cluster.<name>.*` EDS-subscription stat names + values after a successful initial EDS load** (`update_attempt`/`update_success`/`update_empty`/`update_rejected`? `membership_healthy`/`membership_total`? `version`? the exact relative names). The §2.1 subset is locked from this enumeration. Cross-check which names exist for a STATIC cluster (the conditionality carve for §5.2 — projected: STATIC clusters emit NO `update_*`).
4. **Missing/malformed EDS file + missing-`ClusterLoadAssignment` behavior:** does Envoy hard-exit on a missing path (the CDS/LDS/RDS L4 bootstrap-failure mirror)? Warn-and-serve on a parse error (ticking `update_failure`)? **Critically — does a `type: EDS` cluster whose assignment is ABSENT stay in WARMING and serve `no healthy upstream`/503 for routes to it (rather than fatal-exiting)?** This locks envoy-rust's negative-path disposition — projected: envoy-rust mirrors its phase-18/19/20 all-fatal posture (ADR-0049 decision 2), with the warming-vs-fatal divergence recorded.
5. **The `/config_dump` shape with a dynamic endpoint assignment:** the exact `EndpointsConfigDump` JSON (the `dynamic_endpoint_configs[].endpoint_config` nesting? `cluster_name`? `version_info`? `last_updated`?); the entry ORDERING within `configs[]` (where does Endpoints land relative to Bootstrap/Clusters/Listeners/ScopedRoutes/Routes/Secrets per the ADR-0052 order? — fixtures 0026/0027/0028's index assertions depend on Endpoints NOT displacing them); whether the entry appears when no cluster is EDS.
6. **`load_assignment`/`eds_cluster_config` consistency:** a `type: EDS` cluster with an inline `load_assignment`; a `type: STATIC` cluster with `eds_cluster_config`; a `type: EDS` cluster with neither — what does Envoy do (PGV reject at config-load? which message?). Locks the §5.8 exactly-one-of-and-consistent disposition.
7. **Route-to-EDS-endpoint wire shape:** a GET routed to an EDS-supplied endpoint — identical to a static-endpoint cluster (200 + echo body + `x-envoy-upstream-service-time` + the standard header allow-list)? Any new response header or access-log flag?
8. **`service_name` semantics:** does `eds_cluster_config.service_name` default to the cluster name when absent? Must the `ClusterLoadAssignment.cluster_name` field in the file match `service_name` (when set) or the cluster name? (Locks the D3 named-selection key.)
9. **EDS file shareable vs per-side template + `ClusterLoadAssignment` field tolerance:** does Envoy's `ClusterLoadAssignment` in the EDS file require any field envoy-rust's `LoadAssignment` parser rejects (deciding the D6 shared-vs-per-side template question — projected shareable, unlike the per-side LDS)?
10. **Stat conditionality cross-check:** does a STATIC cluster emit ANY `cluster.<name>.update_*` names (the §5.2 inertness witness — projected NO; the fixture-0011/0026/0027/0028 topologies confirm).
11. **(Opportunistic) `version_info` / warming gauges:** does the initial EDS load carry a version string or a warming-state gauge in stats/config_dump? (Informs whether the BEHAVIOR_CONTRACT records a version/warming-presence divergence; no deliverable depends on it.)

If item 1, 2/4 (the warming-vs-fatal disposition), or 5 (the `EndpointsConfigDump` shape / `configs[]` ordering) diverges materially from the projections → land **ADR-0054** at the PLAN-write commit (mirrors ADR-0037/0041/0043/0045/0047/0049/0052).

### 6.3 In-process backstop assertions (heeds the 14.2→20 both-paths lesson)

The backstop covers BOTH the happy path (valid EDS file → probe 200 + stats + config_dump) AND the negative paths (missing EDS file; malformed EDS file; `ClusterLoadAssignment` matching no `service_name`/cluster name; an EDS cluster with an inline `load_assignment`; a non-EDS cluster with `eds_cluster_config`; an EDS cluster with neither) per the §6.2-verified dispositions — the paths the differential fixture cannot exercise.

### 6.4 The 06.x stats convention + the inert-when-unconfigured discipline

Stat handles are `Arc<Counter>`/`Arc<Gauge>` registered once at construction; increments at single sites. Conditional registration per §5.2 — the phase-18 template at `cluster.rs:1060-1097` is the technique; the per-cluster `cluster_type == Eds` keying is the predicate. The increment-at-load-time handle threading (D4) is the one non-obvious wiring decision (the phase-20 D4 precedent).

### 6.5 Pre-state-4 fmt + clippy discipline (heeds `project_state3_arc_skips_clippy`)

`cargo clippy --workspace --all-targets --all-features -- -D warnings` runs PER TASK in the state-3 arc. The D1 `Option` migration (`needless_borrow`/`single_match` on the unwrap sites), the D3 cluster walk (iterator-lint candidates), and the D5 enum extension (`collapsible_if`) are the likely lint sites.

### 6.6 State-4 evidence-discipline (continues per 05.3 → … → 20 chain)

Per-gate command outputs quoted into PROGRESS Task-N; a single Docker-gated CI run as the anchor. The phase-18 lesson (carryforward disposition 2): the CI-evidence check is load-bearing. Pre-build `tests/helpers/*` before `cargo test --workspace` (the cold-helper-compile flake class per `project_flaky_access_log_fixture_0012` — extends to any backstop that `cargo run`s a helper, incl. 0029's).

### 6.7 Isolated-crate build discipline (heeds `project_isolated_crate_build_blindspot`)

The state-4 verification MUST run `cargo build -p envoy-config -p envoy-cluster -p envoy-http1 -p envoy-http2` standalone in addition to the workspace build (the `load_assignment`→`Option` change ripples through envoy-cluster most heavily; envoy-http1/envoy-http2 via the cluster dependency).

### 6.8 Cargo.lock cadence

No new top-level deps projected → no Cargo.lock churn beyond version bumps already in flight.

### 6.9 PLAN.md + PROGRESS.md skeleton + Task 1 preamble land alongside at state-2

The 06.2→20 standalone-PLAN cadence: one pre-Task-1 docs-only commit (PLAN + PROGRESS skeleton + Task 1 preamble + ROADMAP flip + STATE advance + any §6.2 ADR).

### 6.10 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The state-3 arc dispatches PLAN tasks to fresh subagents SERIALLY (`feedback_serial_subagent_dispatch`), each with two-stage review (spec-compliance THEN code-quality), TDD per task, one code commit + one PROGRESS commit per task.

### 6.11 The `xds_file.rs` / dynamic-file-render-helper consolidation opportunity (phase-19 M19-1 + phase-20 M20-T6-a, now N=4)

EDS makes the `@type`-tagged envelope parser a FOURTH copy-paste sibling (`cds.rs`, `lds.rs`, `rds.rs`, + EDS) AND the harness dynamic-file render block a FOURTH triplication-now-quadruplication (the M20-T6-a "extract a dynamic-file render helper" item). Both consolidations were DEFERRED at phase 20 by deliberate decision (its carryforward disposition 4 — risk-managed under the every-phase-green doctrine). The PLAN-writer re-weighs BOTH now against the LoC budget: the parser generalization (`parse_xds_file<T>`) and the harness render helper are net simplifications and forward-useful for the gRPC/ADS phase's resource dispatch. If either consolidation lands, it is in-scope refactoring (the brainstorming-skill "improve code you're working in" discipline), recorded in the PLAN; if deferred again, the rationale is recorded.

---

## 7. Conditional ADRs (projected; land at PLAN-write or in-execution if they fire)

- **ADR-0053 (the scoping ADR) — LANDS AT THIS BRAINSTORM COMMIT.** The continuation pick + the §0 findings + the minimum-viable scope boundary + the deferral ledger. (The ADR-0042/0044/0046/0048/0050/0051 brainstorm-time cadence.)
- **ADR-0054 (§6.2 empirical-verification reconciliation) — PLAUSIBLE.** Fires if §6.2 item 1 (the EDS file envelope / `eds_cluster_config`-on-cluster shape), item 2/4 (the cluster-warming + missing-file/missing-resource disposition — note Envoy's EDS warming posture may DIVERGE from envoy-rust's projected all-fatal posture, the most likely trigger this phase), or item 5 (the `EndpointsConfigDump` shape / `configs[]` ordering — a fixture-0026/0027/0028 compatibility trigger) diverges materially from the projections. Lands at the state-2 PLAN-write commit. Mirrors ADR-0037/0041/0043/0045/0047/0049/0052.
- **ADR-0055 (phase split) — POSSIBLE (projected NOT to fire, comparable margin to phase 20).** Fires only if the §6.2-refined estimate exceeds ~1500 LoC / ~25 tasks. Seam per §1 NOTE / §6.1. Mirrors ADR-0036/0038/0040.

---

## 8. Summary

Phase 21 continues the **xDS / dynamic config family** at its next increment: **file-based EDS**. A cluster setting `type: EDS` + pointing `eds_cluster_config.eds_config.path_config_source` at a YAML file (instead of carrying an inline `load_assignment`) gets its endpoint list loaded at startup, routing traffic, and observable via the per-cluster `cluster.<name>.{update_attempt,update_success,…}` EDS-subscription stats and `/config_dump`'s `EndpointsConfigDump` — bilaterally verified by fixture `0029-xds-file-based-eds`, anchored on a static cluster (the minimum-viable shape, deferring the EDS-cluster-supplied-by-CDS composition). With phases 18 (CDS) + 19 (LDS) + 20 (RDS) done, EDS extends the filesystem-dynamic-config surface down to the endpoint layer, reusing the named/scoped config-source idiom RDS introduced (`route_config_name` → `service_name`) — all with zero concurrency, zero timing sensitivity, zero new crates, and zero new dependencies, and (unlike the deferred watching phase) a §6.2 verification that runs LOCALLY. The genuinely-new surfaces (the `ClusterType::Eds` + `eds_cluster_config` cluster-scoped schema surgery, the `load_assignment`→`Option` migration, the per-cluster EDS `update_*` stat family, and the effective-`load_assignment` threading) keep it comparable in weight to the RDS phase but comfortably single-phase; the hard xDS surfaces (file watching — whose ROI this phase strictly improves to its best yet, multi-endpoint/locality EDS, the EDS+CDS composition, SDS/RTDS, scoped_routes/SRDS/VHDS, the gRPC/ADS state machine, delta) remain cleanly deferred with named owners. After this phase, the filesystem-transport surface covers clusters, listeners, routes, AND endpoints — the four core file-based xDS resource types, setting up the watching capstone to light up all four at once.
