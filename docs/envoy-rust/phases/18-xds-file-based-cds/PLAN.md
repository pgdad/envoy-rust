# Phase 18 (`18-xds-file-based-cds`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (the project default per `feedback_execution_style`; implementers dispatched SERIALLY per `feedback_serial_subagent_dispatch`) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK (NOT deferred to state-4) per `project_state3_arc_skips_clippy`.

**Goal:** Make file-based dynamic cluster discovery (CDS over the filesystem transport — `dynamic_resources.cds_config.path_config_source`) work end-to-end: clusters loaded from a YAML file at startup, routable on the data plane, observable via the `cluster_manager.cds.*` stat subset + the `/config_dump` `ClustersConfigDump` entry, proven bilaterally by fixture `0026-xds-file-based-cds`.

**Architecture:** The CDS file is read and parsed **synchronously at config-load time** (a new `envoy_config::load_dynamic_resources(&mut Bootstrap)` step in `envoy-bin`, between `parse_bootstrap` and `ClusterManager` construction — `std::fs`, NOT tokio::fs; the fuzz target `parse_bootstrap` stays pure). Dynamic clusters land in a `#[serde(skip)]` `Bootstrap.dynamic_clusters: Option<Vec<Cluster>>` side-field; every downstream consumer migrates from `bootstrap.static_resources.clusters` to a new `Bootstrap::all_clusters()` iterator, so dynamic clusters are full Clusters indistinguishable downstream (SPEC §5.3). Route→cluster reference validation **defers** while `dynamic_resources` is configured-but-unloaded and **re-enforces post-merge** (the L12b reconciliation — envoy-rust never honors Envoy's literal `validate_clusters: false` runtime-503 semantics; references must resolve at startup). All CDS load failures are **fatal at startup** (the L4 reconciliation — a narrow recorded divergence from Envoy's warn-and-serve for content errors). The `cluster_manager.*` stats register conditionally in `ClusterManager::from_bootstrap`; the `ClustersConfigDump` entry renders from the Bootstrap side-field already held by `AdminHandler` (no signature change).

**Tech Stack:** Rust (workspace crates `envoy-config`, `envoy-cluster`, `envoy-admin`, `envoy-bin`); `serde`/`serde_yaml`; `envoy-stats` `Counter`/`Gauge`; `testcontainers` differential harness (`with_copy_to` container file mounting); `cargo fuzz` (`parse_bootstrap`).

---

## §6.2 empirical lock-ins (verified against `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, 2026-06-02; divergences landed as ADR-0049)

The state-2 PLAN-write ran the HEAVY SPEC §6.2 verification in Docker (CDS-configured bootstrap + host backend + admin `/stats` + `/config_dump` scrapes; every routing claim cross-checked against backend access logs; version string `b0f43d67aa25c1b03c97186a200cc187f4c22db3/1.33.0/Clean/RELEASE/BoringSSL`). Findings — **the four marked ✦ DIVERGE materially from the SPEC projection (or were absent from it) and are reconciled by ADR-0049:**

- **L1 ✦ — CDS file envelope (§6.2 item 1; partial divergence → ADR-0049).** Both the bare `resources:` list AND the full `DiscoveryResponse` shape (`version_info` + `resources`) are accepted. The `@type` field per resource is **REQUIRED** (omitting it → `update_failure: 1` + log `missing @type in Any is only allowed for an empty object` + the route 503s). Envoy selects its parser by **FILE EXTENSION**: `.yaml`/`.yml` → YAML parser (which also accepts JSON syntax); any other or absent extension → JSON-only parser (YAML content in a `.json` or extensionless file fails with `update_failure`). **The exact minimal working CDS file (byte-for-byte):**

  ```yaml
  resources:
  - "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
    name: dynamic_backend
    type: STRICT_DNS
    dns_lookup_family: V4_ONLY
    load_assignment:
      cluster_name: dynamic_backend
      endpoints:
      - lb_endpoints:
        - endpoint:
            address:
              socket_address: { address: host.docker.internal, port_value: 8124 }
  ```

  **Reconciliation (ADR-0049):** envoy-rust's `parse_cds_file` requires `@type` per resource (`type.googleapis.com/envoy.config.cluster.v3.Cluster` — the ADR-0014 tagged-enum pattern); accepts the bare `resources:` envelope AND an optional `version_info` (accept-and-ignore); parses with `serde_yaml` REGARDLESS of file extension (a narrow recorded divergence: envoy-rust is more lenient on non-`.yaml` extensions; no differential observable — the fixture's CDS file ends in `.yaml`). **The Envoy-side container path substituted into `{{CDS_PATH}}` MUST end in `.yaml`** (Task 6/7 constraint).

- **L2 — initial-load/readiness ordering (item 2; CONFIRMED).** Envoy's startup log order: `cds: add 1 cluster(s)` → `cm init: all clusters initialized` → `all dependencies initialized. starting workers`. The dynamic cluster is routable the instant `/ready` first returns 200 — readiness implies loaded. envoy-rust mirrors this naturally (the merge is synchronous, before listeners bind). Zero settle/timing machinery needed (SPEC §5.6 stands).

- **L3 — the full `cluster_manager.*` stat tree after a successful load (item 3).** Envoy emits 18 names. envoy-rust's minimum-viable subset is **6 names** (extends the SPEC §2.1 4-name projection with the two deterministic siblings `update_attempt` + `update_rejected`):

  | Stat | Kind | Fixture-0026 value (bilateral) |
  |---|---|---|
  | `cluster_manager.cds.update_attempt` | counter | 1 |
  | `cluster_manager.cds.update_success` | counter | 1 |
  | `cluster_manager.cds.update_failure` | counter | 0 |
  | `cluster_manager.cds.update_rejected` | counter | 0 |
  | `cluster_manager.cluster_added` | counter | 1 |
  | `cluster_manager.active_clusters` | gauge | 1 |

  Envoy-only unasserted (ignored by the named-stat scrape): `cds.update_time`, `cds.version`, `cds.version_text`, `cds.update_duration`, `cds.init_fetch_timeout`, `cluster_modified`, `cluster_removed`, `cluster_updated`, `cluster_updated_via_merge`, `update_merge_cancelled`, `update_out_of_merge_window`, `warming_clusters`. None of the `cluster_manager.*` values change pre- vs post-GET (request counters live under `cluster.<name>.*`).

- **L4 ✦ — negative-path disposition is a 3-way split in Envoy (item 4; DIVERGES → ADR-0049).** Envoy: **(a)** nonexistent `path` → **hard startup failure** (container exits non-zero; `paths must refer to an existing path in the system` — a bootstrap-level PGV check); **(b)** file exists but malformed YAML → Envoy **starts and serves** (`/ready` 200), `cluster_manager.cds.update_failure: 1`, `active_clusters: 0`, the route 503s; **(c)** valid YAML but semantically-invalid resource (PGV violation, e.g. empty `name`; or a cluster-build failure) → Envoy starts, `update_rejected: 1` ticks (NOT `update_failure` — parse errors tick `update_failure`, semantic errors tick `update_rejected`); **unknown fields inside a resource are ACCEPTED** with only a warning (lenient protobuf parsing); a STRICT_DNS cluster with no `load_assignment` is accepted as a zero-endpoint cluster (route → `no healthy upstream` 503). **Reconciliation (ADR-0049):** envoy-rust treats **ALL CDS load errors as FATAL at startup** — missing/unreadable file, malformed YAML, missing `@type`, unknown fields (`deny_unknown_fields`), per-cluster validation failure. This is the project's fail-loud posture (every deferred field rejects loudly today); the warn-and-serve alternative would require honoring `validate_clusters: false` at runtime + a 503-on-unknown-cluster data-plane path — machinery with zero differential coverage (a deliberately-broken Envoy-side fixture is not a thing this project does). The divergence for classes (b)/(c) is recorded in BEHAVIOR_CONTRACT (Task 10). `update_failure`/`update_rejected` register at 0 and stay 0 in envoy-rust (any non-zero state is unreachable — the process exits instead); fixture 0026 asserts both at 0 bilaterally, which is satisfiable on both sides.

- **L5 — `ClustersConfigDump` shape (item 5; CONFIRMED, one correction).** With dynamic clusters, Envoy's `/config_dump` carries 6 top-level entries in order: `BootstrapConfigDump`[0], `ClustersConfigDump`[1], `ListenersConfigDump`[2], `ScopedRoutesConfigDump`[3], `RoutesConfigDump`[4], `SecretsConfigDump`[5]. The ClustersConfigDump entry (verbatim):

  ```json
  {
    "@type": "type.googleapis.com/envoy.admin.v3.ClustersConfigDump",
    "dynamic_active_clusters": [
      {
        "cluster": {
          "@type": "type.googleapis.com/envoy.config.cluster.v3.Cluster",
          "name": "dynamic_backend",
          "type": "STRICT_DNS",
          "dns_lookup_family": "V4_ONLY",
          "load_assignment": { "...": "..." }
        },
        "last_updated": "2026-06-02T14:46:32.441Z"
      }
    ]
  }
  ```

  **Correction vs the SPEC D5 projection:** there is NO `version_info` key in the entry (the CDS file had none — proto3 JSON omits empty fields), and a static-only Envoy emits the entry with ONLY a `static_clusters` key (the `dynamic_active_clusters` key entirely absent). Empty keys are omitted on both sides. envoy-rust (Task 5): emit the entry ONLY when `dynamic_resources` is configured; keys `static_clusters` + `dynamic_active_clusters` each `skip_serializing_if = Vec::is_empty`; the inner `cluster` object carries the `@type` tag (the `ConfigDumpEntry` tagged-serialization pattern); `last_updated` reuses the BootstrapConfigDump ISO-8601 emitter. Because both proxies emit `BootstrapConfigDump` at `configs[0]` and `ClustersConfigDump` at `configs[1]`, the fixture's JsonShape assertion anchors on `configs.1.*` bilaterally.

- **L6 — route-to-dynamic-cluster wire shape (item 6; CONFIRMED).** Byte-identical header set to the static-cluster baseline: `server`, `date`, `content-type`, `content-length`, `x-envoy-upstream-service-time` (+ backend headers). No new response headers, no new access-log flags. Fixture 0026's data-plane probe is the standard fixture-0008 shape.

- **L7 — zero-static-clusters bootstrap is valid (item 7; CONFIRMED).** Both "no `clusters:` key at all" and explicit `clusters: []` start clean and serve. Fixture 0026 omits the `clusters:` key entirely (the cleaner shape). NOTE for envoy-rust: `StaticResources.clusters` already has `#[serde(default)]` (`bootstrap.rs:47-52`) — no schema change needed for this.

