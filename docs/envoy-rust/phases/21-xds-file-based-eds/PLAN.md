# Phase 21 (`21-xds-file-based-eds`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (the project default per `feedback_execution_style`) to implement this plan task-by-task, SERIALLY (per `feedback_serial_subagent_dispatch` — never dispatch implementers in parallel; they race on shared `main`, and the harness garbles large parallel tool batches). Steps use checkbox (`- [ ]`) syntax for tracking. TDD per task (`superpowers:test-driven-development` — tests first). Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` **PER TASK** (per `project_state3_arc_skips_clippy` — the per-task verification otherwise runs build/test/fmt but NOT clippy, and lints would first surface at the state-4 gate). One code commit + one PROGRESS commit per task.

**Goal:** Make file-based EDS work end-to-end — a cluster that sets `type: EDS` + `eds_cluster_config.eds_config.path_config_source.path` (instead of an inline `load_assignment`) loads its `ClusterLoadAssignment` (endpoints) from a YAML file at startup, routes data-plane traffic to those endpoints, and exposes the load via the per-cluster `cluster.<name>.{update_attempt,update_success,update_failure,update_empty}` EDS-subscription stats + the `/config_dump` `EndpointsConfigDump` section — bilaterally verified against upstream Envoy by fixture `0029-xds-file-based-eds`.

**Architecture:** Reuse the phase-18/19/20 filesystem-transport machinery (the `ConfigSource`/`PathConfigSource` structs, the `@type`-tagged envelope parser, the `load_dynamic_resources` config-load-time merge, the conditional-registration technique, the per-side `JsonSubtreeRule` path override [ADR-0052], the harness dynamic-file rendering/mounting). The genuinely-new surfaces: (1) the `ClusterType::Eds` + `eds_cluster_config`-on-cluster schema surgery + making the REQUIRED `Cluster.load_assignment` an `Option`, validated EXACTLY-ONE-OF-AND-CONSISTENT-WITH-`cluster_type` — the FIRST CLUSTER-scoped dynamic resource; (2) the per-cluster `cluster.<name>.update_*` EDS-subscription stat family (registered at the existing per-cluster stat site, distinct from the manager-level CDS/LDS singletons and the per-HCM RDS family); (3) the effective-`load_assignment` threading (`load_dynamic_resources` walks every cluster, name-selects its `ClusterLoadAssignment` from the EDS file by `service_name`-or-cluster-name, and populates `load_assignment` so downstream endpoint construction sees a uniform shape — no runtime endpoint mutability, no locks, no watch tasks). Initial-load-only, synchronous, deterministic, zero timing sensitivity, §6.2 verified LOCALLY.

**Tech Stack:** Rust (stable, pinned). `serde`/`serde_yaml` (config parsing — existing). `std::fs` (sync file read — existing). `envoy-stats` (counters — existing). No new crate, no new top-level Cargo dep, no new harness driver, no new helper binary, no concurrency machinery.

---

## §6.2 empirical lock-ins (verified against `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, darwin/Docker, 2026-06-05; **reconciliation ADR-0054 FIRES — item 5 EndpointsConfigDump + item 1 numeric-IP + item 2/4 warm-vs-fatal + item 6a all diverged**; ran LOCALLY)

The full statement is **ADR-0054** (DECISIONS.md). Summary of the lock-ins the tasks below depend on:

- **L1 (envelope / `eds_cluster_config`-on-cluster shape) — MATCH, + a NEW numeric-IP constraint.** Both bare `resources:` and full `DiscoveryResponse` accepted; per-resource `@type: type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment` (REQUIRED). Cluster block: `type: EDS` + `eds_cluster_config: { eds_config: { path_config_source: { path }, resource_api_version? }, service_name? }` (no inline `load_assignment`); `resource_api_version` + `service_name` both OPTIONAL. **The endpoint `socket_address.address` MUST be a NUMERIC IP** — a hostname is rejected (`malformed IP address` → `update_rejected`). EDS endpoints are resolved socket addresses (STATIC semantics, NOT STRICT_DNS).
- **L2 (readiness / warming) — MATCH.** EDS endpoints active before `/ready` 200; first request succeeds immediately, no warm-up window; `warming_state: 0`. envoy-rust mirrors via synchronous `load_dynamic_resources`.
- **L3 (per-cluster stat names/values) — enumerated; subset locked.** After a successful initial load, the §2.1 minimum-viable subset is `cluster.<name>.update_{attempt,success,failure,empty}` (value-exact **1 / 1 / 0 / 0**), with the data-plane witness `cluster.<name>.upstream_rq_total` (**1**, exists unconditionally). **`membership_healthy`/`membership_total` are NOT asserted** — a verified envoy-rust narrowing: `membership_healthy` registers only when `health_checks` is configured (`cluster.rs:926`; the "no membership_healthy gauge for a plain cluster" inertness test at `:2227`), and `membership_total` does NOT exist in envoy-rust at all; fixture 0029 has no health checks. Envoy emits both for every cluster → allow-listed envoy-only (NOT broadened — broadening would touch the existing inertness test + change existing-fixture stat output, out of the minimum-viable scope). Other Envoy-only / NOT asserted: `update_no_rebuild`, `update_rejected` (structurally 0 in envoy-rust — L4 all-fatal), `update_time`, `update_duration` (histogram), `membership_change`/`degraded`/`excluded`, `assignment_*`, `version`/`version_text`, `warming_state`.
- **L4 (missing/malformed/missing-resource disposition) — warm-and-503 on Envoy; ONLY missing-FILE-PATH fatal on both.** (a) missing file PATH = Envoy hard-exit at config-load; (b) malformed content = Envoy `update_failure: 1` + 503; (c) missing/mismatched `ClusterLoadAssignment` = Envoy `update_rejected: 1` + 503 (`Unexpected EDS cluster (expecting <name>): <other>`); (d) empty `resources: []` = Envoy `update_empty: 1` + `update_success: 1` + 503. **envoy-rust = ALL FATAL** (the ADR-0049 decision-2 posture extended to EDS): missing-file `EdsFileError`, malformed `EdsParseError`, missing-CLA `EdsClusterLoadAssignmentNotFound`, empty-endpoints `EmptyClusterEndpoints` (existing) → `update_failure`/`update_empty`/`update_rejected` register at 0, structurally unreachable; negative paths backstop-only. Only (a) is fatal on BOTH.
- **L5 (EndpointsConfigDump shape + `configs[]` ordering) — DIVERGES MATERIALLY (the ADR-0054 trigger).** **(1)** Envoy OMITS `EndpointsConfigDump` from the DEFAULT `/config_dump` — only `/config_dump?include_eds` surfaces it. **(2)** file-based EDS endpoints land under `static_endpoint_configs[]`, NOT `dynamic_endpoint_configs[]`. **(3)** shape: `{ "@type": ".../EndpointsConfigDump", "static_endpoint_configs": [ { "endpoint_config": { "@type": ".../ClusterLoadAssignment", "cluster_name": "eds_backend", "endpoints": [...], "policy": {...} } } ] }` — no `version_info`/`last_updated`. **(4)** `?include_eds` order: Bootstrap[0], Clusters[1], **Endpoints[2]**, Listeners[3], ScopedRoutes[4], Routes[5], Secrets[6]. **Reconciliation (Tasks 5/6/7):** envoy-rust emits `EndpointsConfigDump` (conditional on some cluster being EDS) with `static_endpoint_configs[].endpoint_config`, pushed after Clusters / before Listeners; envoy-rust's admin STRIPS the query string (so `/config_dump?include_eds` routes to ConfigDump) and emits unconditionally-when-EDS (narrowing vs Envoy's `?include_eds`-gating); the fixture scrapes `/config_dump?include_eds` and asserts via a per-side `JsonSubtreeRule` (Envoy `configs.2`, envoy-rust `configs.1` — fixture 0029 has no `cds_config` so envoy-rust emits no ClustersConfigDump). REUSES the ADR-0052 per-side path mechanism (no new harness JSON code).
- **L6 (consistency validation) — 6b/6c MATCH (hard-exit), 6a DIVERGES (Envoy accepts).** 6a EDS+inline `load_assignment` = Envoy ACCEPTS (ignores inline); envoy-rust STRICTER reject (`LoadAssignmentOnEdsCluster`, recorded). 6b STATIC+`eds_cluster_config` = Envoy hard-exit (`eds_cluster_config set in a non-EDS cluster`); envoy-rust `EdsConfigOnNonEdsCluster`. 6c EDS+neither = Envoy hard-exit (`cannot create an EDS cluster without an EDS config`); envoy-rust `MissingEdsClusterConfig`. Plus: a non-EDS cluster with no `load_assignment` → envoy-rust `MissingLoadAssignment`.
- **L7 (wire shape) — MATCH.** A GET to an EDS endpoint is byte-identical to a static-endpoint response (200 + echo body + `x-envoy-upstream-service-time` + `server: envoy` + standard allow-list; no EDS-specific header).
- **L8 (`service_name` selection) — MATCH.** `service_name` unset → file CLA `cluster_name` must equal the cluster name; `service_name: X` set → file CLA `cluster_name` must equal `X`. The D3 selection key = `eds_cluster_config.service_name.unwrap_or(cluster.name)`.
- **L9 (file shareable-as-TEMPLATE + per-side numeric IP) — DIVERGES from the projected plain-shareable.** A minimal `ClusterLoadAssignment` is accepted verbatim (no extra input field), BUT the backend `socket_address.address` must be a NUMERIC IP that differs per side — so the EDS file is a SHARED template (`eds.yaml`) rendered per-side via a NEW `{{EDS_BACKEND_IP}}` kv marker (upstream → the runtime-discovered numeric host-gateway IP; subject → `127.0.0.1`). The harness DISCOVERS the numeric host-gateway IP (one-shot `getent hosts host.docker.internal` in the pinned Envoy image; gated to EDS fixtures). The EDS rendition joins the backend/host-gateway scans (the phase-18 bug-class lesson). **This is the load-bearing harness reconciliation (D6).**
- **L10 (stat conditionality) — DIVERGES (STATIC emits `update_*` at 0) but PRE-EXISTING + already tolerated.** STATIC clusters emit `update_{attempt,success,failure,empty,no_rebuild}` at 0 (only `update_rejected` is EDS-exclusive). PRE-EXISTING Envoy behavior; the existing 28 fixtures already tolerate it (envoy-only allow-list / fixture-0011 set-diff). envoy-rust conditionally registers `update_*` ONLY for `cluster_type == Eds` → adds ZERO names to the existing fixtures. The inertness backstop witnesses it.
- **L11 (version) — Envoy-only.** `cluster.<name>.version` is a nonzero xxhash always; not asserted.

---

## PLAN-time SPEC corrections (verified against HEAD `f350d952f` by a read-only `Explore` survey + controller direct-grep re-verification)

All SPEC §0/§3 code anchors confirmed at HEAD with **NO drift** (the SPEC line numbers were captured at an older HEAD; current locations below):