- **L8 — `resource_api_version` is optional (item 8; CONFIRMED).** Omitting it entirely works with no deprecation warning. envoy-rust: `Option<String>`, accept `"V3"` or absent, reject anything else (`UnsupportedResourceApiVersion`).

- **L9 ✦ — static/dynamic name collision: STATIC WINS (item 9; reverses the SPEC D1 projection → ADR-0049).** A cluster defined both statically and in the CDS file → Envoy keeps the STATIC one and skips the CDS entry as "unmodified" (`added/updated 0 cluster(s), skipped 1 unmodified cluster(s)`); no error, no startup failure; `update_success` still ticks 1; `/config_dump` shows it under `static_clusters` only; the data plane serves the static endpoint. **Reconciliation (ADR-0049):** envoy-rust mirrors — on collision the dynamic cluster is SKIPPED (with a `tracing::warn!`), the static cluster wins, no error. **The SPEC D1 `DuplicateClusterName { name }` ConfigError variant is DROPPED.** The backstop (Task 8) asserts the static endpoint serves.

- **L10 — stat conditionality (item 10; CONFIRMED).** With `dynamic_resources` ABSENT, Envoy emits NO `cluster_manager.cds.*` stats at all (the cds subtree is conditional even in Envoy) but DOES emit the base `cluster_manager.*` counters (`active_clusters`, `cluster_added`, …) unconditionally. envoy-rust registers ALL 6 subset names conditionally (only when `dynamic_resources.cds_config` is configured) — the base names stay Envoy-only-unasserted on all non-CDS fixtures (fixture 0011's Prometheus set-diff posture unchanged; zero existing-fixture edits).

- **L11 — file hot-reload (item 11; INCONCLUSIVE — environment-limited).** `mv`-replace of the bind-mounted CDS file was visible inside the container but Envoy never reloaded (no `cds:` log line, stats unchanged) — the known macOS Docker Desktop virtiofs/inotify limitation, NOT an Envoy behavior finding. The deferred file-watch follow-up phase's SPEC must verify hot-reload on Linux (CI) or via in-container file mutation.

- **L12 ✦ — TWO bootstrap prerequisites absent from the SPEC projection (→ ADR-0049).**
  - **(a) `node.id` + `node.cluster` are REQUIRED by Envoy when CDS is configured** — without them Envoy exits at startup (`node 'id' and 'cluster' are required`). Fixture 0026 carries a `node:` block (every existing fixture already does — e.g. fixture 0008 line 1); envoy-rust already parses `Node { id, cluster }` (phase 01); envoy-rust does NOT add a mirror requirement validator (both sides are always configured; no differential observable).
  - **(b) The static `route_config` referencing a CDS-supplied cluster requires `validate_clusters: false`** — without it Envoy exits at startup (`route: unknown cluster 'dynamic_backend'`); Envoy's inline route-table validation runs against the static cluster set only. Fixture 0026 sets `validate_clusters: false` on the `route_config` of BOTH sides (configs identical). envoy-rust gains `RouteConfiguration.validate_clusters: Option<bool>` as **parse-and-accept** (the ADR-0024/0026 parse-only precedent) — envoy-rust does NOT honor its literal semantics; instead envoy-rust's own validation **defers cluster-reference checks while `dynamic_resources` is configured-but-unloaded and re-enforces post-merge** (Task 1/3). A route to a cluster in NEITHER list still fails envoy-rust startup (recorded narrow divergence vs Envoy's runtime-503 under `validate_clusters: false`).

## PLAN-time SPEC corrections (verified against HEAD `3acf7367b`)

A read-only Explore subagent verified the SPEC §0/§3 anchors + the structural projections against HEAD; the controller re-verified the load-bearing items by direct grep. **One anchor drifted; four structural corrections:**

- **Anchors ACCURATE:** `Bootstrap` struct + xDS-reservation comment `crates/envoy-config/src/bootstrap.rs:8-29` (the comment at `:19-24` names exactly this landing); `validate` entry `bootstrap.rs:1851` (`pub(crate) fn validate(bootstrap: &mut Bootstrap) -> Result<(), ConfigError>`); `ClusterManager::from_bootstrap` `crates/envoy-cluster/src/cluster.rs:721` (`pub async fn from_bootstrap(&Bootstrap, Arc<StatsRegistry>) -> Result<ClusterManager, ClusterError>`; immutable `HashMap<String, Arc<Cluster>>`, no mutator; reached from `envoy-bin` via the free-fn re-export `envoy_cluster::from_bootstrap`, `main.rs:123-127`); envoy-bin startup `main.rs:48-127` + per-cluster TLS loop `:187-210` (iterates `bootstrap.static_resources.clusters` at `:191`); admin `ConfigDumpEntry` enum `crates/envoy-admin/src/endpoint.rs:301-309` (xDS-deferral comment `:294-300` names exactly this landing); harness `with_copy_to` `tests/differential/src/upstream.rs:104-113`; `render_yaml` `tests/differential/src/lib.rs:893-903`.
- **Anchor DRIFTED — the fuzz corpus count:** the `.gitignore` allow-list (`crates/envoy-config/fuzz/.gitignore`) carries **28** seed files, but the `fuzz_corpus_seeds_parse_or_reject_cleanly` test arrays (`bootstrap.rs:3891-3941`) cover only **27** (23 parse + 3 reject + 1 minimal) — `cluster_http2_protocol_options.yaml` is allow-listed but missing from the SUCCESS array (a pre-existing inconsistency, likely from 13.2; carried to the state-5 review inventory). Task 9 adds the new seed to BOTH (arrays 27→28, allow-list 28→29) and MAY opportunistically restore `cluster_http2_protocol_options.yaml` to the SUCCESS array.
- **Correction 1 — the config load is SYNC; envoy-config has NO tokio dep.** `parse_bootstrap` (`lib.rs:502`) is pure (`serde_yaml::from_str` + `validate`); the bootstrap file read lives in `envoy-bin` (`main.rs:49`, `std::fs::read_to_string`). The SPEC D3 `load_dynamic_clusters(&bootstrap).await` / `tokio::fs` projection is CORRECTED: the CDS read is a **sync `std::fs::read_to_string`** inside a new pure-by-injection `envoy_config` public fn; NO tokio dep is added to `envoy-config` (SPEC §6.8's conditional does not fire); the fuzz target stays pure (it never calls the loading fn).
- **Correction 2 — `AdminHandler` needs NO signature widening.** `AdminHandler::new` (`crates/envoy-admin/src/handler.rs:90-104`) already holds `Arc<Bootstrap>` + `Arc<ClusterManager>`; the dynamic clusters ride on the Bootstrap side-field, so D5 reads `bootstrap.dynamic_clusters` directly (the SPEC's "widening its signature; the 08.2 D13b precedent" sentence is unnecessary).
- **Correction 3 — route→cluster reference checks live at TWO sites** (+ one dependent `.expect`): the tcp_proxy filter check (`bootstrap.rs:2013-2020`) and the HCM route check inside `validate_hcm` (`:2205-2206`), with the `Http2ClusterFromHttp1Listener` gate (`:2208-2227`) and an `.expect("UnknownCluster check above guarantees presence")` (`:2214`) relying on the route-check's guarantee. The Task-1 deferred-validation logic must cover BOTH UnknownCluster sites; the `.expect` stays sound because post-merge re-validation (Task 3) restores the guarantee before any runtime consumer is constructed.
- **Correction 4 — downstream consumers that iterate the cluster list** (the Task-3 migration set, confirmed): `validate()`'s per-cluster invariant loop (`bootstrap.rs:1862`); the tcp_proxy + HCM reference checks (above); `ClusterManager`/`envoy_cluster::from_bootstrap` (`cluster.rs:721+`); `H1PoolManager::for_bootstrap` (`crates/envoy-http1/src/pool.rs:443-448`, iterates at `:451`); `H2PoolManager::for_bootstrap` (`crates/envoy-http2/src/pool.rs:592-597`); `envoy_health::Scheduler::spawn` (`crates/envoy-health/src/scheduler.rs:40-47`); the envoy-bin TLS loop (`main.rs:191`). `OutlierManager::for_bootstrap` takes only `&ClusterManager` (`crates/envoy-cluster/src/outlier.rs:114-128`) — no migration needed (it iterates the already-merged manager).
- **Harness confirmations:** `Driver::Http1KeepAlive` (`lib.rs:167-180`) carries `requests` + `settle_ms` + `expected_stats`; `Http1KeepAliveRequest` supports `expected_status` + `expected_body` (ByteExact) + header rules; `AdminScrapeCase` (`lib.rs:277-282`) carries `path`/`expected_status`/`expected_content_type`/`expected_body_rule` (the fixture-0014 `json_shape` rule); `PreRequest` (`lib.rs:296-301`) carries NO assertions → the bilateral `ClustersConfigDump` assertion requires the **Task-6 `admin_scrapes` extension** on `Http1KeepAlive` (reusing `AdminScrapeCase` verbatim — the Http2KeepAlive-reuses-Http1KeepAliveRequest precedent). Existing substitution keys: `PORT`, `ADMIN_PORT`, `BACKEND_HOST`, `BACKEND_PORT`, `HTTP1_BACKEND_PORT`, `HTTP2_BACKEND_PORT`, `TLS_BACKEND_PORT`, + 6 TLS-PKI path keys. Fixture render/start flow: `run_fixture` `lib.rs:2057-2467` (template scan `:2117-2130` → `render_yaml` `:2384-2385` → temp write `:2386-2387` → `upstream::start` `:2460-2467` / `subject::start` `:2468`).

## §6.1 split-gate decision

Re-estimated against the §6.2-refined surface (which ADDS the `validate_clusters` parse field + the deferred-validation logic + the `admin_scrapes` harness extension, but REMOVES the `DuplicateClusterName` error path, the tokio-dep concern, and the AdminHandler signature change): **~1480–1600 LoC / 11 tasks** (production ~480, tests ~480, fixture/harness/backstop/fuzz ~440, docs ~80) — at the boundary but not over the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task gate, the same posture as the phase-16 (~1450–1650) and phase-17 (~1450–1550) no-split decisions. **Single un-split phase.** The work is tightly coupled: Tasks 1–3 form one schema+load unit whose output (the merged cluster list) Tasks 4–5 observe and Task 7's single fixture asserts end-to-end; an 18.1/18.2 split would re-touch the same files in both halves for little isolation benefit. **ADR-0050 (the reserved split ADR) does NOT fire.** (If a single task's sub-steps blow up past ~10 items mid-execution, §6.1 permits a mid-execution split.)

---

## File structure

- `crates/envoy-config/src/bootstrap.rs` — `Bootstrap` gains `dynamic_resources: Option<DynamicResources>` + `#[serde(skip)] dynamic_clusters: Option<Vec<Cluster>>` + `all_clusters()`; new `DynamicResources`/`ConfigSource`/`PathConfigSource` structs; `RouteConfiguration` gains `validate_clusters: Option<bool>` (parse-and-accept); the two cluster-reference checks + the per-cluster invariant loop go through `all_clusters()` with the configured-but-unloaded deferral.
- `crates/envoy-config/src/cds.rs` — NEW module: the CDS file envelope structs + `parse_cds_file(&str) -> Result<Vec<Cluster>, ConfigError>`.
- `crates/envoy-config/src/lib.rs` — `pub mod cds;` + new `ConfigError` variants + `pub fn load_dynamic_resources(&mut Bootstrap) -> Result<(), ConfigError>` (the sync file read + parse + collision-skip + merge + re-validate).
- `crates/envoy-bin/src/main.rs` — the `load_dynamic_resources` call after `parse_bootstrap`; the TLS loop migrates to `all_clusters()`.
- `crates/envoy-cluster/src/cluster.rs` — `from_bootstrap` iterates `all_clusters()` + conditionally registers/sets the 6 `cluster_manager.*` stats.
- `crates/envoy-http1/src/pool.rs` + `crates/envoy-http2/src/pool.rs` + `crates/envoy-health/src/scheduler.rs` — the `for_bootstrap`/`spawn` iteration migrates to `all_clusters()`.
- `crates/envoy-admin/src/endpoint.rs` (+ `handler.rs` call site) — `ConfigDumpEntry` gains the `Clusters` variant (conditional emission).
- `tests/differential/src/lib.rs` — `{{CDS_PATH}}` detection + per-side `cds.yaml` rendering; `Driver::Http1KeepAlive` gains `#[serde(default)] admin_scrapes: Vec<AdminScrapeCase>`.
- `tests/differential/src/upstream.rs` — the CDS file `with_copy_to` mount into the Envoy container.
- `tests/fixtures/0026-xds-file-based-cds/` — `envoy.yaml`, `envoy-rust.yaml`, `cds.yaml`, `expectations.yaml`, `README.md`.
- `tests/differential/tests/xds_file_based_cds.rs` — Docker-gated wrapper.
- `crates/envoy-bin/tests/xds_file_based_cds.rs` — in-process backstop (happy + 3 negative paths + collision).
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/dynamic_resources_cds.yaml` — NEW seed (+ `.gitignore` allow-list + SUCCESS-array entries, atomically).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — "18 entries" stat rows + the xDS-wire-state-machine first population + the ClustersConfigDump admin-body-shapes row.

---

### Task 1: `envoy-config` schema — `dynamic_resources` + `validate_clusters` + deferred cluster-reference validation

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`Bootstrap` `:8-29`; `RouteConfiguration` `:908-912`; `validate` `:1851+`; the reference checks `:2013-2020` + `:2205-2227`)
- Modify: `crates/envoy-config/src/lib.rs` (new `ConfigError` variants + re-exports)
- Test: `crates/envoy-config/src/bootstrap.rs` `#[cfg(test)]` module

- [ ] **Step 1: Write failing serde + validator tests.**

```rust
#[test]
fn bootstrap_parses_dynamic_resources_cds_path_config_source() {
    let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  cds_config:
    resource_api_version: V3
    path_config_source:
      path: /tmp/cds.yaml
"#;
    let b = crate::parse_bootstrap(yaml).unwrap();
    let dr = b.dynamic_resources.as_ref().unwrap();
    let cs = dr.cds_config.as_ref().unwrap();
    assert_eq!(cs.path_config_source.path, "/tmp/cds.yaml");
    assert_eq!(cs.resource_api_version.as_deref(), Some("V3"));
}

#[test]
fn dynamic_resources_rejects_deferred_fields() {
    // lds_config / ads_config / api_config_source / watched_directory all
    // rejected loudly by deny_unknown_fields (SPEC §4 deferral ledger).
    for field in [
        "lds_config: { path_config_source: { path: /x } }",
        "ads_config: { api_type: GRPC }",
    ] {
        let yaml = format!(
            "node: {{ id: t, cluster: t }}\nadmin: {{ address: {{ socket_address: {{ address: 0.0.0.0, port_value: 0 }} }} }}\ndynamic_resources:\n  {field}"
        );
        assert!(crate::parse_bootstrap(&yaml).is_err(), "{field} should reject");
    }
    // api_config_source / ads on the ConfigSource; watched_directory on PathConfigSource:
    let yaml = r#"
node: { id: t, cluster: t }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  cds_config:
    api_config_source: { api_type: GRPC }
"#;
    assert!(crate::parse_bootstrap(yaml).is_err());
    let yaml = r#"
node: { id: t, cluster: t }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  cds_config:
    path_config_source:
      path: /tmp/cds.yaml
      watched_directory: { path: /tmp }
"#;
    assert!(crate::parse_bootstrap(yaml).is_err());
}

#[test]
fn resource_api_version_v3_or_absent_accepted_others_rejected() {
    // L8: absent + V3 accepted; V2 / garbage rejected (UnsupportedResourceApiVersion).
    // (absent case covered by bootstrap_parses_... below using no resource_api_version)
    let yaml = r#"
node: { id: t, cluster: t }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  cds_config:
    resource_api_version: V2
    path_config_source: { path: /tmp/cds.yaml }
"#;
    let err = crate::parse_bootstrap(yaml).unwrap_err();
    assert!(matches!(err, crate::ConfigError::UnsupportedResourceApiVersion(ref v) if v == "V2"));
}

#[test]
fn route_config_parses_validate_clusters_field() {
    // L12b: parse-and-accept (Envoy requires `validate_clusters: false` on a
    // route_config referencing CDS clusters; configs are identical on both sides).
    let yaml = r#"
name: local_route
validate_clusters: false
virtual_hosts: []
"#;
    let rc: RouteConfiguration = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(rc.validate_clusters, Some(false));
}

#[test]
fn route_to_unknown_cluster_deferred_when_dynamic_resources_configured_unloaded() {
    // The fixture-0026 topology: zero static clusters + a route to a cluster the
    // CDS file will supply. parse_bootstrap (which cannot do I/O) must ACCEPT this
    // — the reference check defers until load_dynamic_resources re-validates.
    let yaml = format!("{HCM_LISTENER_TO_DYNAMIC_BACKEND}\ndynamic_resources:\n  cds_config:\n    path_config_source: {{ path: /tmp/cds.yaml }}\n");
    assert!(crate::parse_bootstrap(&yaml).is_ok());
}

#[test]
fn route_to_unknown_cluster_still_rejected_without_dynamic_resources() {
    // Regression: the deferral ONLY applies when dynamic_resources.cds_config is
    // configured. The existing UnknownCluster behavior is unchanged otherwise.
    let yaml = HCM_LISTENER_TO_DYNAMIC_BACKEND; // no dynamic_resources block
    let err = crate::parse_bootstrap(yaml).unwrap_err();
    assert!(matches!(err, crate::ConfigError::UnknownCluster(ref c) if c == "dynamic_backend"));
}
```

Where `HCM_LISTENER_TO_DYNAMIC_BACKEND` is a test-module const carrying a complete bootstrap YAML: `node` + `admin` + one HCM listener whose `route_config` has `validate_clusters: false` and one route to cluster `dynamic_backend`, and NO `clusters:` key (mirror the existing `hcm_route_to_cluster` test fixtures' shape).

- [ ] **Step 2: Run tests, verify they fail.** Run: `cargo test -p envoy-config dynamic_resources` and `cargo test -p envoy-config validate_clusters`. Expected: FAIL (unknown field `dynamic_resources` on Bootstrap; unknown field `validate_clusters` on RouteConfiguration).
- [ ] **Step 3: Implement the schema.**

```rust
// In bootstrap.rs, after the Node struct (the :19-24 xDS-reservation comment is
// UPDATED to note that phase 18 consumed the reservation for dynamic_resources;
// Node itself stays an open struct — the gRPC-xDS phase tightens it):

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bootstrap {
    #[serde(default)]
    pub node: Option<Node>,
    #[serde(default)]
    pub admin: Option<Admin>,
    #[serde(default)]
    pub static_resources: StaticResources,
    /// 18 D1: file-based CDS (the xDS-family opener; ADR-0048/ADR-0049).
    /// Only `cds_config.path_config_source` is supported; everything else
    /// in the upstream DynamicResources proto is rejected loudly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic_resources: Option<DynamicResources>,
    /// 18 D3: clusters loaded from the CDS file by `load_dynamic_resources`.
    /// `None` = not loaded yet (parse_bootstrap leaves it None — the fuzz
    /// target does no I/O); `Some(vec)` = loaded (possibly empty).
    /// NOT serialized: the BootstrapConfigDump must show the bootstrap as
    /// parsed from disk (SPEC §5.5 config_dump separation); dynamic clusters
    /// surface in the ClustersConfigDump entry instead.
    #[serde(skip)]
    pub dynamic_clusters: Option<Vec<Cluster>>,
}

impl Bootstrap {
    /// 18 D3: the effective cluster list — static clusters followed by
    /// dynamically-loaded (CDS) clusters. Every downstream consumer
    /// (validators, ClusterManager, pools, health, TLS) iterates THIS,
    /// never `static_resources.clusters` directly (SPEC §5.3: dynamic
    /// clusters are full Clusters, indistinguishable downstream).
    pub fn all_clusters(&self) -> impl Iterator<Item = &Cluster> {
        self.static_resources
            .clusters
            .iter()
            .chain(self.dynamic_clusters.iter().flatten())
    }

    /// 18 D1/D3: true iff a CDS config source is configured but
    /// `load_dynamic_resources` has not run yet. While true, cluster-reference
    /// validation DEFERS (the references may resolve against the CDS file);
    /// `load_dynamic_resources` re-validates with full enforcement.
    pub(crate) fn cds_configured_but_unloaded(&self) -> bool {
        self.dynamic_resources
            .as_ref()
            .and_then(|dr| dr.cds_config.as_ref())
            .is_some()
            && self.dynamic_clusters.is_none()
    }
}

/// 18 D1: `dynamic_resources` — only the CDS filesystem transport at this
/// phase (ADR-0048). `lds_config` / `ads_config` are deliberately NOT fields:
/// deny_unknown_fields rejects them loudly (SPEC §4 deferral ledger).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DynamicResources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cds_config: Option<ConfigSource>,
}

/// 18 D1: a ConfigSource restricted to the filesystem transport.
/// `api_config_source` (gRPC/REST) / `ads` are NOT fields (rejected; deferred
/// to the gRPC-xDS phase, which also supersedes ADR-0014 per ADR-0048).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConfigSource {
    pub path_config_source: PathConfigSource,
    /// L8: optional; Envoy defaults it. Accept "V3" or absent; reject others
    /// (validate(), UnsupportedResourceApiVersion).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_api_version: Option<String>,
}

/// 18 D1: `path_config_source` — the file path. `watched_directory` is NOT a
/// field (rejected; deferred with file watching per SPEC §4).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PathConfigSource {
    pub path: String,
}
```

`RouteConfiguration` (`:908-912`) gains:

```rust
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteConfiguration {
    pub name: String,
    pub virtual_hosts: Vec<VirtualHost>,
    /// 18 L12b (ADR-0049): parse-and-accept. Envoy requires `validate_clusters:
    /// false` on a static route_config that references CDS-supplied clusters
    /// (else it exits: "route: unknown cluster"). envoy-rust parses the field so
    /// the identical fixture configs load, but does NOT honor its literal
    /// runtime-503 semantics — envoy-rust's own reference validation defers
    /// while CDS is configured-but-unloaded and re-enforces post-merge
    /// (Bootstrap::cds_configured_but_unloaded). A route to a cluster in
    /// NEITHER list still fails startup (recorded divergence, BEHAVIOR_CONTRACT).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate_clusters: Option<bool>,
}
```

New `ConfigError` variants in `lib.rs` (next to `UnknownCluster`):

```rust
#[error("dynamic_resources.cds_config.resource_api_version '{0}' is unsupported; envoy-rust accepts only 'V3' or absent")]
UnsupportedResourceApiVersion(String),
#[error("reading CDS file '{path}': {source}")]
CdsFileError { path: String, #[source] source: std::io::Error },
#[error("parsing CDS file '{path}': {message}")]
CdsParseError { path: String, message: String },
```

Validation changes in `bootstrap.rs`:
1. `validate()` (`:1851`): the per-cluster invariant loop (`:1862`, `for cluster in &bootstrap.static_resources.clusters`) — leave iterating static clusters here (dynamic clusters are validated inside `load_dynamic_resources`, Task 3, which calls the same per-cluster helper); ADD the `resource_api_version` check (`Some(v) if v != "V3"` → `UnsupportedResourceApiVersion`).
2. The tcp_proxy cluster check (`:2013-2020`) and the HCM route check (`:2205-2206`): both become `if !found && !bootstrap.cds_configured_but_unloaded() { return Err(UnknownCluster(...)) }`. The `Http2ClusterFromHttp1Listener` gate (`:2208-2227`): its cluster lookup `.expect(...)` (`:2214`) must not fire when the reference was deferred — restructure to `if let Some(cluster_ref) = clusters.iter().find(...)` (the gate is skipped for deferred references and re-enforced at the Task-3 re-validation). NOTE `validate_hcm` receives `clusters: &[Cluster]` as a slice parameter — Task 3's re-validation passes the merged list; check the call site to thread `bootstrap.cds_configured_but_unloaded()` (or pass an enum `ClusterRefEnforcement { Enforce, DeferUnresolved }`) — the implementer picks the cleaner threading, the binding constraint is: defer iff `cds_configured_but_unloaded()`.

- [ ] **Step 4: Run tests, verify pass.** Run: `cargo test -p envoy-config` (full crate — the existing UnknownCluster tests at `:3691`/`:6009`/`:6023` must stay green; they have no `dynamic_resources` so the deferral never engages for them). Then `cargo build --workspace` (the `Bootstrap` struct gained fields — any exhaustive struct literal in other crates' tests needs `dynamic_resources: None, dynamic_clusters: None` added in the same commit).
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 18 Task 1: dynamic_resources schema + validate_clusters + deferred cluster-reference validation [ADR-0049]"
```

---

### Task 2: `envoy-config` — the CDS file parser (`cds.rs`)

**Files:**
- Create: `crates/envoy-config/src/cds.rs`
- Modify: `crates/envoy-config/src/lib.rs` (`pub mod cds;`)
- Test: `crates/envoy-config/src/cds.rs` `#[cfg(test)]` module

- [ ] **Step 1: Write failing tests.**

```rust
const MINIMAL_CDS: &str = r#"
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: dynamic_backend
  type: STRICT_DNS
  dns_lookup_family: V4_ONLY
  load_assignment:
    cluster_name: dynamic_backend
    endpoints:
    - lb_endpoints:
      - endpoint:
          address:
            socket_address: { address: 127.0.0.1, port_value: 8124 }
"#;

#[test]
fn parses_bare_resources_envelope() {
    // L1: the bare `resources:` list (the minimal working shape Envoy accepts).
    let clusters = parse_cds_file(MINIMAL_CDS).unwrap();
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].name, "dynamic_backend");
}

#[test]
fn parses_discovery_response_envelope_with_version_info() {
    // L1: the full DiscoveryResponse shape (version_info + resources) is also
    // accepted; version_info is accept-and-ignore.
    let yaml = format!("version_info: \"1\"\n{}", MINIMAL_CDS.trim_start());
    let clusters = parse_cds_file(&yaml).unwrap();
    assert_eq!(clusters.len(), 1);
}

#[test]
fn rejects_resource_without_at_type() {
    // L1: @type per resource is REQUIRED (mirrors Envoy's
    // "missing @type in Any" rejection).
    let yaml = r#"
resources:
- name: dynamic_backend
  type: STRICT_DNS
  load_assignment: { cluster_name: dynamic_backend, endpoints: [] }
"#;
    assert!(parse_cds_file(yaml).is_err());
}

#[test]
fn rejects_resource_with_wrong_at_type() {
    // A Listener @type inside a CDS file is rejected (CDS carries Clusters only).
    let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.listener.v3.Listener
  name: not_a_cluster
"#;
    assert!(parse_cds_file(yaml).is_err());
}

#[test]
fn rejects_malformed_yaml() {
    assert!(parse_cds_file("resources: [unclosed").is_err());
}

#[test]
fn rejects_unknown_fields_in_resource() {
    // L4 reconciliation (ADR-0049): envoy-rust is STRICTER than Envoy here —
    // Envoy warn-accepts unknown resource fields; envoy-rust's Cluster schema
    // is deny_unknown_fields → reject-fatal. Recorded divergence.
    let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: c
  type: STRICT_DNS
  this_field_does_not_exist: true
  load_assignment: { cluster_name: c, endpoints: [] }
"#;
    assert!(parse_cds_file(yaml).is_err());
}

#[test]
fn dynamic_clusters_pass_the_same_per_cluster_validation_as_static() {
    // SPEC D2: dynamic clusters go through the SAME validation gauntlet.
    // A cluster whose load_assignment.cluster_name mismatches its name is
    // rejected (the existing LoadAssignmentNameMismatch invariant).
    let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: dynamic_backend
  type: STRICT_DNS
  load_assignment:
    cluster_name: WRONG_NAME
    endpoints:
    - lb_endpoints:
      - endpoint:
          address:
            socket_address: { address: 127.0.0.1, port_value: 8124 }
"#;
    assert!(parse_cds_file(yaml).is_err());
}

#[test]
fn json_content_is_accepted() {
    // L1: serde_yaml parses JSON (JSON is a YAML subset); envoy-rust accepts
    // JSON-syntax CDS files regardless of extension (the recorded narrow
    // leniency divergence vs Envoy's extension-driven parser selection).
    let json = r#"{"resources": [{"@type": "type.googleapis.com/envoy.config.cluster.v3.Cluster", "name": "dynamic_backend", "type": "STRICT_DNS", "dns_lookup_family": "V4_ONLY", "load_assignment": {"cluster_name": "dynamic_backend", "endpoints": [{"lb_endpoints": [{"endpoint": {"address": {"socket_address": {"address": "127.0.0.1", "port_value": 8124}}}}]}]}}]}"#;
    let clusters = parse_cds_file(json).unwrap();
    assert_eq!(clusters.len(), 1);
}
```

- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config cds`. Expected: FAIL (module does not exist).
- [ ] **Step 3: Implement `cds.rs`.**

```rust
//! 18 D2: file-based CDS parsing (the xDS-family filesystem transport opener;
//! ADR-0048/ADR-0049). Parses the path_config_source file's DiscoveryResponse-
//! shaped envelope into Vec<Cluster> using the existing Cluster serde schema
//! (the ADR-0014 YAML-native shim extended by one envelope — NO protos/prost).
//!
//! Envelope shape (L1, empirically verified vs Envoy v1.33):
//!   - bare `resources:` list OR full DiscoveryResponse (`version_info` +
//!     `resources`) — both accepted; version_info is accept-and-ignore.
//!   - each resource MUST carry `@type: type.googleapis.com/envoy.config.cluster.v3.Cluster`
//!     (mirrors Envoy's "missing @type in Any" rejection).
//!   - parsed with serde_yaml regardless of file extension (envoy-rust is more
//!     lenient than Envoy's extension-driven parser selection — recorded
//!     divergence, ADR-0049/BEHAVIOR_CONTRACT).

use serde::Deserialize;

use crate::bootstrap::Cluster;

/// The CDS file envelope. `deny_unknown_fields` is deliberately NOT applied to
/// the envelope itself: Envoy's DiscoveryResponse carries fields envoy-rust
/// ignores (`version_info`, `type_url`, `nonce`, ...) — accept-and-ignore keeps
/// real-world CDS files loadable. The per-resource payload (Cluster) keeps its
/// deny_unknown_fields strictness (the L4 reconciliation).
#[derive(Debug, Deserialize)]
struct CdsFile {
    #[serde(default)]
    resources: Vec<CdsResource>,
}

/// One @type-tagged resource. The tagged-enum-on-@type pattern is ADR-0014's;
/// CDS files carry Cluster resources only (a non-Cluster @type fails to match
/// the single variant and rejects loudly).
#[derive(Debug, Deserialize)]
#[serde(tag = "@type")]
enum CdsResource {
    #[serde(rename = "type.googleapis.com/envoy.config.cluster.v3.Cluster")]
    Cluster(Cluster),
}

/// Parse a CDS file's contents into the dynamic cluster list. Every cluster
/// passes the same per-cluster validation static clusters do (SPEC D2) — the
/// caller (`load_dynamic_resources`, lib.rs) additionally runs collision
/// checking and post-merge route-reference re-validation.
pub fn parse_cds_file(contents: &str) -> Result<Vec<Cluster>, crate::ConfigError> {
    let file: CdsFile = serde_yaml::from_str(contents).map_err(|e| {
        crate::ConfigError::CdsParseError { path: String::new(), message: e.to_string() }
    })?;
    let clusters: Vec<Cluster> = file
        .resources
        .into_iter()
        .map(|CdsResource::Cluster(c)| c)
        .collect();
    for cluster in &clusters {
        crate::bootstrap::validate_cluster(cluster)?; // the per-cluster invariant helper (Step 3b)
    }
    Ok(clusters)
}
```

**Step 3b:** the per-cluster invariant block inside `validate()` (`bootstrap.rs:1862+` — LoadAssignmentNameMismatch, EmptyClusterEndpoints, transport_socket direction, circuit-breaker/health-check/outlier validators, …) is extracted into a `pub(crate) fn validate_cluster(cluster: &Cluster) -> Result<(), ConfigError>` helper so `validate()` and `parse_cds_file` share one source of truth. `validate()`'s loop body becomes a call to it. (Pure refactor — the existing envoy-config tests prove no behavior change.) NOTE: the path in `CdsParseError` is filled by the caller (`load_dynamic_resources`) which knows the path; `parse_cds_file` leaves it empty — OR the implementer threads `path: &str` as a parameter for better error messages (preferred; adjust the signature to `parse_cds_file(path: &str, contents: &str)`).

**EmptyClusterEndpoints caveat:** the existing invariant rejects zero-endpoint clusters. Envoy ACCEPTS a zero-endpoint dynamic cluster (L4c-i — serves `no healthy upstream` 503). Keep envoy-rust's rejection (fail-loud; consistent with static clusters; recorded as part of the L4 all-fatal divergence). The fixture's CDS cluster always has an endpoint.

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-config` (full crate). Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
git add crates/envoy-config/src/cds.rs crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 18 Task 2: CDS file parser (resources envelope, @type-tagged Cluster payloads) [ADR-0049]"
```

---

### Task 3: `load_dynamic_resources` + the effective-cluster-list merge + consumer migration

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (the new `load_dynamic_resources` public fn)
- Modify: `crates/envoy-bin/src/main.rs` (the call after `parse_bootstrap` `:51`; the TLS loop `:191`)
- Modify: `crates/envoy-cluster/src/cluster.rs` (`from_bootstrap` cluster iteration)
- Modify: `crates/envoy-http1/src/pool.rs:451` + `crates/envoy-http2/src/pool.rs` + `crates/envoy-health/src/scheduler.rs:47` (iteration migration)
- Test: `crates/envoy-config/src/lib.rs` or `bootstrap.rs` test module (the load paths); existing crate tests (the migration is behavior-preserving)

- [ ] **Step 1: Write failing tests** (in envoy-config, using `tempfile` — already a workspace dev-dep per ADR-0018):

```rust
#[test]
fn load_dynamic_resources_happy_path() {
    // Write a CDS file to a temp dir; bootstrap points at it; after load,
    // all_clusters() yields the dynamic cluster and re-validation passes.
    let dir = tempfile::tempdir().unwrap();
    let cds_path = dir.path().join("cds.yaml");
    std::fs::write(&cds_path, MINIMAL_CDS).unwrap(); // the Task-2 const, 127.0.0.1 endpoint
    let yaml = bootstrap_yaml_with_cds_route(cds_path.to_str().unwrap()); // listener + route to dynamic_backend + zero static clusters
    let mut b = crate::parse_bootstrap(&yaml).unwrap();
    assert!(b.dynamic_clusters.is_none());
    crate::load_dynamic_resources(&mut b).unwrap();
    assert_eq!(b.dynamic_clusters.as_ref().unwrap().len(), 1);
    assert_eq!(b.all_clusters().count(), 1);
}

#[test]
fn load_is_noop_without_dynamic_resources() {
    let yaml = MINIMAL_ADMIN_ONLY_BOOTSTRAP; // any existing test const
    let mut b = crate::parse_bootstrap(yaml).unwrap();
    crate::load_dynamic_resources(&mut b).unwrap();
    assert!(b.dynamic_clusters.is_none()); // stays None — nothing configured
}

#[test]
fn missing_cds_file_is_fatal() {
    // L4a reconciliation: missing file → CdsFileError (envoy-bin exits).
    let yaml = bootstrap_yaml_with_cds_route("/nonexistent/cds.yaml");
    let mut b = crate::parse_bootstrap(&yaml).unwrap();
    let err = crate::load_dynamic_resources(&mut b).unwrap_err();
    assert!(matches!(err, crate::ConfigError::CdsFileError { .. }));
}

#[test]
fn malformed_cds_file_is_fatal() {
    // L4b reconciliation: malformed content → CdsParseError (envoy-rust diverges
    // from Envoy's warn-and-serve; ADR-0049).
    let dir = tempfile::tempdir().unwrap();
    let cds_path = dir.path().join("cds.yaml");
    std::fs::write(&cds_path, "resources: [unclosed").unwrap();
    let yaml = bootstrap_yaml_with_cds_route(cds_path.to_str().unwrap());
    let mut b = crate::parse_bootstrap(&yaml).unwrap();
    assert!(crate::load_dynamic_resources(&mut b).is_err());
}

#[test]
fn static_dynamic_collision_static_wins() {
    // L9 reconciliation (ADR-0049): the dynamic duplicate is SKIPPED (no error).
    let dir = tempfile::tempdir().unwrap();
    let cds_path = dir.path().join("cds.yaml");
    std::fs::write(&cds_path, MINIMAL_CDS).unwrap(); // defines dynamic_backend → port 8124
    // Bootstrap that ALSO statically defines dynamic_backend → port 8123:
    let yaml = bootstrap_yaml_with_static_and_cds("dynamic_backend", 8123, cds_path.to_str().unwrap());
    let mut b = crate::parse_bootstrap(&yaml).unwrap();
    crate::load_dynamic_resources(&mut b).unwrap();
    // The dynamic list is empty (skipped); all_clusters yields ONLY the static one (port 8123).
    assert_eq!(b.dynamic_clusters.as_ref().unwrap().len(), 0);
    assert_eq!(b.all_clusters().count(), 1);
    let port = /* extract the single endpoint port from all_clusters().next() */;
    assert_eq!(port, 8123);
}

#[test]
fn unresolved_route_reference_fatal_after_load() {
    // Post-merge re-validation: a route to a cluster in NEITHER list fails at
    // load_dynamic_resources (NOT deferred forever, NOT a runtime panic).
    let dir = tempfile::tempdir().unwrap();
    let cds_path = dir.path().join("cds.yaml");
    std::fs::write(&cds_path, MINIMAL_CDS).unwrap(); // defines dynamic_backend only
    let yaml = bootstrap_yaml_with_cds_route_to(cds_path.to_str().unwrap(), "no_such_cluster");
    let mut b = crate::parse_bootstrap(&yaml).unwrap();
    let err = crate::load_dynamic_resources(&mut b).unwrap_err();
    assert!(matches!(err, crate::ConfigError::UnknownCluster(ref c) if c == "no_such_cluster"));
}
```

- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config load_dynamic`. Expected: FAIL (`load_dynamic_resources` does not exist).
- [ ] **Step 3: Implement `load_dynamic_resources` in `lib.rs`.**

```rust
/// 18 D3: read + parse + merge the CDS file (ADR-0048/ADR-0049). Called by
/// envoy-bin AFTER parse_bootstrap and BEFORE any consumer iterates clusters.
/// No-op when dynamic_resources.cds_config is unconfigured. ALL failures are
/// fatal (the L4 reconciliation — envoy-rust never warn-and-serves a broken
/// CDS file; recorded divergence vs Envoy, BEHAVIOR_CONTRACT).
///
/// Deliberately NOT called by parse_bootstrap: parse_bootstrap is the fuzz
/// target and must stay pure (no file I/O).
pub fn load_dynamic_resources(bootstrap: &mut Bootstrap) -> Result<(), ConfigError> {
    let Some(cs) = bootstrap
        .dynamic_resources
        .as_ref()
        .and_then(|dr| dr.cds_config.as_ref())
    else {
        return Ok(());
    };
    let path = cs.path_config_source.path.clone();
    let contents = std::fs::read_to_string(&path)
        .map_err(|source| ConfigError::CdsFileError { path: path.clone(), source })?;
    let parsed = cds::parse_cds_file(&path, &contents)?;
    // L9 (ADR-0049): static wins on name collision — the dynamic duplicate is
    // skipped with a warning, mirroring Envoy's "skipped N unmodified cluster(s)".
    let mut dynamic = Vec::with_capacity(parsed.len());
    for cluster in parsed {
        if bootstrap.static_resources.clusters.iter().any(|c| c.name == cluster.name) {
            tracing::warn!(cluster = %cluster.name, "CDS cluster collides with a static cluster; static wins (skipped)");
            continue;
        }
        // Intra-file duplicates: last-write-wins would be silent; reject loudly instead
        // (no Envoy observable either way — the file is envoy-rust's own input here).
        if dynamic.iter().any(|c: &Cluster| c.name == cluster.name) {
            tracing::warn!(cluster = %cluster.name, "duplicate cluster in CDS file; first wins (skipped)");
            continue;
        }
        dynamic.push(cluster);
    }
    bootstrap.dynamic_clusters = Some(dynamic);
    // Post-merge re-validation: cds_configured_but_unloaded() is now false, so
    // the deferred cluster-reference checks (UnknownCluster + the H2-from-H1
    // gate) re-run with full enforcement against all_clusters().
    bootstrap::validate(bootstrap)?;
    Ok(())
}
```

NOTE: `tracing` is NOT currently a dependency of `envoy-config` — check `crates/envoy-config/Cargo.toml`; if absent, either add it (it is a permitted foundation, already in the workspace dep tree) or use `eprintln!`-free silent skip + a returned report struct. **Preferred: add `tracing` to envoy-config's deps** (one line; the warn is operator-relevant). The implementer verifies `cargo build -p envoy-config` standalone after (the `project_isolated_crate_build_blindspot` discipline).

**Step 3b — the validate() cluster-reference checks must consult `all_clusters()`** (not `static_resources.clusters`) so the re-validation pass resolves dynamic references: the tcp_proxy check (`:2013-2020`) and `validate_hcm`'s call site change from `&bootstrap.static_resources.clusters` to a collected `Vec<&Cluster>` from `all_clusters()` (or the slice parameter becomes `&[&Cluster]`). The Task-1 deferral conjunct stays.

**Step 3c — consumer migration** (each is a 1-3 line change; existing tests prove behavior preservation since `dynamic_clusters` is `None`/empty everywhere except the new tests):
- `envoy-bin/src/main.rs:51`: after `parse_bootstrap`, insert `envoy_config::load_dynamic_resources(&mut bootstrap)?;` (requires `let mut bootstrap = ...` before the `Arc::new` wrap — restructure to parse → load → `Arc::new`).
- `envoy-bin/src/main.rs:191` (TLS loop): `for cluster in &bootstrap.static_resources.clusters` → `for cluster in bootstrap.all_clusters()`.
- `envoy-cluster/src/cluster.rs` `from_bootstrap`: the cluster iteration → `bootstrap.all_clusters()`.
- `envoy-http1/src/pool.rs:451` + `envoy-http2/src/pool.rs` (the same site in `for_bootstrap`): → `bootstrap.all_clusters()`.
- `envoy-health/src/scheduler.rs:47`: → `bootstrap.all_clusters()`.
- (`OutlierManager` iterates the ClusterManager — no change.)

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-config && cargo test --workspace` (the migration touches 5 crates — full workspace test). Expected: PASS. Also run the 4 standalone builds: `cargo build -p envoy-config -p envoy-cluster -p envoy-http1 -p envoy-http2`.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-config crates/envoy-bin/src/main.rs crates/envoy-cluster/src/cluster.rs crates/envoy-http1/src/pool.rs crates/envoy-http2/src/pool.rs crates/envoy-health/src/scheduler.rs
git commit -m "phase 18 Task 3: load_dynamic_resources + effective-cluster-list merge + all_clusters() consumer migration [ADR-0049]"
```

---

### Task 4: `cluster_manager.*` stats (conditional registration)

**Files:**
- Modify: `crates/envoy-cluster/src/cluster.rs` (`from_bootstrap` — the manager-level stat registration)
- Test: `crates/envoy-cluster/src/cluster.rs` test module

- [ ] **Step 1: Write failing tests.** (a) **conditional registration (the §5.2 inertness invariant):** `from_bootstrap` on a bootstrap WITHOUT `dynamic_resources` → NO stat whose name starts with `cluster_manager.` exists in the registry (scrape and assert absence); (b) **the 6-stat subset on a CDS bootstrap:** `from_bootstrap` on a bootstrap with `dynamic_resources` + `dynamic_clusters: Some(vec![one cluster])` (constructed directly in the test — no file I/O needed at this layer) → `cluster_manager.cds.update_attempt == 1`, `cds.update_success == 1`, `cds.update_failure == 0`, `cds.update_rejected == 0`, `cluster_added == <total cluster count>`, `active_clusters == <total cluster count>`; (c) **counts include static clusters:** a bootstrap with 1 static + 1 dynamic cluster → `cluster_added == 2`, `active_clusters == 2` (L3: Envoy counts ALL clusters added to the manager — bilateral on fixture 0026 because it has zero static).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-cluster cluster_manager_stats`. Expected: FAIL.
- [ ] **Step 3: Implement.** In `from_bootstrap` (`cluster.rs:721+`), after the per-cluster construction loop:

```rust
// 18 D4 (ADR-0049 L3/L10): the cluster_manager.* stat family — the project's
// first top-level-scope (non-prefixed-by-resource-name) stat family. Registered
// ONLY when dynamic_resources.cds_config is configured (the §5.2 conditional-
// registration discipline; Envoy emits the base cluster_manager.* names
// unconditionally — those stay Envoy-only-unasserted on non-CDS fixtures).
// All failure paths are fatal pre-construction (L4 reconciliation), so
// update_failure / update_rejected register at 0 and never tick.
if bootstrap
    .dynamic_resources
    .as_ref()
    .and_then(|dr| dr.cds_config.as_ref())
    .is_some()
{
    let total = clusters.len() as i64; // the merged map size (static + dynamic)
    registry.register_counter("cluster_manager.cds.update_attempt")?.add(1);
    registry.register_counter("cluster_manager.cds.update_success")?.add(1);
    registry.register_counter("cluster_manager.cds.update_failure")?; // 0
    registry.register_counter("cluster_manager.cds.update_rejected")?; // 0
    let added = registry.register_counter("cluster_manager.cluster_added")?;
    added.add(total.try_into().unwrap_or(0));
    registry.register_gauge("cluster_manager.active_clusters")?.set(total);
}
```

(Adapt to the actual `register_counter`/`register_gauge` error-mapping pattern used at `:817-839` — `StatsRegistration` ClusterError wrapping. `Counter::add(n)`/`Gauge::set(n)` signatures per `crates/envoy-stats/src/registry.rs:45-90`.)

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-cluster` (full). Expected: PASS. Existing fixtures' inertness is structural: no existing fixture/bootstrap configures `dynamic_resources`, so the registration block never runs for them.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-cluster --all-targets --all-features -- -D warnings
git add crates/envoy-cluster/src/cluster.rs
git commit -m "phase 18 Task 4: conditional cluster_manager.* stat family (6-name CDS subset) [ADR-0049]"
```

---

### Task 5: `/config_dump` `ClustersConfigDump` entry (conditional emission)

**Files:**
- Modify: `crates/envoy-admin/src/endpoint.rs` (`ConfigDumpEntry` enum `:301-309` + the renderer that builds the `configs` array)
- Test: `crates/envoy-admin/src/` test module (mirror the existing BootstrapConfigDump rendering tests)

- [ ] **Step 1: Write failing tests.** (a) **conditional emission:** a bootstrap WITHOUT `dynamic_resources` → `/config_dump` renders exactly ONE entry (BootstrapConfigDump; fixture-0014 regression shape); (b) **the entry with dynamic clusters:** a bootstrap with `dynamic_resources` + `dynamic_clusters: Some(vec![cluster "dynamic_backend"])` → `configs` has TWO entries; `configs[1]["@type"] == "type.googleapis.com/envoy.admin.v3.ClustersConfigDump"`; `configs[1]["dynamic_active_clusters"][0]["cluster"]["name"] == "dynamic_backend"`; `configs[1]["dynamic_active_clusters"][0]["cluster"]["@type"] == "type.googleapis.com/envoy.config.cluster.v3.Cluster"`; `configs[1]["dynamic_active_clusters"][0]["last_updated"]` parses as ISO-8601; (c) **empty-key omission (L5):** zero static clusters → NO `static_clusters` key in the entry JSON (serde `skip_serializing_if`); (d) **the BootstrapConfigDump shows `dynamic_resources` but NOT the loaded clusters** (§5.5 separation — `dynamic_clusters` is `#[serde(skip)]`, structurally guaranteed; assert the BootstrapConfigDump JSON has a `dynamic_resources` key and its `static_resources` has no clusters).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-admin config_dump`. Expected: FAIL.
- [ ] **Step 3: Implement.** Extend `ConfigDumpEntry` (`endpoint.rs:301-309`):

```rust
#[derive(Serialize)]
#[serde(tag = "@type")]
pub(crate) enum ConfigDumpEntry<'a> {
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump")]
    Bootstrap {
        bootstrap: &'a envoy_config::Bootstrap,
        last_updated: String,
    },
    /// 18 D5 (ADR-0049 L5): emitted ONLY when dynamic_resources is configured
    /// (fixture 0014 untouched — its config_dump stays single-entry). Keys
    /// mirror Envoy's proto3-JSON shape: empty lists are omitted entirely.
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.ClustersConfigDump")]
    Clusters {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        static_clusters: Vec<StaticClusterEntry<'a>>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        dynamic_active_clusters: Vec<DynamicClusterEntry<'a>>,
    },
}

/// One static cluster inside ClustersConfigDump (Envoy shape: {"cluster": {...}}).
#[derive(Serialize)]
pub(crate) struct StaticClusterEntry<'a> {
    pub(crate) cluster: TaggedCluster<'a>,
}

/// One dynamically-loaded cluster (Envoy shape: {"cluster": {...}, "last_updated": "..."}).
#[derive(Serialize)]
pub(crate) struct DynamicClusterEntry<'a> {
    pub(crate) cluster: TaggedCluster<'a>,
    pub(crate) last_updated: String,
}

/// A Cluster serialized with the inner @type tag Envoy's Any-projection carries.
#[derive(Serialize)]
pub(crate) struct TaggedCluster<'a> {
    #[serde(rename = "@type")]
    pub(crate) type_url: &'static str, // "type.googleapis.com/envoy.config.cluster.v3.Cluster"
    #[serde(flatten)]
    pub(crate) cluster: &'a envoy_config::Cluster,
}
```

The `/config_dump` renderer (the site that today builds `vec![ConfigDumpEntry::Bootstrap {...}]` — find it in `endpoint.rs`/`handler.rs`): when `self.bootstrap.dynamic_resources.as_ref().and_then(|dr| dr.cds_config.as_ref()).is_some()`, push a `Clusters` entry populated from `self.bootstrap.static_resources.clusters` (static_clusters) + `self.bootstrap.dynamic_clusters.iter().flatten()` (dynamic_active_clusters), with `last_updated` from the same ISO-8601 source the Bootstrap entry uses. The entry is pushed AFTER the Bootstrap entry → `configs[1]` (matching Envoy's order, L5).

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-admin` (full — the fixture-0014-shape regression tests must stay green). Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-admin --all-targets --all-features -- -D warnings
git add crates/envoy-admin/src/
git commit -m "phase 18 Task 5: ClustersConfigDump config_dump entry (conditional emission) [ADR-0049]"
```

---

### Task 6: Harness — CDS-file rendering/mounting + the `admin_scrapes` keep-alive extension

**Files:**
- Modify: `tests/differential/src/lib.rs` (template detection `:2117-2130`; render/write `:2384-2387`; `Driver::Http1KeepAlive` `:167-180`; the keep-alive dispatch arm)
- Modify: `tests/differential/src/upstream.rs` (`start` signature + the `with_copy_to` block `:104-113`)
- Test: `tests/differential/src/lib.rs` test module (the render-path unit tests; the Docker-gated end-to-end proof is Task 7)

- [ ] **Step 1: Write failing unit tests.** (a) a fixture template containing `{{CDS_PATH}}` + a fixture dir containing `cds.yaml` → `run_fixture`'s pre-flight produces per-side rendered CDS files where the upstream side's substitutions use the container-perspective backend host and the subject side's use the host perspective (test the render step in isolation — mirror the existing `run_fixture_dispatches_http1_backend_on_template_marker` test pattern `lib.rs:4312+`); (b) the upstream-side `{{CDS_PATH}}` substitution value is a container path ending in `.yaml` (the L1 extension constraint); the subject-side value is the host temp path; (c) `Driver::Http1KeepAlive` with an `admin_scrapes:` list deserializes (serde round-trip test; default = empty for existing fixtures).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p differential cds`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - **CDS detection + rendering:** in `run_fixture`, alongside the existing template scans (`:2117-2130`): `let needs_cds = upstream_template.contains("{{CDS_PATH}}") || subject_template.contains("{{CDS_PATH}}");`. When true, read `fixture_dir.join("cds.yaml")`, render it twice via the existing `render_yaml` with each side's existing kv map (backend host/port substitution), and write both to the temp dir (`write_temp(tmp.path(), "cds-upstream.yaml", ...)` / `"cds-subject.yaml"`).
  - **Upstream side:** add the rendered upstream CDS file to the Envoy container via `with_copy_to("/etc/envoy-cds/cds.yaml", host_path)` (extend `upstream::start` with an `Option<PathBuf>` cds parameter — mirror how `tls_pki` is threaded at `:104-113`); substitute `{{CDS_PATH}}` → `/etc/envoy-cds/cds.yaml` in the upstream main config's kv map. **The container path MUST end in `.yaml`** (L1).
  - **Subject side:** substitute `{{CDS_PATH}}` → the host temp path of `cds-subject.yaml` in the subject kv map.
  - **`admin_scrapes` extension:** `Driver::Http1KeepAlive` gains `#[serde(default)] admin_scrapes: Vec<AdminScrapeCase>`. In the keep-alive dispatch arm (after the existing `expected_stats` scrape loop), iterate `admin_scrapes` and run each case through the SAME per-case assertion function the `Driver::AdminScrape` arm uses (extract/reuse that function — do not duplicate the json_shape diff logic). Existing fixtures (0020–0025) deserialize unchanged (`#[serde(default)]`).
- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p differential` (non-Docker unit tests). Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p differential --all-targets --all-features -- -D warnings
git add tests/differential/src/
git commit -m "phase 18 Task 6: harness CDS-file rendering/mounting ({{CDS_PATH}}) + Http1KeepAlive admin_scrapes [ADR-0049]"
```

---

### Task 7: Fixture `0026-xds-file-based-cds` + Docker-gated wrapper

**Files:**
- Create: `tests/fixtures/0026-xds-file-based-cds/{envoy.yaml,envoy-rust.yaml,cds.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/xds_file_based_cds.rs`

- [ ] **Step 1: Write the Docker-gated wrapper** (mirror `tests/differential/tests/upstream_circuit_breaker_budgets.rs`):

```rust
#[tokio::test]
async fn xds_file_based_cds_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..").join("..")
        .join("tests/fixtures/0026-xds-file-based-cds");
    differential::run_fixture(&dir).await.expect("fixture passes");
}
```

The backend is the existing `http1-echo-server` helper (the fixture-0008 backend — `{{HTTP1_BACKEND_PORT}}` template marker triggers its launch; no new helper, no stateful backend needed).

- [ ] **Step 2: Write the fixture configs.** `envoy.yaml` + `envoy-rust.yaml` (IDENTICAL content — the fixture principle):

```yaml
# Fixture 0026: file-based CDS (phase 18, ADR-0048/ADR-0049). The listener +
# route are static; the cluster `dynamic_backend` exists ONLY in the CDS file
# ({{CDS_PATH}} — rendered + mounted per side by the harness, Task 6).
# L12a: node.id/node.cluster REQUIRED by Envoy when CDS is configured.
# L12b: validate_clusters: false REQUIRED on the route_config (Envoy's inline
#       route validation runs against the static cluster set only).
node: { id: envoy-rust-phase-18-fixture-0026, cluster: envoy-rust-phase-18 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: {{ADMIN_PORT}} } } }
dynamic_resources:
  cds_config:
    resource_api_version: V3
    path_config_source:
      path: {{CDS_PATH}}
static_resources:
  listeners:
  - name: ingress_http
    address:
      socket_address: { address: 0.0.0.0, port_value: {{PORT}} }
    filter_chains:
    - filters:
      - name: envoy.filters.network.http_connection_manager
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
          stat_prefix: ingress_http
          route_config:
            name: local_route
            validate_clusters: false
            virtual_hosts:
            - name: backend
              domains: ["*"]
              routes:
              - match: { prefix: "/" }
                route: { cluster: dynamic_backend }
          http_filters:
          - name: envoy.filters.http.router
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
```

(NO `clusters:` key under static_resources — L7. The exact HCM/listener YAML shape MUST be copied from fixture 0008's `envoy.yaml` — the proven wire shape — with only the route_config/cluster-reference differences above; the implementer diffs against 0008 to ensure no drift in the boilerplate.)

`cds.yaml` (the shared template, rendered per side by Task 6):

```yaml
# The CDS file (L1: the bare `resources:` envelope, @type-tagged Cluster payloads).
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: dynamic_backend
  type: STRICT_DNS
  dns_lookup_family: V4_ONLY
  load_assignment:
    cluster_name: dynamic_backend
    endpoints:
    - lb_endpoints:
      - endpoint:
          address:
            socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP1_BACKEND_PORT}} }
```

- [ ] **Step 3: Write `expectations.yaml`** (`Driver::Http1KeepAlive` + the Task-6 `admin_scrapes`):

```yaml
# Fixture 0026: file-based CDS — the xDS-family opener (phase 18).
# Three bilateral observables (SPEC §1 + ADR-0048 finding 3):
#   1. Data plane: GET / through the dynamically-loaded cluster → 200 + echo
#      body (the fixture-0008 wire shape). A proxy that ignored
#      dynamic_resources would 503 (no such cluster) — the load-bearing probe.
#   2. Stats: the 6-name cluster_manager subset, value-exact (L3).
#   3. /config_dump: the ClustersConfigDump entry carries the dynamic cluster
#      (L5; via the Task-6 admin_scrapes extension).
driver:
  kind: http1_keep_alive
  requests:
    - method: GET
      path: /
      host: dynamic_backend
      expected_status: 200
      # The http1-echo-server body is request-dependent (it echoes the request
      # line); both proxies forward the identical request → identical echo body.
      # Byte-exactness is enforced by the harness's bilateral body comparison;
      # no expected_body literal is pinned here (the fixture-0008 posture).
  settle_ms: 200
  expected_stats:
    # L3: the 6-name cluster_manager subset (conditional registration; bilateral
    # value-exact on this all-dynamic topology).
    - { name: cluster_manager.cds.update_attempt,   value: 1 }
    - { name: cluster_manager.cds.update_success,   value: 1 }
    - { name: cluster_manager.cds.update_failure,   value: 0 }
    - { name: cluster_manager.cds.update_rejected,  value: 0 }
    - { name: cluster_manager.cluster_added,        value: 1 }
    - { name: cluster_manager.active_clusters,      value: 1 }
    # The dynamic cluster behaves as a full Cluster (SPEC §5.3): the standard
    # per-cluster + HCM counters fire exactly as for a static cluster.
    - { name: cluster.dynamic_backend.upstream_rq_total,  value: 1 }
    - { name: cluster.dynamic_backend.upstream_cx_total,  value: 1 }
    - { name: http.ingress_http.downstream_rq_total,      value: 1 }
    - { name: http.ingress_http.downstream_rq_2xx,        value: 1 }
  admin_scrapes:
    # L5: the ClustersConfigDump bilateral assertion (the Task-6 extension).
    # configs[0] = BootstrapConfigDump, configs[1] = ClustersConfigDump on BOTH
    # sides (Envoy's documented order; envoy-rust emits exactly these two).
    - path: /config_dump
      expected_status: 200
      expected_content_type: "application/json"
      expected_body_rule:
        kind: json_shape
        required_keys: ["configs"]
        required_subtree:
          path: "configs.1.dynamic_active_clusters.0.cluster.name"
          expected: "dynamic_backend"
        value_may_differ_keys: ["configs"]
        allowlist_envoy_only_keys: []
        allowlist_envoy_rust_only_keys: []
```

NOT asserted (with a README explanation): the Envoy-only `cluster_manager.*` siblings (L3 enumeration — `cds.version`, `cds.update_time`, `warming_clusters`, …; ignored by the named-stat scrape); `configs.1.@type` (implied by the deeper `dynamic_active_clusters` subtree assertion); the per-request `x-envoy-upstream-service-time` value (name-presence is part of the standard bilateral header comparison; value differs per the existing Header allow-list row).

- [ ] **Step 4: Write `README.md`** — the fixture's purpose (the xDS-family opener; what the probe discriminates), the L12a/L12b prerequisites (`node:` + `validate_clusters: false` — why they are in the config), the L1 envelope-shape + `.yaml`-extension constraint, the L4 negative-path divergence pointer (backstop-only; ADR-0049), and the L3 Envoy-only stat enumeration.
- [ ] **Step 5: Run** (Docker-gated). Run: `cargo test -p differential --test xds_file_based_cds -- --nocapture` (with Docker available). Expected: PASS bilaterally. If any assertion diverges, the REAL ENVOY value is the source of truth (D-3.3) — adjust envoy-rust/the assertion via `superpowers:systematic-debugging`, NOT by loosening the expectation silently.
- [ ] **Step 6: Run the regression suite** (the other 25 fixtures; Docker-gated; never concurrently with cargo builds per the CI-flake memory). Expected: all green (the CDS machinery is inert for them — no existing fixture configures `dynamic_resources`; the harness extension is `#[serde(default)]`).
- [ ] **Step 7: commit.**

```bash
git add tests/fixtures/0026-xds-file-based-cds/ tests/differential/tests/xds_file_based_cds.rs
git commit -m "phase 18 Task 7: fixture 0026-xds-file-based-cds + Docker wrapper [ADR-0049]"
```

---

### Task 8: In-process backstop (happy path + negative paths + collision)

**Files:**
- Create: `crates/envoy-bin/tests/xds_file_based_cds.rs`

- [ ] **Step 1: Write the backstop** (mirror the `crates/envoy-bin/tests/admin_only.rs` + `upstream_circuit_breaker_budgets.rs` pattern: temp config + `tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))` + `kill_on_drop(true)` + `wait_ready` + raw-TcpStream HTTP). Five test paths:
  - **(i) happy path:** temp dir with `cds.yaml` (the dynamic_backend cluster → an in-process TCP echo backend) + a bootstrap with `dynamic_resources` pointing at it + the static listener/route (validate_clusters: false) + zero static clusters → boot → data-plane `GET /` returns 200 with the backend body → admin `/stats` shows the 6 cluster_manager names at 1/1/0/0/1/1 → admin `/config_dump` JSON has `configs[1]["@type"] == ".../ClustersConfigDump"` and `dynamic_active_clusters[0].cluster.name == "dynamic_backend"`.
  - **(ii) missing CDS file (L4a):** bootstrap points at a nonexistent path → the process EXITS non-zero before binding listeners (assert `child.wait()` completes with failure within a budget AND the stderr contains the CdsFileError message; the listener port never accepts).
  - **(iii) malformed CDS file (L4b reconciliation):** the file contains `resources: [unclosed` → same fatal-exit assertion (envoy-rust diverges from Envoy's warn-and-serve here; this test IS the recorded-divergence proof).
  - **(iv) static/dynamic collision (L9):** bootstrap defines `dynamic_backend` statically (→ backend A) AND the CDS file defines it (→ backend B) → boot succeeds (no error), data-plane GET returns backend A's body (static wins), `/config_dump`'s ClustersConfigDump shows `static_clusters` containing it and `dynamic_active_clusters` empty/absent.
  - **(v) inertness:** a bootstrap WITHOUT `dynamic_resources` → `/stats` contains NO `cluster_manager.` names; `/config_dump` has exactly 1 entry (the fixture-0014 regression shape).
- [ ] **Step 2: Run, iterate to pass.** Run: `cargo test -p envoy-bin --test xds_file_based_cds`. Expected: PASS (all 5 paths).
- [ ] **Step 3: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-bin/tests/xds_file_based_cds.rs
git commit -m "phase 18 Task 8: in-process CDS backstop (happy + fatal negative paths + static-wins collision)"
```

---

### Task 9: Fuzz seed `dynamic_resources_cds.yaml` (corpus 28 → 29)

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/dynamic_resources_cds.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (allow-list entry)
- Modify: `crates/envoy-config/src/bootstrap.rs` (the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array `:3895-3917`)

- [ ] **Step 1: Write the seed** — a complete bootstrap exercising the new schema surface: `node` + `admin` + `dynamic_resources.cds_config.{resource_api_version: V3, path_config_source.path}` + an HCM listener with `validate_clusters: false` + a route to a CDS-deferred cluster + zero static clusters (the fixture-0026 shape, minus template markers — use literal port values). This seed exercises the deferral path (`parse_bootstrap` accepts it; the fuzz target never calls `load_dynamic_resources`, so no file I/O).
- [ ] **Step 2: The atomic three-edit** (the 09→16 lesson — seed file + `.gitignore` allow-list + SUCCESS array land together): add `!corpus/parse_bootstrap/dynamic_resources_cds.yaml` to the `.gitignore`; add `"fuzz/corpus/parse_bootstrap/dynamic_resources_cds.yaml"` to the SUCCESS array. **Opportunistic fix (pre-existing inconsistency found at this PLAN-write):** `cluster_http2_protocol_options.yaml` is in the `.gitignore` allow-list but missing from the SUCCESS array — add it to the array too (verify it parses first; if it doesn't, leave it and record in PROGRESS).
- [ ] **Step 3: Run the corpus gate.** Run: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly`. Expected: PASS.
- [ ] **Step 4: Short-budget fuzz smoke.** Run: `cargo +nightly fuzz run parse_bootstrap -- -runs=100000 -max_total_time=60` (from `crates/envoy-config`). Expected: no crash.
- [ ] **Step 5: commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/dynamic_resources_cds.yaml crates/envoy-config/fuzz/.gitignore crates/envoy-config/src/bootstrap.rs
git commit -m "phase 18 Task 9: fuzz seed dynamic_resources_cds.yaml (corpus 28->29)"
```

---

### Task 10: BEHAVIOR_CONTRACT extensions (stat rows + xDS-section first population + admin-body-shapes row)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`

- [ ] **Step 1: Add the "18 entries (file-based CDS)" stat rows** under "Stat-name mapping": the 6-name table from §6.2 lock-in L3 (each row: kind, value-exact on fixture 0026, conditional registration when `dynamic_resources.cds_config` configured [the §5.2 narrowing vs Envoy's unconditional base names — recorded explicitly], registration site `ClusterManager::from_bootstrap`, the L4 note that update_failure/update_rejected are structurally 0 in envoy-rust because all load errors are fatal). Plus the L3 Envoy-only enumeration paragraph (the 12 unasserted names).
- [ ] **Step 2: First-populate the "xDS wire state machine" section** (replacing the `_(empty; populated when xDS family begins)_` placeholder) with a "Filesystem transport (`path_config_source`) — phase 18" subsection recording: **(a)** the L1 envelope shape (the byte-exact minimal working file; @type required; Envoy's extension-driven parser selection vs envoy-rust's always-YAML leniency); **(b)** the L2 initial-load semantics (readiness implies loaded on both proxies; Envoy's init ordering log evidence); **(c)** the L4 negative-path disposition table (Envoy's 3-way split vs envoy-rust's all-fatal posture — the recorded divergence, per ADR-0049); **(d)** the L9 static-wins collision rule (both proxies); **(e)** the L12 bootstrap prerequisites (node.id/cluster + validate_clusters: false); **(f)** an explicit note that the gRPC/ADS message-sequence state machine remains unpopulated (deferred to the gRPC-xDS phase, which also supersedes ADR-0014).
- [ ] **Step 3: Add the `/config_dump` ClustersConfigDump row** to "Admin endpoint body shapes": the L5 JSON shape; conditional emission (only when `dynamic_resources` configured); the empty-key-omission rule; `last_updated` name-required-value-may-differ; the fixture-0026 `configs.1.dynamic_active_clusters.0.cluster.name` bilateral anchor.
- [ ] **Step 4: commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 18 Task 10: BEHAVIOR_CONTRACT CDS stat rows + xDS filesystem-transport section + ClustersConfigDump row [ADR-0049]"
```

---

### Task 11: State-4 phase-done verification + STATE advance to state-5-next

**Files:**
- Modify: `docs/envoy-rust/phases/18-xds-file-based-cds/PROGRESS.md`; `docs/envoy-rust/STATE.md`

- [ ] **Step 1: Run the full §7.5 gate suite** and quote each into PROGRESS (the 05.3→17 evidence discipline): `cargo build --workspace --all-targets`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo fmt --all -- --check`; `cargo test --workspace`; `cargo deny check`; the short-budget `parse_bootstrap` fuzz run; the Docker-gated differential suite (**all 26 fixtures `0001`–`0026` green simultaneously**; never run concurrently with cargo builds per the CI-flake memory); the `h2spec` ≥95% gate (CI-anchored).
- [ ] **Step 2: Run the standalone-crate builds** (`project_isolated_crate_build_blindspot` / SPEC §6.7): `cargo build -p envoy-config`, `-p envoy-cluster`, `-p envoy-http1`, `-p envoy-http2` — quote each in PROGRESS. (envoy-config is the named risk this phase: it gained file I/O + possibly a `tracing` dep.)
- [ ] **Step 3: Quote per-gate evidence** in PROGRESS (CI run URL + HEAD SHA + completion timestamp + per-gate output).
- [ ] **Step 4: Advance STATE.md** to `18` state-4-complete / state-5-next (Next expected skill → `superpowers:requesting-code-review` over the phase-18 commit range). Commit.

```bash
git add docs/envoy-rust/phases/18-xds-file-based-cds/PROGRESS.md docs/envoy-rust/STATE.md
git commit -m "phase 18 Task 11: state-4 phase-done verification + STATE advance to state-5-next [ADR-0049]"
```

> **State 5 (code review → REVIEW.md) and State 6 (close-out: §5.3-format commit + flip ROADMAP row 18 in-progress→done + STATE → awaiting next planning) are LATER sessions** per §5.1 one-state-per-session. Phase 18 is a non-split top-level phase → it flips its OWN row alone at state 6. **Named state-5 review focus:** (1) the deferred-validation soundness — can ANY config path reach a runtime cluster-lookup miss (the `.expect` at `bootstrap.rs:2214` and the HCM dispatch lookups)? The invariant: `load_dynamic_resources` re-validation runs before any consumer is constructed; (2) the all_clusters() migration completeness — did every `static_resources.clusters` iteration site migrate (grep for stragglers)?; (3) the §5.2 inertness — zero new stats/config_dump entries/behavior on the 25 pre-existing fixtures; (4) the L4 fatal-error posture consistency (no partial-load state can leak into a serving process); (5) the harness admin_scrapes extension's backward compatibility (fixtures 0020–0025 byte-untouched).

---

## Self-review

- **Spec coverage:** D1 → Task 1; D2 → Task 2; D3 → Task 3; D4 → Task 4; D5 → Task 5; D6 → Task 6; D7 → Task 7; D8 → Tasks 8+9+10; state-4 → Task 11. SPEC §2.1 (stat subset) → L3/Task 4; §2.2 (xDS section) → Task 10; §5.1–§5.6 invariants → Tasks 1/3 (5.3, 5.4), 4 (5.2), 5 (5.5), 7 (5.6); §6.3 (backstop both-paths) → Task 8; §6.7 (standalone builds) → Task 11; §6.8 (no new dep) → confirmed (tracing is the only candidate addition, already a workspace foundation). All D1–D8 covered.
- **SPEC deltas locked by §6.2 (ADR-0049):** the D1 `DuplicateClusterName` variant DROPPED (L9 static-wins); the D1 `UnsupportedDynamicResourceType` variant unnecessary (deny_unknown_fields covers it — the SPEC anticipated this collapse); the D3 async/tokio::fs projection → sync std::fs; the D5 `version_info` key dropped from the dump entry (L5); the SPEC §2.1 4-name subset → 6 names (adds update_attempt/update_rejected); RouteConfiguration gains validate_clusters (L12b — absent from the SPEC entirely); the fixture topology gains `node:` + `validate_clusters: false` (L12).
- **Type consistency:** `DynamicResources`/`ConfigSource`/`PathConfigSource` defined in Task 1, consumed in Tasks 2/3/4/5/8; `Bootstrap::all_clusters()`/`cds_configured_but_unloaded()` defined in Task 1, consumed in Tasks 3/4/5; `parse_cds_file(path, contents)` defined in Task 2, called in Task 3; `load_dynamic_resources` defined in Task 3, called by envoy-bin (Task 3) and the backstop (Task 8); `AdminScrapeCase` reused (not redefined) in Task 6; ConfigError variants (`UnsupportedResourceApiVersion`, `CdsFileError`, `CdsParseError`) defined in Task 1, raised in Tasks 1/2/3.
- **No placeholders:** every code step shows code or names the exact existing pattern to mirror with file:line anchors verified at this PLAN-write.
- **Regression guard:** inertness is structural — no existing fixture configures `dynamic_resources` (the conditional registration/emission never engages); the harness `admin_scrapes` field is `#[serde(default)]`; the all_clusters() migration is identity-preserving when `dynamic_clusters` is None. The Docker fixture (Task 7 step 6) re-proves all 25 pre-existing fixtures.
- **Fuzz-purity guard:** `parse_bootstrap` (the fuzz target) never does file I/O — `load_dynamic_resources` is a separate entry point only envoy-bin and tests call. The new seed exercises the schema + deferral path only.