- **C1.** `Cluster` struct at `crates/envoy-config/src/bootstrap.rs:174-223`; `load_assignment: LoadAssignment` (REQUIRED, non-Option) at `:181`. The full field list: `name`, `cluster_type` (renamed `type`), `lb_policy`, `load_assignment`, `transport_socket?`, `dns_lookup_family?`, `typed_extension_protocol_options?`, `health_checks` (Vec), `common_lb_config?`, `circuit_breakers?`, `outlier_detection?`. (SPEC anchor exact.)
- **C2.** `ClusterType` enum at `bootstrap.rs:225-239` — `#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]`, two variants `Static` (→ `STATIC`) + `StrictDns` (→ `STRICT_DNS`). Adding `Eds` (→ `EDS`) needs NO serde rename (the `rename_all` handles it). (Exact.)
- **C3.** `LoadAssignment` at `bootstrap.rs:262-267` (`cluster_name: String` + `endpoints: Vec<LocalityLbEndpoints>`); `LocalityLbEndpoints` `:312-316`; `LbEndpoint` `:318-322`; `Endpoint` `:324-328`; `Address` `:348-352`; `SocketAddress` `:354-359` (`address: String` + `port_value: u16`). The EDS payload reuses `LoadAssignment` VERBATIM. (Exact.)
- **C4.** `DynamicResources` (`cds_config`/`lds_config: Option<ConfigSource>`) at `bootstrap.rs:92-102`; `ConfigSource` (`path_config_source` + `resource_api_version: Option<String>`, `deny_unknown_fields`) at `:107-115` — REUSED verbatim by `EdsClusterConfig`; `PathConfigSource` (`path: String`) at `:119-123`. `ConfigSource`'s `deny_unknown_fields` rejects `api_config_source`/`ads`/`watched_directory` (the deferred surfaces — §4). (Exact.)
- **C5.** `cds::parse_cds_file(path, contents) -> Result<Vec<Cluster>, ConfigError>` at `cds.rs:57`; `@type` `type.googleapis.com/envoy.config.cluster.v3.Cluster` at `:44`. `lds.rs:53`/`rds.rs:51` mirror. The new `eds.rs` mirrors `cds.rs` (the closest analogue — both produce a `Vec` of an existing schema struct). (Exact.)
- **C6.** `load_dynamic_resources(bootstrap: &mut Bootstrap) -> Result<(), ConfigError>` at `lib.rs:623`; CDS merge `:624-660`, LDS merge `:662-694`, the `check_route_sources` re-check `:701`, the RDS pass `:703-749`, the single post-merge `bootstrap::validate(bootstrap)?` `:751-762` gated on `dynamic_clusters.is_some() || dynamic_listeners.is_some() || had_rds_hcm`. **The EDS pass slots AFTER the RDS pass (`:749`) and BEFORE the post-merge validate (`:751`); the validate gate MUST extend to include `has_eds_cluster` (C16).** (Exact.)
- **C7.** envoy-bin startup (`main.rs`): read file → `parse_bootstrap` → `load_dynamic_resources(&mut bootstrap)?` → `Arc::new(bootstrap)` → `ClusterManager::from_bootstrap` (reads `load_assignment` AFTER the EDS pass populated it). (Exact — the EDS merge precedes cluster construction.)
- **C8.** `validate_cluster(cluster: &Cluster)` at `bootstrap.rs:2261-2278` — `LoadAssignmentNameMismatch` (reads `cluster.load_assignment.cluster_name` `:2262/2265`) + `EmptyClusterEndpoints` (reads `.load_assignment.endpoints` `:2269`). After the migration these reads guard on `Option`; the name-mismatch check applies to **inline (non-EDS) clusters only** (an EDS cluster's populated CLA `cluster_name` equals `service_name`, NOT the cluster name — L8/§5.8; re-checking against `cluster.name` would falsely reject). (Exact + the EDS guard is new.)
- **C9.** Endpoint-build dispatch at `crates/envoy-cluster/src/cluster.rs:730-793` — `for locality in &cfg.load_assignment.endpoints` `:735` (the consumer that must `Option`-unwrap), with `match cfg.cluster_type { Static => SocketAddr::from_str …, StrictDns => lookup_host … }`. The new `Eds` arm shares the `Static` arm (EDS endpoints are numeric — L1). (Exact.)
- **C10.** Per-cluster stat registration at `cluster.rs:817-947` (builds `cluster.<name>.*`, owns `membership_healthy` at `:926-946`). The conditional EDS `update_*` sub-registration lands here, gated on `cfg.cluster_type == Eds`. (Exact.)
- **C11.** Conditional `cluster_manager.cds.*` template at `cluster.rs:1059-1097` (gated on `dynamic_resources.cds_config.is_some()`; `mk` closure; `register_counter(...)?.add(1)`). The technique the EDS `update_*` registration follows, with the per-cluster `cluster_type == Eds` predicate. (Exact.)
- **C12.** Immutable `ClusterManager` at `cluster.rs:598-644` (`clusters: HashMap<String, Arc<Cluster>>`; `get`/`clusters`/`empty` only; constructed once at `:1098`). No runtime mutator — the EDS merge happens at config-load time. (Exact.)
- **C13.** `ConfigDumpEntry<'a>` enum at `crates/envoy-admin/src/endpoint.rs:303-350` (`#[serde(tag = "@type")]`; variants `Bootstrap` + `Clusters` + `Listeners` + `Routes`); the path dispatch `match path { "/config_dump" => ConfigDump … }` at `:97-101` (**exact-match — needs the query-strip, D5**); the render push-order at `:500-607`: `Bootstrap` unconditional `:500`; `Clusters` iff `needs_cds` `:537`; `Listeners` iff `needs_lds` `:580`; `Routes` iff `had_rds_hcm` `:607`. The new `Endpoints` variant + its push (iff some cluster is EDS) go AFTER `Clusters` and BEFORE `Listeners`. (Exact.)
- **C14.** `ConfigError` enum in `lib.rs`; naming convention `{Resource}{ErrorType}` (`CdsFileError`/`CdsParseError`, `LdsFileError`/`LdsParseError`, `RdsFileError`/`RdsParseError`/`RdsRouteConfigNotFound`). The new variants: `EdsFileError { path, source }`, `EdsParseError { path, message }`, `EdsClusterLoadAssignmentNotFound { name, path }`, `MissingLoadAssignment { cluster }`, `MissingEdsClusterConfig { cluster }`, `EdsConfigOnNonEdsCluster { cluster }`, `LoadAssignmentOnEdsCluster { cluster }`.
- **C15 (corpus).** The git-tracked curated fuzz corpus is **31** seeds (the `.gitignore` allow-list in `crates/envoy-config/fuzz/.gitignore`, last three `dynamic_resources_cds.yaml` + `dynamic_resources_lds.yaml` + `hcm_rds_route_config.yaml`). Phase 21 adds `cluster_eds.yaml` → **31 → 32** (matches SPEC §1 (d)).
- **C16 (the validate-gate + EDS-pass-trigger).** Fixture 0029 has a STATIC cluster with `type: EDS` but NO `dynamic_resources`/`rds` — so the phase-20 validate gate (`dynamic_clusters || dynamic_listeners || had_rds_hcm`) would NOT fire, and the post-merge re-validation would be skipped. **The EDS pass MUST run whenever ANY cluster (static or CDS-dynamic) has `cluster_type == Eds`, regardless of `dynamic_resources`; and the post-merge validate gate MUST extend to include `has_eds_cluster`** (so the EDS-populated `load_assignment` is re-validated). Add a `has_eds_cluster(&Bootstrap) -> bool` helper (walks the effective static+dynamic cluster set), or set a `bool` flag inside the EDS pass.
- **C17 (the D1 migration sweep — SMALLER than feared).** The first survey conflated YAML-in-string test fixtures with Rust struct literals. The actual Rust `Cluster {…}` struct-literal construction sites needing `eds_cluster_config: None` (+ `load_assignment: Some(...)`) number **~17** — concentrated in `crates/envoy-cluster/src/cluster.rs` (~12) + `crates/envoy-admin/src/endpoint.rs` (~4 tests) + `bootstrap.rs` (~1) (the 85 `load_assignment:` occurrences in `bootstrap.rs` are YAML strings parsed by `serde`, NOT Rust literals — they deserialize to `Some` unchanged, NO edit). Plus the production reads (`cluster.rs:735` endpoint build, `bootstrap.rs:2262/2265/2269` validator) + ~12 `.load_assignment` test reads in `bootstrap.rs` (`.as_ref().unwrap()` adaptation). Comparable to phase-18 (26 sites) / phase-20 (11 sites). **Confirm the exact literal set with a workspace build at Task 1 — the build is RED until all are swept (fix in the SAME commit).**
- **C18 (the `eds.rs` / dynamic-file-render-helper consolidation — DEFERRED, deliberate; SPEC §6.11, now N=4).** EDS makes the `@type`-tagged parser a FOURTH copy-paste sibling AND the harness render block a FOURTH triplication-now-quadruplication. **Decision: write a NEW `eds.rs` mirroring `cds.rs`; render `{{EDS_PATH}}` as a fourth shared-template branch — DEFER both consolidations** (the phase-20 C17 risk-managed decision: consolidating `parse_xds_file<T>` / a render helper touches three currently-green modules + the green harness for a cleanliness win, against the D-3.6 every-phase-green doctrine + the §5.1 one-state-per-session budget). The consolidations stay future hardening items (recorded in PROGRESS rollovers; M19-1/M20-T6-a remain open at N≥4). A reviewer should read this as a risk-managed choice, not an oversight.
- **C19 (BootstrapConfigDump note).** The EDS pass mutates `load_assignment` in-place on the bootstrap. The `BootstrapConfigDump` for a static EDS cluster will therefore show the POPULATED `load_assignment` (a known minor divergence vs Envoy, which shows the cluster as-configured with no resolved endpoints in BootstrapConfigDump) — but it is NOT asserted (fixture 0029's config_dump probe asserts only the `EndpointsConfigDump` `cluster_name` subtree; the surrounding `configs` array is `value_may_differ`). The `EndpointsConfigDump` is the faithful resolved-endpoints surface (§5.5). Recorded in the fixture README + BEHAVIOR_CONTRACT.

---

## §6.1 split-gate decision

**Split does NOT fire.** The §6.2-refined estimate: D1 schema + `load_assignment`→`Option` migration + the Eds cluster-build arm + the exactly-one-of validator (~4 variants) ~110 prod + the ~17-site + ~16-read sweep + ~100 tests; D2 `eds.rs` ~70 + ~100 tests; D3 cluster-walk merge + name-select + ordering + the validate-gate extension + consumer migration ~130 + ~110 tests; D4 per-cluster EDS `update_*` stats (deterministic values, register-and-set per L3 — no handle threading) ~90 + ~70 tests; D5 config_dump `Endpoints` variant (`static_endpoint_configs`) + query-strip ~110 + ~70 tests; D6 harness `{{EDS_PATH}}` (shared template) + `{{EDS_BACKEND_IP}}` per-side numeric marker + the host-gateway-IP discovery (~40) + scans + (the per-side `JsonSubtreeRule` REUSED from phase 20, no new code) ~110 + ~30 tests; D7 fixture ~210 (YAML + wrapper); D8 backstop + seed + contract ~190. **Total ~1150–1500 LoC / 11 tasks** — under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task gate, comparable margin to phase 20. **ADR-0055 (split) does NOT fire** (reserved-but-unconsumed).

---

## File structure

| File | Create/Modify | Responsibility |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | Modify | `ClusterType::Eds`; `EdsClusterConfig` struct; `load_assignment: Option<LoadAssignment>` + `eds_cluster_config: Option<EdsClusterConfig>`; `validate_cluster` exactly-one-of-and-consistent check + `Option` guards + the EDS name-mismatch carve |
| `crates/envoy-config/src/lib.rs` | Modify | `load_dynamic_resources` EDS pass (cluster walk + name-select + populate); the validate-gate extension (`has_eds_cluster`); new `ConfigError` variants; `pub mod eds;` |
| `crates/envoy-config/src/eds.rs` | Create | `parse_eds_file(path, contents) -> Result<Vec<LoadAssignment>, ConfigError>` (mirrors `cds.rs`) |
| `crates/envoy-cluster/src/cluster.rs` | Modify | the endpoint-build `Eds` match arm (numeric, shares `Static`) + the `load_assignment` `Option`-unwrap `:735`; the conditional per-cluster `cluster.<name>.update_*` registration `:817-947` |
| `crates/envoy-admin/src/endpoint.rs` | Modify | `ConfigDumpEntry::Endpoints` variant (`static_endpoint_configs[].endpoint_config`) + conditional push after Clusters; the admin path-dispatch query-string strip; the ~4 test `Cluster {}` literal sweep |
| `crates/envoy-http1/src/hcm.rs`, `crates/envoy-http2/src/hcm.rs`, `crates/envoy-health/src/scheduler.rs`, `crates/envoy-tcp/src/lib.rs`, etc. | Modify | the remaining Rust `Cluster {}` literal sweep sites (`eds_cluster_config: None`, `load_assignment: Some(...)`) — same commit (Task 1) |
| `crates/envoy-bin/src/main.rs` | Modify | (only if D4 needs a call site — see Task 4; the per-cluster registration is inside `from_bootstrap`, so likely NO main.rs change) |
| `tests/differential/src/lib.rs` | Modify | `{{EDS_PATH}}` shared-template rendering/mounting; the `{{EDS_BACKEND_IP}}` per-side numeric marker + host-gateway-IP discovery; the EDS rendition in the backend/host-gateway scans; (the per-side `JsonSubtreeRule` reused) |
| `tests/fixtures/0029-xds-file-based-eds/` | Create | `envoy.yaml` + `envoy-rust.yaml` + `eds.yaml` (shared template) + `expectations.yaml` + `README.md` |
| `tests/differential/tests/xds_file_based_eds.rs` | Create | Docker-gated wrapper |
| `crates/envoy-bin/tests/xds_file_based_eds.rs` | Create | in-process backstop (happy + 6 negative + inertness) |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_eds.yaml` | Create | fuzz seed (corpus 31 → 32) |
| `crates/envoy-config/fuzz/.gitignore` | Modify | allow-list the new seed |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | Modify | EDS stat rows + the xDS-section EDS extension + the EndpointsConfigDump admin-body-shapes row |

---

### Task 1: `envoy-config` schema — `ClusterType::Eds` + `EdsClusterConfig` + `load_assignment` → `Option` + the exactly-one-of-and-consistent validator + the Eds cluster-build arm + the D1 migration sweep

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`ClusterType` `:225-239`; `Cluster` `:174-223`; `validate_cluster` `:2261-2278`)
- Modify: `crates/envoy-config/src/lib.rs` (`ConfigError` variants)
- Modify: `crates/envoy-cluster/src/cluster.rs` (the `Eds` match arm + the `:735` `Option`-unwrap)
- Modify: every Rust `Cluster {}` literal site (the ~17 — same commit, the workspace-compile carry-forward)
- Test: `crates/envoy-config/src/bootstrap.rs` + `lib.rs` test modules

> **CARRY-FORWARD WARNING (D1):** making `load_assignment` an `Option` + adding `eds_cluster_config` is a workspace-compile-affecting change. YAML fixtures with inline `load_assignment:` still parse (→ `Some`), but every Rust site that CONSTRUCTS `Cluster { … load_assignment: <expr>, … }` (~17 literals — confirm by building; concentrated in `envoy-cluster/src/cluster.rs`, `envoy-admin/src/endpoint.rs` tests, plus scattered single sites in `envoy-http1`/`envoy-http2`/`envoy-health`/`envoy-tcp`/`envoy-bin` tests) must change to `load_assignment: Some(<expr>), eds_cluster_config: None,` and every site that READS `.load_assignment` (production: `cluster.rs:735`, `bootstrap.rs:2262/2265/2269`; ~12 `bootstrap.rs` test reads) must adapt. **The build is RED until ALL sites are fixed — fix them all in this task** (the phase-18/19/20 struct-literal-sweep precedent). Also: adding `ClusterType::Eds` makes the `match cfg.cluster_type` at `cluster.rs:735-793` non-exhaustive → the `Eds` arm MUST be added in this task (it shares the `Static` arm).

- [ ] **Step 1: Write failing tests** (`crates/envoy-config` test module). (a) **`EdsClusterConfig` parses:** a YAML cluster `type: EDS` + `eds_cluster_config: { eds_config: { path_config_source: { path: /x } }, service_name: svc }` + NO `load_assignment` → `parse_bootstrap` succeeds; `cluster.cluster_type == ClusterType::Eds`, `eds_cluster_config == Some(EdsClusterConfig { eds_config: ConfigSource { path_config_source: PathConfigSource { path: "/x" }, resource_api_version: None }, service_name: Some("svc") })`, `load_assignment == None`. (b) **`service_name` + `resource_api_version` optional:** `eds_cluster_config: { eds_config: { path_config_source: { path: /x } } }` → `service_name == None`. (c) **inline STATIC cluster still parses to `Some`:** an existing-shape STATIC cluster with inline `load_assignment:` → `load_assignment.is_some()`, `eds_cluster_config.is_none()` (regression-equivalence). (d) **EDS + neither → `MissingEdsClusterConfig`:** `type: EDS` with no `eds_cluster_config` and no `load_assignment` → `Err(ConfigError::MissingEdsClusterConfig { .. })`. (e) **EDS + inline `load_assignment` → `LoadAssignmentOnEdsCluster`** (L6 6a — stricter than Envoy): `type: EDS` + `eds_cluster_config` + an inline `load_assignment` → `Err(LoadAssignmentOnEdsCluster { .. })`. (f) **STATIC + `eds_cluster_config` → `EdsConfigOnNonEdsCluster`** (L6 6b): `type: STATIC` + `load_assignment` + `eds_cluster_config` → `Err(EdsConfigOnNonEdsCluster { .. })`. (g) **STATIC + neither → `MissingLoadAssignment`:** `type: STATIC` with no `load_assignment` → `Err(MissingLoadAssignment { .. })`. (h) **unknown field inside `eds_cluster_config` rejected:** `eds_cluster_config: { eds_config: {...}, ads: {} }` → parse error (`deny_unknown_fields`). (i) **`api_config_source`/`ads`/`watched_directory` inside `eds_config` still rejected** (`ConfigSource`'s `deny_unknown_fields` unchanged).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config eds_schema`. Expected: FAIL (compile error — `Eds`/`EdsClusterConfig`/the new variants undefined).
- [ ] **Step 3: Implement the schema** in `bootstrap.rs`:

```rust
// ClusterType (:225-239): add the third variant (rename_all handles EDS).
pub enum ClusterType {
    Static,
    StrictDns,
    /// 21 D1 (ADR-0053/0054): endpoints loaded from a ClusterLoadAssignment file
    /// via eds_cluster_config (initial-load-only). Endpoints are resolved numeric
    /// socket addresses (STATIC semantics — L1), populated by load_dynamic_resources.
    Eds,
}

// Cluster (:181): load_assignment becomes Option + new eds_cluster_config sibling.
// Replace `pub load_assignment: LoadAssignment,` with:
    /// 21 D1 (ADR-0053/0054): the inline endpoint list. EXACTLY ONE of
    /// `load_assignment` (inline; STATIC/STRICT_DNS) or `eds_cluster_config`
    /// (file; EDS) per cluster, consistent with `cluster_type` (enforced at
    /// validation — §5.8). After load_dynamic_resources populates an EDS
    /// cluster's load_assignment from its file, it is Some (§5.3); downstream
    /// endpoint construction reads it uniformly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_assignment: Option<LoadAssignment>,
    /// 21 D1: EDS — endpoints loaded from a file (reuses ConfigSource verbatim).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eds_cluster_config: Option<EdsClusterConfig>,

// New struct (near ConfigSource, ~:124):
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EdsClusterConfig {
    pub eds_config: ConfigSource, // reused verbatim from phase 18
    /// Selects the ClusterLoadAssignment by name; defaults to the cluster name (L8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
}
```

- [ ] **Step 4: Implement the `ConfigError` variants** (`lib.rs`, near `RdsRouteConfigNotFound`):

```rust
    #[error("cluster {cluster:?}: a non-EDS cluster requires `load_assignment`")]
    MissingLoadAssignment { cluster: String },
    #[error("cluster {cluster:?}: a `type: EDS` cluster requires `eds_cluster_config`")]
    MissingEdsClusterConfig { cluster: String },
    #[error("cluster {cluster:?}: `eds_cluster_config` set on a non-EDS cluster")]
    EdsConfigOnNonEdsCluster { cluster: String },
    #[error("cluster {cluster:?}: a `type: EDS` cluster must not carry an inline `load_assignment`")]
    LoadAssignmentOnEdsCluster { cluster: String },
    #[error("EDS file error reading {path:?}: {source}")]
    EdsFileError { path: String, source: std::io::Error },
    #[error("EDS file parse error in {path:?}: {message}")]
    EdsParseError { path: String, message: String },
    #[error("EDS ClusterLoadAssignment {name:?} not found in {path:?}")]
    EdsClusterLoadAssignmentNotFound { name: String, path: String },
```

- [ ] **Step 5: Implement the exactly-one-of-and-consistent check + the `Option` guards** in `validate_cluster` (`bootstrap.rs:2261-2278`). Add the cardinality/consistency check FIRST (runs at parse AND post-merge — harmless post-merge since an EDS cluster then carries `load_assignment: Some` AND `eds_cluster_config: Some`, which the check tolerates), then guard the existing endpoint checks on the `Option`, carving out the EDS name-mismatch:

```rust
pub(crate) fn validate_cluster(cluster: &Cluster) -> Result<(), crate::ConfigError> {
    // 21 D1 (§5.8; L6): exactly-one-of-and-consistent endpoint source.
    // At PARSE time an EDS cluster has load_assignment: None + eds_cluster_config:
    // Some (caught by the `is_eds && la_some` arm only if a STRAY inline la was
    // also given). Post-merge the EDS cluster has BOTH Some (the loaded state) —
    // the (true, _, true) EDS arm tolerates it (no AmbiguousSource error).
    let is_eds = cluster.cluster_type == ClusterType::Eds;
    let la_some = cluster.load_assignment.is_some();
    let eds_some = cluster.eds_cluster_config.is_some();
    match (is_eds, la_some, eds_some) {
        (true, _, false) => return Err(crate::ConfigError::MissingEdsClusterConfig {
            cluster: cluster.name.clone(),
        }),
        // an EDS cluster with an inline load_assignment at PARSE time is stricter-
        // rejected (L6 6a — Envoy accepts-and-ignores; we reject). After the merge
        // populates load_assignment this fn is re-run, but the merge NEVER sets a
        // *pre-existing* inline la — it sets the loaded one — so distinguish:
        // the parse-time reject uses the `eds_cluster_config && la` BEFORE the merge.
        // Implementation: gate the LoadAssignmentOnEdsCluster reject on a parse-only
        // flag (see note below).
        (false, false, _) => return Err(crate::ConfigError::MissingLoadAssignment {
            cluster: cluster.name.clone(),
        }),
        (false, _, true) => return Err(crate::ConfigError::EdsConfigOnNonEdsCluster {
            cluster: cluster.name.clone(),
        }),
        _ => {}
    }
    // 21 D1: endpoint checks run only when load_assignment is present (an EDS
    // cluster pre-merge has None — its endpoints are validated post-merge).
    let Some(la) = cluster.load_assignment.as_ref() else {
        return Ok(()); // EDS cluster, pre-merge — nothing inline to validate yet
    };
    // The name-mismatch check applies to INLINE (non-EDS) clusters only: an EDS
    // cluster's populated CLA cluster_name equals service_name (L8), not the
    // cluster name — re-checking against cluster.name would falsely reject.
    if !is_eds && la.cluster_name != cluster.name {
        return Err(crate::ConfigError::LoadAssignmentNameMismatch {
            cluster: cluster.name.clone(),
            assignment: la.cluster_name.clone(),
        });
    }
    let total_endpoints: usize =
        la.endpoints.iter().map(|le| le.lb_endpoints.len()).sum();
    if total_endpoints == 0 {
        return Err(crate::ConfigError::EmptyClusterEndpoints(cluster.name.clone()));
    }
    Ok(())
}
```

  > **NOTE on the `LoadAssignmentOnEdsCluster` parse-time reject (L6 6a):** the merge (Task 3) populates an EDS cluster's `load_assignment`, after which `validate_cluster` is re-run post-merge and would see `(true, true, true)`. To reject an inline-`load_assignment`-on-EDS at PARSE time WITHOUT false-positiving the post-merge loaded state, do the `LoadAssignmentOnEdsCluster` check **at parse time only**, in a `parse_bootstrap`-level pass (a `check_endpoint_sources(&Bootstrap)` helper that runs over the static listeners' clusters before any file is read — mirroring phase-20's `check_route_sources`), NOT inside `validate_cluster`. `validate_cluster`'s `(true, true, _)` case is then treated as the valid loaded state. Keep `MissingEdsClusterConfig`/`MissingLoadAssignment`/`EdsConfigOnNonEdsCluster` in `validate_cluster` (those never false-positive: the merge never adds an `eds_cluster_config` or removes a `load_assignment`). **Tests (e) must drive the parse-time path** (`parse_bootstrap`, not `validate_cluster` directly).
- [ ] **Step 6: Implement the `parse_bootstrap` `check_endpoint_sources` pass** (mirrors phase-20's `check_route_sources`): walk `bootstrap.static_resources.clusters`; for each `cluster_type == Eds` cluster carrying an inline `load_assignment` → `Err(LoadAssignmentOnEdsCluster { cluster })`. Place it as `fn check_endpoint_sources(bootstrap: &Bootstrap) -> Result<(), ConfigError>` so Task 3 can re-call it over the merged (static + CDS) cluster set (a CDS-supplied cluster could itself be a malformed EDS+`load_assignment`).
- [ ] **Step 7: Implement the cluster-build `Eds` arm** (`crates/envoy-cluster/src/cluster.rs:735`). Unwrap the now-`Option` `load_assignment` and add `Eds` to the `Static` arm (EDS endpoints are numeric — L1):

```rust
    // 21 D1 (§5.3): every cluster has load_assignment: Some after load_dynamic_resources
    // (inline from parse; EDS populated by the merge). The expect is the structural witness.
    let load_assignment = cfg
        .load_assignment
        .as_ref()
        .expect("load_assignment populated post-load — §5.3 invariant");
    let mut endpoints: Vec<SocketAddr> = Vec::new();
    for locality in &load_assignment.endpoints {
        for lbe in &locality.lb_endpoints {
            let sa = &lbe.endpoint.address.socket_address;
            match cfg.cluster_type {
                // 21 D1 (L1): EDS endpoints are resolved numeric socket addresses,
                // parsed exactly like STATIC (NOT DNS-resolved like STRICT_DNS).
                envoy_config::ClusterType::Static | envoy_config::ClusterType::Eds => {
                    // ... existing Static SocketAddr::from_str branch verbatim ...
                }
                envoy_config::ClusterType::StrictDns => {
                    // ... existing StrictDns lookup_host branch verbatim ...
                }
            }
        }
    }
```

- [ ] **Step 8: The D1 migration sweep — fix ALL Rust `Cluster {}` literal sites + the read sites** (same commit). For each of the ~17 Rust `Cluster {…}` literals (build the workspace to find them all; concentrated in `envoy-cluster/src/cluster.rs`, `envoy-admin/src/endpoint.rs`, plus single sites in `envoy-http1`/`envoy-http2`/`envoy-health`/`envoy-tcp`/`envoy-bin` tests): change `load_assignment: <expr>,` → `load_assignment: Some(<expr>), eds_cluster_config: None,`. For the production reads: `cluster.rs:735` handled in Step 7; `bootstrap.rs:2262/2265/2269` handled in Step 5 (now reads through `la`). For the ~12 `bootstrap.rs` test reads (`.load_assignment.cluster_name` / `.load_assignment.endpoints` assertions on parsed inline clusters): `.load_assignment.as_ref().unwrap().cluster_name` etc. (they assert on inline STATIC clusters, which are `Some`).
- [ ] **Step 9: Run, verify pass + the whole workspace compiles.** Run: `cargo test -p envoy-config && cargo build --workspace --all-targets`. Expected: PASS (the migration sweep restores the workspace build; inline-`load_assignment` fixtures parse identically).
- [ ] **Step 10: clippy + fmt + standalone builds + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p envoy-config && cargo build -p envoy-cluster && cargo build -p envoy-http1 && cargo build -p envoy-http2
git add crates/
git commit -m "phase 21 Task 1: ClusterType::Eds + eds_cluster_config + load_assignment->Option + exactly-one-of validator + Eds build arm + D1 sweep [ADR-0053, ADR-0054]"
```

---

### Task 2: `envoy-config` — the EDS file parser (`eds.rs`)

**Files:**
- Create: `crates/envoy-config/src/eds.rs`
- Modify: `crates/envoy-config/src/lib.rs` (`pub mod eds;`)
- Test: `crates/envoy-config/src/eds.rs` test module

- [ ] **Step 1: Write failing tests.** (a) **bare `resources:` envelope (L1):** a YAML with `resources:` listing one `@type`-tagged `ClusterLoadAssignment` `eds_backend` (one endpoint, numeric IP) → `parse_eds_file("/x.yaml", &s)` returns `Ok(vec![la])` with `la.cluster_name == "eds_backend"`, one endpoint. (b) **full `DiscoveryResponse` envelope (L1):** the same wrapped with `version_info: v1` + `resources:` → also `Ok(vec![la])` (version ignored). (c) **multiple ClusterLoadAssignments:** a file with two CLAs → `Ok(vec![la1, la2])` (name-selection is Task 3, not here). (d) **non-ClusterLoadAssignment `@type` rejected:** a resource tagged `...v3.Cluster` → `Err(EdsParseError)` (the serde `@type` tag rejects). (e) **malformed YAML → `EdsParseError`** (L4). (f) **missing `@type` → `EdsParseError`** (L1 — Envoy `update_failure`; envoy-rust fatal-at-parse).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config eds_parse`. Expected: FAIL.
- [ ] **Step 3: Implement `eds.rs`** (mirror `cds.rs` — the C18 decision: a new sibling module, NOT the deferred `xds_file.rs` consolidation). The per-resource payload is the existing `LoadAssignment` struct (envoy-rust's name for `ClusterLoadAssignment`):

```rust
//! 21 D2 (ADR-0053/0054): the EDS file parser. Mirrors cds.rs/lds.rs/rds.rs — the
//! @type-tagged envelope with ClusterLoadAssignment resources (envoy-rust parses
//! these into the existing `LoadAssignment` struct, reused verbatim). Always-YAML
//! (serde_yaml, regardless of extension — the ADR-0049 decision-1 posture; the
//! Envoy-side container path is structurally .yaml). The named-resource selection
//! (service_name-or-cluster-name) happens at merge time (lib.rs), not here.
//! M19-1/M20-T6-a (the xds_file.rs consolidation, now N=4) DEFERRED per PLAN C18.
use crate::bootstrap::LoadAssignment;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "@type")]
enum EdsResource {
    #[serde(rename = "type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment")]
    ClusterLoadAssignment(LoadAssignment),
}

#[derive(Debug, Deserialize)]
struct EdsFile {
    #[serde(default)]
    #[allow(dead_code)]
    version_info: Option<String>, // accepted-and-ignored (L1)
    resources: Vec<EdsResource>,
}

pub fn parse_eds_file(
    path: &str,
    contents: &str,
) -> Result<Vec<LoadAssignment>, crate::ConfigError> {
    let file: EdsFile =
        serde_yaml::from_str(contents).map_err(|e| crate::ConfigError::EdsParseError {
            path: path.to_string(),
            message: e.to_string(),
        })?;
    Ok(file
        .resources
        .into_iter()
        .map(|EdsResource::ClusterLoadAssignment(la)| la)
        .collect())
}
```

  (Verify the `EdsFile` `version_info` handling matches `CdsFile`/`LdsFile`/`RdsFile` exactly. Add `pub mod eds;` to `lib.rs` beside `pub mod rds;`.)
- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-config eds_parse`. Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-config/
git commit -m "phase 21 Task 2: EDS file parser (eds.rs) [ADR-0053]"
```

---

### Task 3: `load_dynamic_resources` EDS pass — cluster walk + name-selection + effective-`load_assignment` population + the §5.7 ordering + the validate-gate extension

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (`load_dynamic_resources` `:623-764`)
- Test: `crates/envoy-config/src/lib.rs` test module (use `tempfile` — the existing dev-dep)

- [ ] **Step 1: Write failing tests** (use `tempfile` for EDS/CDS files; numeric `127.0.0.1` endpoints). (a) **the EDS pass loads + populates:** a bootstrap with a STATIC listener (inline route `/` → `eds_backend`) + a cluster `eds_backend` `type: EDS` + `eds_cluster_config` pointing at a temp EDS file (CLA `eds_backend`, one endpoint `127.0.0.1:9001`) → `load_dynamic_resources` succeeds; the cluster's `load_assignment == Some(la)` with `la.cluster_name == "eds_backend"`, one endpoint; `eds_cluster_config` still `Some`. (b) **`service_name` selection (L8):** `eds_cluster_config.service_name: my_svc` + the EDS file CLA named `my_svc` (NOT `eds_backend`) → populates from the `my_svc` CLA; succeeds. (c) **missing EDS file is fatal (L4):** `EdsFileError`. (d) **malformed EDS file is fatal (L4):** `EdsParseError`. (e) **missing/mismatched CLA is fatal (L4/L8):** the EDS file defines `other`, the cluster wants `eds_backend` → `EdsClusterLoadAssignmentNotFound { name: "eds_backend", .. }`. (f) **empty CLA endpoints is fatal (L4 (d)):** the CLA has zero endpoints → post-merge `EmptyClusterEndpoints`. (g) **the §5.7 ordering — CDS cluster is composition-ready:** a bootstrap with `cds_config` (temp CDS file → a STATIC cluster `dynamic_backend`) + a static EDS cluster → `load_dynamic_resources` succeeds (the EDS pass walks the full static+CDS set; `dynamic_backend` is untouched, `eds_backend` populated). (h) **the validate-gate fires for a static-only EDS bootstrap (C16):** a bootstrap with NO `dynamic_resources`/`rds` but a static EDS cluster whose CLA has a bad shape (empty endpoints) → the post-merge validate STILL runs and rejects (proving the gate extension). (i) **an EDS cluster with an inline `load_assignment` is rejected at parse (L6 6a):** `parse_bootstrap` (NOT `load_dynamic_resources`) → `LoadAssignmentOnEdsCluster` (the Task-1 Step-6 `check_endpoint_sources`).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config load_dynamic_eds`. Expected: FAIL.
- [ ] **Step 3: Implement the EDS pass** in `load_dynamic_resources`, AFTER the RDS pass (`:749`) and BEFORE the post-merge `validate()` (`:751`):

```rust
    // ---- EDS pass (phase 21, ADR-0053/0054; §6.2 L1/L4/L8; §5.7) ----
    // Walk every cluster across the EFFECTIVE set (static + CDS-merged dynamic);
    // for each `type: EDS` cluster, read its file, name-select the
    // ClusterLoadAssignment by service_name-or-cluster-name, and populate the
    // effective load_assignment. Runs AFTER the CDS merge so a CDS-supplied
    // cluster that is ALSO type: EDS gets its endpoints loaded (composition-ready,
    // §4 defers the bilateral fixture); the post-merge validate() below
    // re-validates the populated endpoints. NO dynamic_resources block is required
    // — a purely-static EDS cluster (fixture 0029) triggers this pass too (C16).
    let mut had_eds_cluster = false;
    let (static_clusters, dynamic_clusters) = (
        &mut bootstrap.static_resources.clusters,
        &mut bootstrap.dynamic_clusters,
    );
    for cluster in static_clusters
        .iter_mut()
        .chain(dynamic_clusters.iter_mut().flatten())
    {
        if cluster.cluster_type != ClusterType::Eds {
            continue;
        }
        had_eds_cluster = true;
        let eds = cluster
            .eds_cluster_config
            .as_ref()
            .expect("EDS cluster has eds_cluster_config — validated at parse");
        let path = eds.eds_config.path_config_source.path.clone();
        let select_name = eds
            .service_name
            .clone()
            .unwrap_or_else(|| cluster.name.clone()); // L8
        let contents = std::fs::read_to_string(&path)
            .map_err(|source| ConfigError::EdsFileError { path: path.clone(), source })?;
        let mut parsed = eds::parse_eds_file(&path, &contents)?;
        let selected = parsed
            .iter()
            .position(|la| la.cluster_name == select_name)
            .map(|i| parsed.remove(i))
            .ok_or(ConfigError::EdsClusterLoadAssignmentNotFound {
                name: select_name,
                path,
            })?;
        cluster.load_assignment = Some(selected); // §5.3: uniform downstream shape
    }
    // ---- §5.7: ONE post-merge re-validation after the CDS + LDS + RDS + EDS merges ----
    if bootstrap.dynamic_clusters.is_some()
        || bootstrap.dynamic_listeners.is_some()
        || had_rds_hcm
        || had_eds_cluster
    {
        bootstrap::validate(bootstrap)?;
    }
    Ok(())
```

  (The split-borrow of `bootstrap.static_resources.clusters` + `bootstrap.dynamic_clusters` must end before `validate(bootstrap)`; collect `had_eds_cluster` as a `bool`, then drop the borrow. Extend the existing validate gate — currently `dynamic_clusters || dynamic_listeners || had_rds_hcm` — with `|| had_eds_cluster`. Confirm `ClusterType`/`ConfigError`/`eds` are in scope.)
- [ ] **Step 4: Re-run `check_endpoint_sources` over the merged cluster set.** A CDS-supplied cluster could itself be a malformed `type: EDS` + inline `load_assignment` — call the Task-1 `check_endpoint_sources` helper over the effective static+CDS cluster set inside `load_dynamic_resources` BEFORE the EDS pass populates anything (so a CDS cluster's bad endpoint-source state fails). Add a test: a CDS file whose cluster is `type: EDS` with an inline `load_assignment` → `LoadAssignmentOnEdsCluster`.
- [ ] **Step 5: Run, verify pass.** Run: `cargo test -p envoy-config && cargo build --workspace --all-targets`. Expected: PASS.
- [ ] **Step 6: clippy + fmt + standalone builds + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p envoy-config && cargo build -p envoy-cluster && cargo build -p envoy-http1 && cargo build -p envoy-http2
git add crates/envoy-config/
git commit -m "phase 21 Task 3: load_dynamic_resources EDS pass + §5.7 ordering + effective-load_assignment population + validate-gate extension [ADR-0053, ADR-0054]"
```

---

### Task 4: Per-cluster `cluster.<name>.update_*` EDS stats (conditional registration at the existing per-cluster site)

**Files:**
- Modify: `crates/envoy-cluster/src/cluster.rs` (the per-cluster stat registration `:817-947`)
- Test: `crates/envoy-cluster/src/cluster.rs` test module

> **Simplification (L3):** all 4 stat values are deterministic at a successful initial load (`update_attempt`/`update_success` = 1; `update_failure`/`update_empty` = 0 — the all-fatal posture makes non-zero structurally unreachable). So the registration register-and-sets directly when `cfg.cluster_type == Eds`, exactly like the CDS template at `:1068-1097` — NO handle threading to a separate increment site. **This task registers ONLY the 4 `update_*` counters — NOT `membership_healthy`/`membership_total`** (`membership_healthy` is health-check-gated at `cluster.rs:926` and `membership_total` does not exist; fixture 0029 has no health checks → those are NOT in the asserted subset, a recorded narrowing vs Envoy; do NOT broaden their registration — that would break the existing `:2227` inertness test). The data-plane witness `cluster.<name>.upstream_rq_total` already exists unconditionally (`cluster.rs:106`).

- [ ] **Step 1: Write failing tests.** (a) **conditional registration (the §5.2 inertness invariant):** building a `ClusterManager::from_bootstrap` over a bootstrap whose clusters are all STATIC/STRICT_DNS (incl. one with `cds_config` — the fixture-0026 inertness witness) → NO stat whose name matches `cluster.<name>.update_*` exists in the registry afterward. (b) **the 4-name subset on an EDS cluster:** a bootstrap with a `type: EDS` cluster `eds_backend` (load_assignment populated, e.g. by a direct construction or a pre-populated bootstrap) → `cluster.eds_backend.update_attempt == 1`, `.update_success == 1`, `.update_failure == 0`, `.update_empty == 0`. (c) **NO membership gauges registered by this task:** assert that registering an EDS cluster (no health checks) does NOT create `cluster.eds_backend.membership_total` (absent) and does NOT register `cluster.eds_backend.membership_healthy` (HC-gated) — i.e. this task adds ONLY the 4 `update_*` names. (Construct the bootstrap with the EDS cluster's `load_assignment` already `Some` — the registration keys on `cfg.cluster_type == Eds`, independent of how it was populated.)
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-cluster eds_stats`. Expected: FAIL.
- [ ] **Step 3: Implement the conditional EDS `update_*` registration** at the per-cluster stat site (`cluster.rs:817-947`, where `cluster.<name>.*` is built). After the existing per-cluster counters/gauges, add:

```rust
        // 21 D4 (ADR-0053/0054; §6.2 L3/L10): the per-cluster EDS update_* family
        // — registered ONLY for `type: EDS` clusters (the §5.2 conditional-
        // registration discipline; STATIC/STRICT_DNS clusters emit no update_*).
        // All values deterministic at a successful initial load (the all-fatal
        // posture makes update_failure/update_empty structurally 0 — L4), so
        // register-and-set directly (no handle threading). membership_healthy/
        // membership_total already register elsewhere and are EDS-driven.
        if cfg.cluster_type == envoy_config::ClusterType::Eds {
            let mk = |suffix: &str| -> Result<Arc<envoy_stats::Counter>, ClusterError> {
                registry
                    .register_counter(&format!("cluster.{}.{suffix}", cfg.name))
                    .map_err(|e| ClusterError::StatsRegistration {
                        cluster: cfg.name.clone(),
                        message: e.to_string(),
                    })
            };
            mk("update_attempt")?.add(1);
            mk("update_success")?.add(1);
            mk("update_failure")?; // registers at 0 (L4)
            mk("update_empty")?; // registers at 0 (L4)
        }
```

  (Verify `Arc`/`envoy_stats::Counter`/`register_counter`/`ClusterError::StatsRegistration` are the exact in-scope types/variants used by the surrounding registrations. Place it where `cfg` + `registry` + `cfg.name` are in scope, near the `membership_healthy` registration.)
- [ ] **Step 4: Confirm NO main.rs call site change.** The per-cluster registration is inside `from_bootstrap` (already called at startup), so unlike `register_rds_stats` this needs NO new `main.rs` call. Verify by building.
- [ ] **Step 5: Run, verify pass.** Run: `cargo test -p envoy-cluster && cargo build --workspace --all-targets`. Expected: PASS.
- [ ] **Step 6: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-cluster/
git commit -m "phase 21 Task 4: conditional per-cluster cluster.<name>.update_* EDS stat family [ADR-0053, ADR-0054]"
```

---

### Task 5: `/config_dump` `EndpointsConfigDump` entry (`static_endpoint_configs`, conditional emission, after Clusters) + the admin query-string strip

**Files:**
- Modify: `crates/envoy-admin/src/endpoint.rs` (`ConfigDumpEntry` enum `:303-350`; the path dispatch `:97-101`; the render-ordering `:500-607`; the ~4 test `Cluster {}` literals — already swept in Task 1)
- Test: `crates/envoy-admin/src/endpoint.rs` test module

> **L5 lock-in:** Envoy OMITS `EndpointsConfigDump` from the default `/config_dump` (only `?include_eds` surfaces it) and uses `static_endpoint_configs[].endpoint_config` (file-based EDS is "static" config-dump-wise). envoy-rust emits it CONDITIONALLY (when some cluster is EDS) using `static_endpoint_configs[].endpoint_config`, UNCONDITIONAL of the query (a recorded narrowing), pushed after Clusters / before Listeners; the admin strips the query string so `/config_dump?include_eds` routes to `ConfigDump`. On fixture 0029 (no cds/lds/rds) envoy-rust lands it at `configs[1]`; Envoy `?include_eds` has it at `configs[2]` (reconciled per-side in Task 6/7).

- [ ] **Step 1: Write failing tests.** (a) **query-strip:** `AdminEndpoint::dispatch("GET", "/config_dump?include_eds")` resolves to `ConfigDump` (not 404). (b) **conditional emission:** a handler whose bootstrap has a `type: EDS` cluster (load_assignment populated) → `/config_dump` `configs[]` contains an entry with `@type` ending `EndpointsConfigDump` whose `static_endpoint_configs[0].endpoint_config.cluster_name == eds_backend`. (c) **inertness:** a handler with only STATIC/STRICT_DNS clusters (no EDS) → NO `EndpointsConfigDump` entry (fixtures 0014/0026/0027/0028 untouched). (d) **ordering:** on a bootstrap with `cds_config` + an EDS cluster → the Endpoints entry is at `configs[2]` (Bootstrap[0], Clusters[1], Endpoints[2]); on a bootstrap with NO cds + an EDS cluster → at `configs[1]`.
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-admin config_dump_endpoints`. Expected: FAIL.
- [ ] **Step 3: Implement the query-string strip** in the path dispatch (`:97`):

```rust
    // 21 D5: strip the query string so /config_dump?include_eds routes to ConfigDump
    // (Envoy's admin does the same; surfaces the EndpointsConfigDump bilaterally — L5).
    let path = path.split('?').next().unwrap_or(path);
    match path {
        // ... existing arms unchanged ...
```

- [ ] **Step 4: Implement the enum variant** (`ConfigDumpEntry`, after `Clusters`):

```rust
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.EndpointsConfigDump")]
    Endpoints {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        static_endpoint_configs: Vec<StaticEndpointConfigEntry<'a>>,
    },
```

  with the supporting structs (mirroring `DynamicClusterEntry`; the `endpoint_config` carries its own `@type` + the `ClusterLoadAssignment` body — reuse the `LoadAssignment` serialize shape):

```rust
#[derive(Serialize)]
struct StaticEndpointConfigEntry<'a> {
    endpoint_config: ClusterLoadAssignmentBody<'a>,
}
#[derive(Serialize)]
struct ClusterLoadAssignmentBody<'a> {
    #[serde(rename = "@type")]
    type_url: &'static str, // "type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment"
    cluster_name: &'a str,
    endpoints: &'a Vec<envoy_config::LocalityLbEndpoints>,
}
```

  (If `LoadAssignment` already `Serialize`s in the shape Envoy emits, prefer flattening `&'a LoadAssignment` with the `@type` injected — match whatever the `Clusters`/`Listeners` entries do for their inner bodies.)
- [ ] **Step 5: Implement the conditional push** (render-ordering, AFTER the `Clusters` block `:537` and BEFORE the `Listeners` block `:580`):

```rust
    // 21 D5 (ADR-0053/0054; §6.2 L5): EndpointsConfigDump — emitted ONLY when some
    // cluster is `type: EDS` (conditional emission; fixtures 0014/0026/0027/0028
    // untouched). Uses static_endpoint_configs (file-based EDS is "static" config-
    // dump-wise — L5); pushed after Clusters / before Listeners (Envoy's
    // ?include_eds order Clusters[1]/Endpoints[2]/Listeners[3]). envoy-rust emits
    // it unconditional of ?include_eds (a recorded narrowing); the differential
    // index mismatch is reconciled per-side in the harness.
    let static_endpoint_configs: Vec<StaticEndpointConfigEntry> = bootstrap
        .all_clusters() // or the static+dynamic cluster accessor used here
        .filter(|c| c.cluster_type == envoy_config::ClusterType::Eds)
        .filter_map(|c| c.load_assignment.as_ref().map(|la| StaticEndpointConfigEntry {
            endpoint_config: ClusterLoadAssignmentBody {
                type_url: "type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment",
                cluster_name: &la.cluster_name,
                endpoints: &la.endpoints,
            },
        }))
        .collect();
    if !static_endpoint_configs.is_empty() {
        configs.push(ConfigDumpEntry::Endpoints { static_endpoint_configs });
    }
```

  (Verify the cluster accessor used here matches the enclosing function's binding — the existing `Clusters` block shows the exact accessor + lifetime shape; if there is no `all_clusters()` accessor, walk `bootstrap.static_resources.clusters` + `bootstrap.dynamic_clusters` like the EDS pass does.)
- [ ] **Step 6: Run, verify pass.** Run: `cargo test -p envoy-admin && cargo build --workspace --all-targets`. Expected: PASS.
- [ ] **Step 7: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-admin/
git commit -m "phase 21 Task 5: /config_dump EndpointsConfigDump entry (static_endpoint_configs) + query-string strip [ADR-0053, ADR-0054]"
```

---

### Task 6: Harness — `{{EDS_PATH}}` shared-template rendering/mounting + the `{{EDS_BACKEND_IP}}` per-side numeric marker + host-gateway-IP discovery

**Files:**
- Modify: `tests/differential/src/lib.rs` (the dynamic-file machinery `:2227-2303`; the kv-map substitution `:2509-2605`; the backend/host-gateway scans `:968`/the scan-source assembly; the per-side `JsonSubtreeRule` `:613-640` is REUSED from phase 20 — no change)
- Test: `tests/differential/src/lib.rs` test module (unit test for the gateway-IP discovery + the EDS-marker scan; the fixture is Task 7)

> **L9 lock-in:** the EDS file is a SHARED template (`eds.yaml`) rendered per-side, but the backend `socket_address.address` must be a NUMERIC IP that differs per side (Envoy → the host-gateway numeric IP; envoy-rust → `127.0.0.1`). So a NEW `{{EDS_BACKEND_IP}}` kv marker is added (NOT the shared-string `{{BACKEND_HOST}}` other fixtures use — EDS rejects hostnames, L1), and the harness DISCOVERS the numeric host-gateway IP at runtime. The per-side `JsonSubtreeRule` already exists (ADR-0052) — reused for the config_dump index reconciliation, no new code.

- [ ] **Step 1: Write failing tests.** (a) **gateway-IP discovery returns a numeric IP:** the discovery helper returns an `Ipv4Addr`-parseable string (run the one-shot container; assert the result parses as a numeric IPv4 — skip/ignore the test if Docker is unavailable, mirroring the existing Docker-gated test discipline). (b) **EDS marker scan:** a fixture template containing `{{EDS_PATH}}` sets `needs_eds == true`; one without it sets `false` (the inertness gate — fixtures 0001-0028 unaffected).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p differential eds_marker`. Expected: FAIL.
- [ ] **Step 3: Implement the host-gateway-IP discovery** (a `fn discover_host_gateway_ip() -> Result<String>` — gated to EDS fixtures, run once when `needs_eds`):

```rust
/// 21 D6 (ADR-0054; §6.2 L9): discover the NUMERIC host-gateway IP the Envoy
/// container uses to reach the host backend. EDS rejects hostnames (L1), so the
/// EDS file's endpoint address must be numeric — and it varies by platform
/// (192.168.65.254 on macOS Docker Desktop; the bridge gateway e.g. 172.17.0.1
/// on Linux CI). Resolve it portably by running getent inside a throwaway
/// container with the host-gateway mapping (the pinned Envoy image is Ubuntu-based
/// and ships getent; NO new image dependency). The bridge-network-inspect shortcut
/// is WRONG on macOS, so getent-in-container is the only cross-platform method.
fn discover_host_gateway_ip() -> anyhow::Result<String> {
    let out = std::process::Command::new("docker")
        .args([
            "run", "--rm",
            "--add-host=host.docker.internal:host-gateway",
            "--entrypoint", "getent",
            "envoyproxy/envoy:v1.33.0", // the ENVOY_TARGET.md pin
            "hosts", "host.docker.internal",
        ])
        .output()
        .context("running getent to discover the host-gateway IP")?;
    let line = String::from_utf8_lossy(&out.stdout);
    let ip = line
        .split_whitespace()
        .next()
        .filter(|s| s.parse::<std::net::Ipv4Addr>().is_ok())
        .ok_or_else(|| anyhow::anyhow!("getent did not return a numeric host-gateway IP: {line:?}"))?
        .to_string();
    Ok(ip)
}
```

  (Pin the image tag to the `ENVOY_TARGET.md` value — confirm it is `v1.33.0`. Cache the result if multiple EDS fixtures run, or accept the per-fixture cost — only fixture 0029 uses it today.)
- [ ] **Step 4: Implement `{{EDS_PATH}}` rendering/mounting** (mirror the SHARED-`cds.yaml` path at `:2227-2240`, NOT the per-side LDS path — L9: one shared `eds.yaml` rendered per-side via the kv map):

```rust
    let needs_eds =
        upstream_template.contains("{{EDS_PATH}}") || subject_template.contains("{{EDS_PATH}}");
    let eds_template = if needs_eds {
        Some(
            std::fs::read_to_string(fixture_dir.join("eds.yaml"))
                .context("reading eds.yaml (fixture references {{EDS_PATH}})")?,
        )
    } else {
        None
    };
    let subject_eds_path = tmp.path().join("eds-subject.yaml");
    let subject_eds_path_str = subject_eds_path.to_string_lossy().into_owned();
    let host_gateway_ip = if needs_eds { Some(discover_host_gateway_ip()?) } else { None };
```

  Add `EDS_CONTAINER_PATH` (a `.yaml`-ending constant, mirroring `CDS_CONTAINER_PATH`). In the per-side kv maps: upstream → `EDS_PATH` = `EDS_CONTAINER_PATH`, `EDS_BACKEND_IP` = the discovered `host_gateway_ip`; subject → `EDS_PATH` = `subject_eds_path_str`, `EDS_BACKEND_IP` = `"127.0.0.1"`. Render the shared `eds.yaml` per-side through each side's kv map (so `{{EDS_BACKEND_IP}}` + `{{HTTP1_BACKEND_PORT}}` resolve per-side), write the subject rendition to `subject_eds_path`, mount the upstream rendition into the Envoy container (`with_copy_to` to `EDS_CONTAINER_PATH`). **Add the EDS rendition to the `scan_needs_marker` backend-detection sources AND the `uses_host_gateway` scan sources** (the phase-18 carryforward-disposition-2 bug-class lesson: scan ALL rendered sources — fixture 0029's backend lives ONLY in the EDS file; note the EDS rendition uses the NUMERIC gateway IP, not the `host.docker.internal` string, so `uses_host_gateway` must still fire because the MAIN config or the `--add-host` wiring needs it — confirm the host-gateway mapping is still applied via the existing `BACKEND_PORT`/`HTTP1_BACKEND_PORT` gate, since fixture 0029 also reserves an `HTTP1_BACKEND_PORT` for the echo backend).
- [ ] **Step 5: Run, verify pass.** Run: `cargo test -p differential --lib`. Expected: PASS (the existing fixtures' machinery unchanged; the new EDS-marker + discovery tests green).
- [ ] **Step 6: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add tests/differential/
git commit -m "phase 21 Task 6: harness {{EDS_PATH}} shared-template + {{EDS_BACKEND_IP}} per-side numeric marker + host-gateway-IP discovery [ADR-0054]"
```

---

### Task 7: Fixture `0029-xds-file-based-eds` + Docker-gated wrapper

**Files:**
- Create: `tests/fixtures/0029-xds-file-based-eds/{envoy.yaml, envoy-rust.yaml, eds.yaml, expectations.yaml, README.md}`
- Create: `tests/differential/tests/xds_file_based_eds.rs`

- [ ] **Step 1: Author the fixture configs.** Both sides: `admin` + `node: { id, cluster }` + ONE static listener (H1 HCM, `stat_prefix: ingress_http`, INLINE `route_config` `local_route` routing `/` → `eds_backend`, `http_filters` ending in router) + ONE static cluster `eds_backend` (`type: EDS` + `eds_cluster_config: { eds_config: { path_config_source: { path: {{EDS_PATH}} } } }`, NO inline `load_assignment`, `connect_timeout`, `lb_policy: ROUND_ROBIN`). NO `dynamic_resources` (the cluster is static-but-EDS). The Envoy side (`envoy.yaml`) may carry the established Envoy-only main-config fields (per the 0008/0026/0027/0028 per-side main-config precedent — `generate_request_id: false`, header_mutation/XFF strip, router `suppress_envoy_headers` + the `x-envoy-upstream-service-time` re-add) for byte-exact echo bodies; envoy-rust emits these natively. Shared `eds.yaml` (rendered per-side via the kv map — L9): a `resources:` envelope with one `@type`-tagged `ClusterLoadAssignment` `eds_backend`, one locality, one `lb_endpoint` → `address: {{EDS_BACKEND_IP}}, port_value: {{HTTP1_BACKEND_PORT}}` (the `http1-echo-server` helper). `README.md`: the topology + the L1–L11 lock-ins + the numeric-IP/per-side-marker rationale (L9) + the `?include_eds`/`static_endpoint_configs`/per-side-index config_dump rationale (L5) + the C19 BootstrapConfigDump-shows-populated-load_assignment note + the Envoy-only stat enumeration (L3, not asserted).
- [ ] **Step 2: Author `expectations.yaml`** (the §1 probe list; `Driver::Http1KeepAlive`, one GET):

```yaml
driver:
  kind: http1_keep_alive
  requests:
    - method: GET
      path: /
      host: eds_backend
      expected_status: 200
      expected_body: { kind: byte_exact, body: "<echo body for / — copy the 0028 echo shape>" }
      require_header_present: x-envoy-upstream-service-time
  settle_ms: 200
  expected_stats:
    # EDS load (L3 — the conditional per-cluster 4-name subset):
    - { name: cluster.eds_backend.update_attempt, value: 1 }
    - { name: cluster.eds_backend.update_success, value: 1 }
    - { name: cluster.eds_backend.update_failure, value: 0 }
    - { name: cluster.eds_backend.update_empty,   value: 0 }
    # Data-plane through the EDS-supplied endpoint (the discriminating witness):
    - { name: cluster.eds_backend.upstream_rq_total, value: 1 }
    # HCM downstream:
    - { name: http.ingress_http.downstream_rq_total, value: 1 }
    - { name: http.ingress_http.downstream_rq_2xx,   value: 1 }
    # NOTE: membership_healthy/membership_total are NOT asserted — envoy-rust gates
    # membership_healthy on health_checks (absent here) and has no membership_total
    # (L3 narrowing); Envoy emits both, allow-listed envoy-only.
  admin_scrapes:
    # /config_dump?include_eds (L5): EndpointsConfigDump at envoy configs[2]
    # (Clusters[1]/Endpoints[2]), envoy-rust configs[1] (no cds → no ClustersConfigDump).
    # static_endpoint_configs (file-based EDS is "static"); per-side path override.
    - path: /config_dump?include_eds
      expected_status: 200
      expected_content_type: application/json
      expected_body_rule:
        kind: json_shape
        required_keys: ["configs"]
        required_subtree:
          path_envoy: configs.2.static_endpoint_configs.0.endpoint_config.cluster_name
          path_envoy_rust: configs.1.static_endpoint_configs.0.endpoint_config.cluster_name
          expected: eds_backend
        value_may_differ_keys: ["configs"]
```

  (Copy the exact `expected_body` echo string from the working fixture 0028 — the `http1-echo-server` response shape is identical. Verify the H1 echo body byte-for-byte against a local run in Step 3. Do NOT assert `membership_healthy`/`membership_total` — envoy-rust does not emit them for a non-health-checked cluster, L3.)
- [ ] **Step 3: Author the Docker-gated wrapper** `tests/differential/tests/xds_file_based_eds.rs` (copy the `xds_file_based_rds.rs` wrapper shape). Then run it LOCALLY once: **pre-build `tests/helpers/*` first**, and **do NOT run the Docker suite concurrently with any cargo build** (per `project_flaky_access_log_fixture_0012`):

```bash
cargo build -p http1-echo-server   # pre-build the helper (cold-helper flake class)
cargo test -p differential --test xds_file_based_eds -- --nocapture
```
  Expected: PASS (probe 200 + stats + config_dump bilateral). If the echo body differs, fix the `expected_body` string (Step 2). If the gateway-IP discovery or the EDS-backend reachability fails on Envoy, re-check the `{{EDS_BACKEND_IP}}` substitution (Envoy side = numeric gateway IP) + the `--add-host` wiring.
- [ ] **Step 4: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add tests/fixtures/0029-xds-file-based-eds/ tests/differential/tests/xds_file_based_eds.rs
git commit -m "phase 21 Task 7: fixture 0029-xds-file-based-eds + Docker-gated wrapper [ADR-0053, ADR-0054]"
```

---

### Task 8: In-process backstop (happy path + 6 negative paths + inertness)

**Files:**
- Create: `crates/envoy-bin/tests/xds_file_based_eds.rs`

> **Reuse note (M18-9, now N≥5):** the backstop helper block (`reserve_port`, `wait_ready`, `http1_oneshot`, `spawn_envoy_bin`, `write_file`, the bootstrap builder) is copied from `crates/envoy-bin/tests/xds_file_based_rds.rs`. Record the duplication in the file header — the extract-a-test-support-crate item stays a future hardening task (PLAN C18 / phase-20 carryforward).

- [ ] **Step 1: Write the backstop** (copy the RDS backstop's helper block; adapt the bootstrap builder to emit a `type: EDS` cluster + a temp EDS file with a `127.0.0.1` endpoint pointing at an in-process backend). Cover:
  - **(i) happy path:** temp EDS file (CLA `eds_backend`, one endpoint → the in-process backend) → probe `GET /` → 200 + the backend body + the `cluster.eds_backend.update_*` 4-name subset present with the L3 values (1/1/0/0) + `cluster.eds_backend.upstream_rq_total == 1` + `/config_dump` contains an `EndpointsConfigDump` whose `static_endpoint_configs[0].endpoint_config.cluster_name == "eds_backend"` (do NOT assert membership gauges — L3 narrowing).
  - **(ii) missing EDS file → process exits non-zero** (`EdsFileError`) — the L4 agrees-with-Envoy class (missing FILE PATH is fatal on both).
  - **(iii) malformed EDS file → exits** (`EdsParseError`) — the L4 envoy-rust-diverges class.
  - **(iv) missing/mismatched ClusterLoadAssignment → exits** (`EdsClusterLoadAssignmentNotFound`) — L4/L8.
  - **(v) EDS cluster with an inline `load_assignment` → exits** (`LoadAssignmentOnEdsCluster`) — L6 6a (envoy-rust stricter than Envoy).
  - **(vi) STATIC cluster with `eds_cluster_config` → exits** (`EdsConfigOnNonEdsCluster`) — L6 6b.
  - **(vii) EDS cluster with neither → exits** (`MissingEdsClusterConfig`) — L6 6c.
  - **(viii) inertness witness:** a STATIC-only bootstrap (no EDS cluster) → `/config_dump` does NOT contain `"EndpointsConfigDump"` and `/stats` has no `cluster.<name>.update_*` name.
- [ ] **Step 2: Run, verify pass.** Pre-build the helper, then: `cargo test -p envoy-bin --test xds_file_based_eds`. Expected: PASS (8 cases).
- [ ] **Step 3: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-bin/tests/
git commit -m "phase 21 Task 8: in-process EDS backstop (happy + 6 negative + inertness) [ADR-0053, ADR-0054]"
```

---

### Task 9: Fuzz seed `cluster_eds.yaml` (corpus 31 → 32)

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_eds.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (allow-list the new seed)

> **Atomic-edit lesson (09→20):** the `.gitignore` allow-list line AND the seed file land in the SAME commit (the corpus-consistency discipline; keep it consistent).

- [ ] **Step 1: Author the seed** — a minimal bootstrap with a `type: EDS` cluster (`eds_cluster_config` pointing at a path; NO inline `load_assignment`). `parse_bootstrap` parses-and-validates the SCHEMA only — it NEVER reads the referenced EDS file (the file load is `load_dynamic_resources`, a separate entry point the fuzz target does not call); the exactly-one-of check passes (EDS + `eds_cluster_config`, no `load_assignment`); the EDS cluster's `load_assignment` is `None` at parse, so `validate_cluster` skips the endpoint checks (validated post-merge, which the fuzz target does not run).

```yaml
# 21 Task 9: fuzz seed for the EDS cluster schema surface (parse_bootstrap never
# reads the referenced EDS file — parse-and-validate only; the EDS endpoint
# population is load_dynamic_resources, a separate entry point). Mirrors
# dynamic_resources_cds/lds.yaml + hcm_rds_route_config.yaml.
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
node: { id: seed, cluster: seed }
static_resources:
  clusters:
    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
      connect_timeout: 1s
      eds_cluster_config:
        eds_config:
          path_config_source: { path: /etc/envoy-eds/eds.yaml }
  listeners:
    - name: l
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: AUTO
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: eds_backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
```

  (Match the exact `connect_timeout`/`lb_policy`/`codec_type`/router typed_config shape the other corpus seeds use — copy the scaffold from `dynamic_resources_cds.yaml` / `strict_dns_cluster.yaml`. Confirm `connect_timeout` is a valid `Cluster` field — if not, drop it.)
- [ ] **Step 2: Allow-list it** — add `!corpus/parse_bootstrap/cluster_eds.yaml` (matching the existing `.gitignore` allow-list convention).
- [ ] **Step 3: Verify it parses.** Add a quick `cargo test -p envoy-config` assertion that `parse_bootstrap` accepts the seed contents (or replay via the fuzz corpus). Expected: parses clean (the EDS cluster's `load_assignment: None` passes the parse-time validate; no file is read).
- [ ] **Step 4: commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_eds.yaml crates/envoy-config/fuzz/.gitignore
git commit -m "phase 21 Task 9: fuzz seed cluster_eds.yaml (corpus 31->32) [ADR-0053]"
```

---

### Task 10: BEHAVIOR_CONTRACT extensions (EDS stat rows + xDS-section EDS extension + EndpointsConfigDump row)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `Stat-name mapping` section; the `xDS wire state machine` section; the `Admin endpoint body shapes` section)

- [ ] **Step 1: Add the per-cluster EDS stat rows** to `Stat-name mapping` — the 4-name subset (`cluster.<name>.{update_attempt,update_success,update_failure,update_empty}`, value-exact 1/1/0/0 at initial load) + the data-plane witness `cluster.<name>.upstream_rq_total` (1), with the per-cluster-scoping note + the conditional-registration narrowing (EDS clusters only; STATIC/STRICT_DNS emit none in envoy-rust — vs Envoy emitting them at 0 for every cluster, L10) + **the membership-gauge narrowing** (envoy-rust does NOT emit `membership_healthy` [HC-gated] or `membership_total` [absent] for a non-health-checked EDS cluster; Envoy emits both for every cluster → allow-listed envoy-only, NOT broadened) + the Envoy-only enumeration (`update_no_rebuild`/`update_rejected`/`update_time`/`update_duration`/`membership_change`/`degraded`/`excluded`/`assignment_*`/`version`/`version_text`/`warming_state`, NOT asserted).
- [ ] **Step 2: Add the `### Filesystem transport (path_config_source) — phase 21 EDS extension` subsection** to `xDS wire state machine`, in the phase-18/19/20 §(a)–(f) parallel structure, recording L1–L11 from ADR-0054: (a) the EDS envelope (L1) + the numeric-IP constraint + the `eds_cluster_config`-on-cluster shape; (b) initial-load/readiness + warming-resolves-synchronously (L2); (c) the negative-path disposition (L4) — missing-file fatal on both, malformed→`update_failure`/missing-CLA→`update_rejected`/empty→`update_empty` warm-503 on Envoy + the envoy-rust all-fatal extension; (d) the exactly-one-of-and-consistent validation (L6) — 6b/6c fatal on both, 6a (EDS+`load_assignment`) Envoy-accepts/envoy-rust-stricter-rejects; (e) the `service_name`-or-cluster-name selection (L8); (f) the EndpointsConfigDump (L5) — Envoy's `?include_eds`-gating + `static_endpoint_configs` (file-based EDS is "static") + the per-side config_dump index reconciliation + envoy-rust's unconditional-when-EDS emission.
- [ ] **Step 3: Add the `EndpointsConfigDump` row** to `Admin endpoint body shapes` — the `static_endpoint_configs[].endpoint_config` shape (no `version_info`/`last_updated`), conditional emission (envoy-rust: when some cluster is EDS, unconditional of `?include_eds`; Envoy: only under `?include_eds`), the Envoy `configs[2]` (with `?include_eds`, after Clusters) / envoy-rust `configs[1]` (fixture 0029, no cds) index divergence + the per-side-path reconciliation (reusing the ADR-0052 mechanism), and the C19 BootstrapConfigDump-shows-populated-`load_assignment` note. Note fixtures 0014/0026/0027/0028 hold (no EDS cluster → no Endpoints entry → their `configs[]` indices NOT displaced).
- [ ] **Step 4: commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 21 Task 10: BEHAVIOR_CONTRACT EDS rows (stats + xDS EDS extension + EndpointsConfigDump) [ADR-0053, ADR-0054]"
```

---

### Task 11: State-4 phase-done verification + STATE advance to state-5-next

**Files:**
- Modify: `docs/envoy-rust/phases/21-xds-file-based-eds/PROGRESS.md` (quote every gate output)
- Modify: `docs/envoy-rust/STATE.md` (advance to state-4-complete / state-5-next)

- [ ] **Step 1: Run the full local gate suite, quoting every output into PROGRESS** (the 05.3→20 evidence discipline). Pre-build `tests/helpers/*` before the workspace test run (the cold-helper flake class):

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo build -p http1-echo-server   # pre-build helpers (project_flaky_access_log_fixture_0012)
cargo test --workspace
cargo deny check
# the 4 standalone-crate builds (project_isolated_crate_build_blindspot):
cargo build -p envoy-config && cargo build -p envoy-cluster && cargo build -p envoy-http1 && cargo build -p envoy-http2
```

- [ ] **Step 2: Run the fuzz short-budget gate** on the extended corpus (the `parse_bootstrap` target; the new `cluster_eds.yaml` seed included):

```bash
cd crates/envoy-config/fuzz && cargo +nightly fuzz run parse_bootstrap -- -runs=200000 -max_total_time=60   # or the project's CI-budget invocation
```
  Quote the clean-run output into PROGRESS.

- [ ] **Step 3: Push + capture the Docker-gated CI anchor.** Push the branch; capture the CI run that lights up ALL gates simultaneously: fixture 0029 + all 28 pre-existing fixtures (0001–0028) green + h2spec ≥95% + the fuzz gate on the 32-seed corpus + the 5 stable-toolchain gates. Quote the run id + `conclusion=success` into PROGRESS (the phase-18 lesson: the CI-evidence check is load-bearing — do NOT claim done on local green alone; note the fixture-0012/0011/0022 flake family per `project_flaky_access_log_fixture_0012` — a re-run clears them, not a regression).
- [ ] **Step 4: Advance STATE.md** to Active phase `21` state-4-complete / state-5-next (prepend the new active pointer; demote the current state-2 pointer to `_Historical_`; rewrite `## Next expected skill` to the state-5 code-review arc — `superpowers:requesting-code-review` over the phase-21 code range, SERIAL review subagents per concern-cluster, controller spot-verification; update `## Last commit` + `## Last updated`; append the state-4 evidence summary to the `### Phase-21 …` Notes). Commit PROGRESS + STATE.

```bash
git add docs/envoy-rust/phases/21-xds-file-based-eds/PROGRESS.md docs/envoy-rust/STATE.md
git commit -m "phase 21 Task 11: state-4 phase-done verification + STATE advance to state-5-next [ADR-0053, ADR-0054]"
```

---

## Self-review

**1. Spec coverage** (SPEC §3 D1–D8 → tasks):
- D1 (schema: `ClusterType::Eds` + `eds_cluster_config` + `load_assignment`→Option + exactly-one-of-and-consistent + ConfigError variants + the Eds cluster-build arm) → Task 1. ✓
- D2 (EDS file parser) → Task 2. ✓
- D3 (effective-`load_assignment` merge + name-selection + §5.7 ordering + validate-gate extension + consumer migration) → Task 3 (+ the consumer migration folded into Task 1's sweep + Task 3). ✓
- D4 (per-cluster `cluster.<name>.update_*` stats) → Task 4. ✓
- D5 (`EndpointsConfigDump` + query-strip) → Task 5. ✓
- D6 (harness `{{EDS_PATH}}` + `{{EDS_BACKEND_IP}}` + gateway-IP discovery + the per-side config_dump reconciliation [reused]) → Task 6. ✓
- D7 (fixture 0029 + wrapper) → Task 7. ✓
- D8 (backstop + fuzz seed + BEHAVIOR_CONTRACT) → Tasks 8 + 9 + 10. ✓
- SPEC §1 acceptance (a)–(f) → Task 11 (the state-4 gate). ✓
- SPEC §6.2 (the 11-item verification) → performed at THIS PLAN-write; locked as L1–L11 (ADR-0054). ✓
- SPEC §5 invariants: §5.1 no-new-crate (all tasks reuse), §5.2 inertness (Tasks 4/5/8 inertness tests), §5.3 every-cluster-resolves (Task 1 Step 7 expect + Task 3), §5.4 ownership (Task 3 — load-at-config-time), §5.5 config_dump separation + fixture-0026/0027/0028 stability (Task 5 + Task 7 compat + the C19 note), §5.6 one-shot load (Task 3 sync), §5.7 merge ordering (Task 3 Step 3 — CDS before EDS), §5.8 exactly-one-of-and-consistent (Task 1 Steps 5/6 + Task 3 Step 4). ✓

**2. Placeholder scan:** every code step shows the actual code or a precise adapt-to-existing instruction with the anchor line. The fixture echo-body string (Task 7 Step 2) is the one deliberate "copy from fixture 0028 + verify against a local run" — bilaterally verified at Step 3, not a placeholder. The `discover_host_gateway_ip` helper (Task 6 Step 3) is fully specified.

**3. Type consistency:** `parse_eds_file(path, contents) -> Result<Vec<LoadAssignment>, ConfigError>` (Task 2) is consumed by Task 3's EDS pass. `EdsClusterConfig { eds_config, service_name }` (Task 1) is read by Task 3 (`eds.eds_config.path_config_source.path`, `eds.service_name`), Task 4 (`cfg.cluster_type == Eds`), Task 5 (`c.cluster_type == Eds`). `ClusterType::Eds` (Task 1) gates the cluster-build arm (Task 1), the stats (Task 4), the config_dump (Task 5), the EDS pass (Task 3). `ConfigDumpEntry::Endpoints { static_endpoint_configs }` + `StaticEndpointConfigEntry.endpoint_config.cluster_name` (Task 5) match the fixture's `configs.N.static_endpoint_configs.0.endpoint_config.cluster_name` assertion (Task 7). `JsonSubtreeRule.path_envoy`/`path_envoy_rust` (reused from phase 20) match the fixture (Task 7). The `update_{attempt,success,failure,empty}` names (Task 4) match the fixture's `expected_stats` (Task 7) + the BEHAVIOR_CONTRACT rows (Task 10). Consistent.

**Carry-forward to the state-3 executor:** the D1 migration (Task 1) leaves the workspace RED until ALL ~17 `Cluster {}` construction sites (each gains `eds_cluster_config: None` + `load_assignment: Some(...)`) + the read sites + the new `Eds` match arm are fixed in the same commit — do not split it; confirm the exact set by building. Clippy PER TASK (`project_state3_arc_skips_clippy`). The state-4 verification (Task 11) MUST run the 4 standalone-crate builds (`project_isolated_crate_build_blindspot` — the `load_assignment`→`Option` change ripples through `envoy-cluster` most heavily) + capture the CI anchor (do not claim done on local green). Pre-build `tests/helpers/*` before `cargo test --workspace` + never run the Docker suite concurrently with cargo builds (`project_flaky_access_log_fixture_0012`). The `eds.rs` / dynamic-file-render-helper consolidations (M19-1/M20-T6-a, now N=4) are DEFERRED by deliberate decision (C18) — record in PROGRESS rollovers, do not attempt mid-phase. The host-gateway-IP discovery (Task 6) is the one genuinely-new harness primitive — verify it on BOTH macOS Docker Desktop (local) and Linux CI.
