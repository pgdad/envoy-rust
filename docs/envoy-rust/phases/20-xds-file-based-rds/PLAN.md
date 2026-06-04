# Phase 20 (`20-xds-file-based-rds`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (the project default per `feedback_execution_style`) to implement this plan task-by-task, SERIALLY (per `feedback_serial_subagent_dispatch` — never dispatch implementers in parallel; they race on shared `main`). Steps use checkbox (`- [ ]`) syntax for tracking. TDD per task (`superpowers:test-driven-development` — tests first). Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` **PER TASK** (per `project_state3_arc_skips_clippy` — the per-task verification otherwise runs build/test/fmt but NOT clippy, and lints would first surface at the state-4 gate). One code commit + one PROGRESS commit per task.

**Goal:** Make file-based RDS work end-to-end — an HCM that configures `rds.config_source.path_config_source.path` (instead of an inline `route_config`) loads its `RouteConfiguration` from a YAML file at startup, routes data-plane traffic through it, and exposes the load via the per-HCM `http.<stat_prefix>.rds.<route_config_name>.*` stats + the `/config_dump` `RoutesConfigDump` section — bilaterally verified against upstream Envoy by fixture `0028-xds-file-based-rds`.

**Architecture:** Reuse the phase-18/19 filesystem-transport machinery (the `ConfigSource`/`PathConfigSource` structs, the `@type`-tagged envelope parser, the `load_dynamic_resources` config-load-time merge, the conditional-registration technique, the harness dynamic-file rendering/mounting). The three genuinely-new surfaces: (1) the HCM `route_config` XOR `rds` schema surgery (`route_config` becomes `Option`, a new sibling `rds: Option<Rds>`, validated exactly-one-of) — the FIRST HCM-scoped dynamic resource; (2) the per-HCM `http.<stat_prefix>.rds.<route_config_name>.*` stat topology (registered per-HCM, distinct from the manager-level CDS/LDS singletons); (3) the effective-`route_config` threading (`load_dynamic_resources` walks every HCM, name-selects its RouteConfiguration from the RDS file, and populates `route_config` so downstream HCM dispatch sees a uniform shape — no runtime route-table mutability, no locks, no watch tasks). Initial-load-only, synchronous, deterministic, zero timing sensitivity.

**Tech Stack:** Rust (stable, pinned). `serde`/`serde_yaml` (config parsing — existing). `std::fs` (sync file read — existing). `envoy-stats` (counters — existing). No new crate, no new top-level Cargo dep, no new harness driver, no new helper binary, no concurrency machinery.

---

## §6.2 empirical lock-ins (verified against `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, darwin/Docker, 2026-06-04; **reconciliation ADR-0052 FIRES — item 5 `configs[]` ordering diverged**)

The full statement is **ADR-0052** (DECISIONS.md). Summary of the lock-ins the tasks below depend on:

- **L1 (envelope / `rds`-on-HCM shape) — MATCH.** Both bare `resources:` and full `DiscoveryResponse` accepted; per-resource `@type: type.googleapis.com/envoy.config.route.v3.RouteConfiguration`. HCM block: `rds: { route_config_name: <name>, config_source: { path_config_source: { path: <p> } } }`; `resource_api_version` OPTIONAL. Mirrors CDS/LDS exactly.
- **L2 (readiness ordering) — MATCH.** RDS route table active before `/ready` 200; no warm-up. envoy-rust mirrors via synchronous load.
- **L3 (per-HCM stat names/values) — MATCH, `config_reload` IN the subset.** Prefix `http.<stat_prefix>.rds.<route_config_name>.`. The 5-name subset after a successful initial load: `update_attempt: 1`, `update_success: 1`, `update_failure: 0`, `update_rejected: 0`, **`config_reload: 1`**. Envoy-only (NOT asserted): `version`, `version_text`, `update_time`, `config_reload_time_ms`, `update_empty`, `init_fetch_timeout`, `update_duration`.
- **L4 (missing/malformed/semantic disposition) — MATCH (the CDS/LDS 3-way split).** Missing path = Envoy hard-exit; malformed YAML = Envoy `update_failure: 1` + serve; semantic-invalid = Envoy `update_rejected: 1` + serve. **envoy-rust = ALL FATAL** (the ADR-0049 decision-2 all-fatal posture extended to RDS) → `update_failure`/`update_rejected` register at 0, structurally unreachable non-zero; negative paths backstop-only.
- **L6 (`route_config_name` mismatch) — MATCH.** Envoy: `update_rejected: 1` + runtime 404. envoy-rust: FATAL (`RdsRouteConfigNotFound`). Backstop-only.
- **L7 (RDS→CDS composition + `validate_clusters`) — an RDS→CDS route needs NO `validate_clusters: false` (RDS behaves like LDS, not CDS-static).** envoy-rust keeps defer-then-revalidate: a route to a cluster in NEITHER list fails envoy-rust startup (`UnknownCluster`) vs Envoy's runtime-503 — the recorded narrow divergence, extended to RDS routes. The shared `rds.yaml` carries no `validate_clusters`.
- **L8 (wire shape + field tolerance) — MATCH + the RDS file is SHAREABLE.** A GET through an RDS route is shape-identical to an inline-route response. A minimal RouteConfiguration needs only `name` + vh `name`/`domains` + a route `match` + an action — all envoy-rust accepts. **Fixture 0028 uses a SINGLE shared `rds.yaml`** (rendered per-side through the kv map, like the shared `cds.yaml`), NOT per-side templates.
- **L9 (both/neither route source) — MATCH (both fatal on BOTH proxies).** Envoy: both → `oneof` reject; neither → PGV `route_specifier is required`. envoy-rust: parse-time exactly-one-of validator → `AmbiguousRouteSource` (both) / `MissingRouteSource` (neither). No differential divergence.
- **L10 (stat conditionality) — MATCH.** An inline-`route_config` HCM emits ZERO `http.<prefix>.rds.*` names. envoy-rust gates on the owning HCM's `rds.is_some()`.
- **L11 (version) — Envoy-only.** Not asserted.
- **L5 (RoutesConfigDump shape + `configs[]` ordering) — DIVERGES (the ADR-0052 trigger).** Dynamic entry: `{ "@type": ".../RoutesConfigDump", "dynamic_route_configs": [ { "route_config": { "@type": ".../RouteConfiguration", "name": "local_route", "virtual_hosts": [...] }, "last_updated": "<ts>" } ] }` — NO `version_info` in the dynamic entry. **Envoy's `configs[]` order is Bootstrap[0], Clusters[1], Listeners[2], ScopedRoutes[3], Routes[4], Secrets[5]** — `RoutesConfigDump` at index **4** (Clusters[1] + Listeners[2] NOT displaced; fixtures 0026/0027 hold). Envoy ALSO emits `RoutesConfigDump` (under `static_route_configs`) even without RDS. **Reconciliation:** envoy-rust keeps conditional emission (emit only when some HCM uses `rds`) — on fixture 0028 it lands at envoy-rust `configs[2]`; the index mismatch (envoy-rust [2] vs Envoy [4]) is bridged by a per-side `JsonSubtreeRule` path override in the harness (Task 6) used by fixture 0028's config_dump assertion (Task 7).

---

## PLAN-time SPEC corrections (verified against HEAD `a3ef29786` by a read-only `Explore` survey + controller direct-grep re-verification)

All 13 SPEC §0/§3 code anchors confirmed at HEAD with **NO drift**:

- **C1.** `HttpConnectionManagerConfig` is at `crates/envoy-config/src/bootstrap.rs:524-549`; `route_config: RouteConfiguration` (REQUIRED, non-Option) at `:547`; `stat_prefix: String` at `:527`. (SPEC anchor exact.)
- **C2.** `RouteConfiguration` at `bootstrap.rs:1017-1030` — `name`, `virtual_hosts`, `validate_clusters: Option<bool>`. (Exact.)
- **C3.** `DynamicResources` (`cds_config`/`lds_config: Option<ConfigSource>`) at `bootstrap.rs:94-102`; `ConfigSource` (`path_config_source` + `resource_api_version: Option<String>`) at `:107-115`; `PathConfigSource` (`path: String`) at `:119-123`. (Exact — `ConfigSource` is reused verbatim by `Rds`.)
- **C4.** `cds::parse_cds_file(path, contents) -> Result<Vec<Cluster>, ConfigError>` at `cds.rs:48`; the `@type`-tagged `CdsResource` enum at `:42-46`; inline per-cluster validation. (Exact.)
- **C5.** `lds::parse_lds_file(path, contents) -> Result<Vec<Listener>, ConfigError>` at `lds.rs:53`; the `@type`-tagged `LdsResource` enum at `:36-40`; NO inline validation (deferred post-merge). (Exact — `rds.rs` mirrors `lds.rs`, the closest analogue.)
- **C6.** `load_dynamic_resources(bootstrap: &mut Bootstrap) -> Result<(), ConfigError>` at `lib.rs:571`; CDS merge `:571-608`, LDS merge `:610-642`, the single post-merge `bootstrap::validate(bootstrap)?` at `:653` gated on `dynamic_clusters.is_some() || dynamic_listeners.is_some()`. Predicates: `cds_configured_but_unloaded()` `:57-63`, `lds_configured_but_unloaded()` `:79-85`, `all_listeners()` `:69-74` (read-only iterator over static + dynamic). (Exact.)
- **C7.** envoy-bin startup: `main.rs:49-50` read file → `:51` `parse_bootstrap` → `:54` `load_dynamic_resources(&mut bootstrap)?` → `:55` `Arc::new(bootstrap)`. (Exact.)
- **C8.** `HCMStats::register(registry, stat_prefix)` at `crates/envoy-http1/src/hcm.rs:76-96`; `HCMStats` struct `:36-68`. `route_config` reads: `clone_route_config(&cfg.route_config)` at `:200`, the vhost match `config.route_config.virtual_hosts.iter()...` at `:1177`, the `clone_route_config(rc: &RouteConfiguration)` helper at `:211`. (Exact.)
- **C9 (CORRECTION).** `crates/envoy-http2/src/hcm.rs` **re-exports `HCMConfig` as a type alias** and shares `HCMStats` via the shared `HCMConfig` — there is **NO separate H2 production `route_config` read site and NO separate H2 stats registration**. The H2 `route_config` usages are test-only (8 struct-literal construction sites). (SPEC implied an H2 mirror needing changes; in production only the H1 sites + the shared type need the migration.)
- **C10.** `register_lds_stats(bootstrap, registry) -> Result<(), ListenerError>` at `crates/envoy-listener/src/lib.rs:369-393` — the conditional-registration template `register_rds_stats` mirrors (early-return when unconfigured; `mk` closure; `add(1)`). (Exact.)
- **C11.** CDS conditional stat registration at `crates/envoy-cluster/src/cluster.rs:1068-1097`. (Exact.)
- **C12.** `ConfigDumpEntry<'a>` enum at `crates/envoy-admin/src/endpoint.rs:301-338` (`Bootstrap` + `Clusters` + `Listeners`, `#[serde(tag = "@type")]`); render ordering at `:467-551`: `Bootstrap` pushed unconditionally `:467`; `Clusters` pushed iff `cds_config` present `:475-504`; `Listeners` pushed iff `lds_config` present `:514-547`. The new `Routes` variant + its conditional push (iff some HCM has `rds`) go AFTER the Listeners block. (Exact.)
- **C13.** Harness: `{{CDS_PATH}}` detection/render at `tests/differential/src/lib.rs:2187-2215` (shared `cds.yaml`, rendered per-side via the kv map); `{{LDS_PATH}}` at `:2217-2251` (per-side `lds-envoy.yaml`/`lds-envoy-rust.yaml`); `scan_needs_marker`/`uses_host_gateway` at `:923-945`. `BodyRule::JsonShape` at `:534-546`; `JsonSubtreeRule { path, expected }` at `:612-617` (**NO per-side path support today** — Task 6 adds it); `walk_pointer` (dotted path, array-index/object-key) at `:624-647`; the subtree match at `:4190-4223`. (Exact.)
- **C14.** `ConfigError` enum in `crates/envoy-config/src/lib.rs`; recent variants `CdsFileError { path, source }` `:75`, `CdsParseError { path, message }` `:84`, `LdsFileError`/`LdsParseError` `:88`/`:96`. Naming convention `{Resource}{ErrorType}`. RDS variants follow: `RdsFileError { path, source }`, `RdsParseError { path, message }`, `RdsRouteConfigNotFound { name, path }`, `MissingRouteSource { stat_prefix }`, `AmbiguousRouteSource { stat_prefix }`.
- **C15 (corpus).** The git-tracked curated fuzz corpus is **30** seeds (the `Explore` arithmetic of 31 was wrong; the enumerated `crates/envoy-config/fuzz/.gitignore` list is 30, last two `dynamic_resources_cds.yaml` + `dynamic_resources_lds.yaml`). Phase 20 adds `hcm_rds_route_config.yaml` → **30 → 31** (matches SPEC §1 (d)).
- **C16 (the exactly-one-of placement decision).** The exactly-one-of check (`MissingRouteSource`/`AmbiguousRouteSource`) runs at **parse time** (in `parse_bootstrap`, before any file is read), NOT inside the post-merge re-validation. `validate()` (called both at parse and post-merge) handles `Option<route_config>`: validates the inline RouteConfiguration when `Some`, skips when `None` (an `rds` HCM pre-load). After `load_dynamic_resources` populates `route_config` for `rds` HCMs, both `route_config` AND `rds` are `Some` — the post-merge validate does NOT re-check cardinality, so the loaded state is valid and `rds.is_some()` remains the stats/config_dump predicate. (Rationale in ADR-0052; this avoids the "both-Some-after-load" false-positive.)
- **C17 (the `xds_file.rs` consolidation — DEFERRED, deliberate).** SPEC §6.11 / phase-19 REVIEW M19-1 flagged consolidating `cds.rs`+`lds.rs`+RDS into a resource-parametric `xds_file.rs` (now the 3rd sibling). **Decision: write a NEW `rds.rs` mirroring `lds.rs` (the lowest-risk, proven path); DEFER the consolidation.** Rationale: consolidation touches two currently-green modules (`cds.rs`/`lds.rs`, covered by fixtures 0026/0027) for a code-cleanliness win, against the less-margin LoC budget + the D-3.6 every-phase-green doctrine + §5.1 one-state-per-session. The consolidation stays a future hardening item (recorded in PROGRESS rollovers; M19-1 remains open at N≥3). A reviewer should read this as a risk-managed choice, not an oversight.

---

## §6.1 split-gate decision

**Split does NOT fire.** The §6.2-refined estimate: D1 ~80 prod + the 11-construction-site + ~10-read-site migration + ~80 tests; D2 `rds.rs` ~70 + ~110 tests; D3 HCM-walk merge + name-select + ordering + consumer migration ~150 + ~120 tests; D4 `register_rds_stats` (deterministic values, no handle threading per L3) ~90 + ~70 tests; D5 config_dump `Routes` variant ~90 + ~70 tests; D6 harness `{{RDS_PATH}}` (shared template — simpler than LDS) + the per-side `JsonSubtreeRule` (~15 LoC) ~80 + ~30 tests; D7 fixture ~230 (YAML + wrapper); D8 backstop + seed + contract ~190. **Total ~1100–1450 LoC / 11 tasks** — under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task gate. **ADR-0053 (split) does NOT fire** (reserved-but-unconsumed).

---

## File structure

| File | Create/Modify | Responsibility |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | Modify | `Rds` struct; `route_config: Option<RouteConfiguration>`; `validate()` handles `Option`; the per-HCM exactly-one-of check (called from `parse_bootstrap`) |
| `crates/envoy-config/src/lib.rs` | Modify | `parse_bootstrap` exactly-one-of pass; `load_dynamic_resources` RDS pass (HCM walk + name-select + populate); new `ConfigError` variants; `pub mod rds;` |
| `crates/envoy-config/src/rds.rs` | Create | `parse_rds_file(path, contents) -> Result<Vec<RouteConfiguration>, ConfigError>` (mirrors `lds.rs`) |
| `crates/envoy-http1/src/hcm.rs` | Modify | `clone_route_config` Option handling at `:200`/`:211`; the vhost match `:1177`; the 3 test construction sites |
| `crates/envoy-http2/src/hcm.rs` | Modify | the 8 test construction sites (`route_config: Some(...)`, `rds: None`) |
| `crates/envoy-listener/src/lib.rs` | Modify | `register_rds_stats(bootstrap, registry)` (per-HCM conditional registration) |
| `crates/envoy-bin/src/main.rs` | Modify | call `register_rds_stats` after `load_dynamic_resources` + `register_lds_stats` |
| `crates/envoy-admin/src/endpoint.rs` | Modify | `ConfigDumpEntry::Routes` variant + conditional push after Listeners |
| `tests/differential/src/lib.rs` | Modify | `{{RDS_PATH}}` shared-template rendering/mounting; per-side `JsonSubtreeRule` path override; RDS rendition added to the backend/host-gateway scans |
| `tests/fixtures/0028-xds-file-based-rds/` | Create | `envoy.yaml` + `envoy-rust.yaml` + `rds.yaml` (shared) + `cds.yaml` (shared) + `expectations.yaml` + `README.md` |
| `tests/differential/tests/xds_file_based_rds.rs` | Create | Docker-gated wrapper |
| `crates/envoy-bin/tests/xds_file_based_rds.rs` | Create | in-process backstop (happy + 6 negative paths) |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rds_route_config.yaml` | Create | fuzz seed (corpus 30 → 31) |
| `crates/envoy-config/fuzz/.gitignore` | Modify | allow-list the new seed |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | Modify | RDS stat rows + the xDS-section RDS extension + the RoutesConfigDump admin-body-shapes row |

---

### Task 1: `envoy-config` schema — the `Rds` struct + `route_config` → `Option` + the exactly-one-of validator + the D1 migration sweep

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`HttpConnectionManagerConfig` `:524-549`; `validate_hcm` `:2330`; the validator HCM-walk `:2151`; the test read sites)
- Modify: `crates/envoy-config/src/lib.rs` (`ConfigError`; the `parse_bootstrap` exactly-one-of pass)
- Modify: `crates/envoy-http1/src/hcm.rs` + `crates/envoy-http2/src/hcm.rs` (the 11 construction sites — same commit, the workspace-compile carry-forward)
- Test: `crates/envoy-config/src/bootstrap.rs` + `lib.rs` test modules

> **CARRY-FORWARD WARNING (D1):** making `route_config` an `Option` is a workspace-compile-affecting change. YAML fixtures with inline `route_config:` still parse (→ `Some`), but every Rust site that CONSTRUCTS `HttpConnectionManagerConfig { route_config: rc, .. }` (11 sites: `envoy-http1/src/hcm.rs` `:2525`/`:2596`/`:3233`; `envoy-http2/src/hcm.rs` `:1012`/`:1163`/`:1286`/`:1739`/`:1822`/`:2256`/`:2320`/`:2605`) must change to `route_config: Some(rc), rds: None` in THIS commit, and every site that READS `.route_config` (production: `bootstrap.rs:2370-2375` validator, `envoy-http1/src/hcm.rs:200`/`:1177`; ~9 `bootstrap.rs` test reads at `:5107`/`:5108`/`:5872`/`:5920`/`:6020`/`:6115`) must adapt. The build is RED until all sites are fixed — fix them all in this task.

- [ ] **Step 1: Write failing tests** (`crates/envoy-config` test module). (a) **the `Rds` struct parses:** a YAML HCM with `rds: { route_config_name: local_route, config_source: { path_config_source: { path: /x } } }` + NO `route_config` → `parse_bootstrap` succeeds; the HCM's `rds == Some(Rds { route_config_name: "local_route", config_source: ConfigSource { path_config_source: PathConfigSource { path: "/x" }, resource_api_version: None } })` and `route_config == None`. (b) **`resource_api_version` optional inside `rds.config_source`:** same with `config_source: { path_config_source: { path: /x }, resource_api_version: V3 }` → parses, `resource_api_version == Some("V3")`. (c) **inline `route_config` still parses to `Some`:** an existing-shape HCM with inline `route_config:` → `route_config.is_some()`, `rds.is_none()` (regression-equivalence). (d) **neither → `MissingRouteSource`:** an HCM with neither `route_config` nor `rds` → `parse_bootstrap` returns `Err(ConfigError::MissingRouteSource { .. })`. (e) **both → `AmbiguousRouteSource`:** an HCM with BOTH inline `route_config:` AND `rds:` → `Err(ConfigError::AmbiguousRouteSource { .. })`. (f) **unknown field inside `rds` rejected:** `rds: { route_config_name: x, config_source: {...}, ads: {} }` → parse error (the `deny_unknown_fields` on `Rds`). (g) **`api_config_source`/`ads`/`watched_directory` inside `config_source` still rejected** (the deferred surfaces; `ConfigSource`'s `deny_unknown_fields` unchanged).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config rds_schema`. Expected: FAIL (compile error — `Rds`/`MissingRouteSource`/`AmbiguousRouteSource` undefined).
- [ ] **Step 3: Implement the schema.** In `bootstrap.rs`:

```rust
// HttpConnectionManagerConfig (:524-549): route_config becomes Option + new rds sibling.
// Replace `pub route_config: RouteConfiguration,` (:547) with:
    /// 20 D1 (ADR-0051/0052): the inline route table. EXACTLY ONE of
    /// `route_config` (inline) or `rds` (file) per HCM (enforced at parse time —
    /// §5.8). After load_dynamic_resources populates an rds HCM's route_config
    /// from its file, both are Some (the loaded state — §5.3); downstream
    /// dispatch reads route_config uniformly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_config: Option<RouteConfiguration>,
    /// 20 D1: RDS — route configuration loaded from a file (reuses ConfigSource).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rds: Option<Rds>,

// New struct (near ConfigSource, ~:107):
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rds {
    pub route_config_name: String,
    pub config_source: ConfigSource, // reused verbatim from phase 18
}
```

- [ ] **Step 4: Implement the `ConfigError` variants** (`lib.rs`, near `LdsParseError`):

```rust
    #[error("missing route source on HCM (stat_prefix {stat_prefix:?}): exactly one of `route_config` or `rds` is required")]
    MissingRouteSource { stat_prefix: String },
    #[error("ambiguous route source on HCM (stat_prefix {stat_prefix:?}): `route_config` and `rds` are mutually exclusive")]
    AmbiguousRouteSource { stat_prefix: String },
    #[error("RDS file error reading {path:?}: {source}")]
    RdsFileError { path: String, source: std::io::Error },
    #[error("RDS file parse error in {path:?}: {message}")]
    RdsParseError { path: String, message: String },
    #[error("RDS route_config_name {name:?} not found in {path:?}")]
    RdsRouteConfigNotFound { name: String, path: String },
```

- [ ] **Step 5: Implement the parse-time exactly-one-of pass** (`parse_bootstrap` in `lib.rs`, after the YAML deserialize succeeds and BEFORE `validate()`). Walk every HCM across `bootstrap.static_resources.listeners` filter chains (the validator's `:2151` extraction pattern: `for chain in &listener.filter_chains { for filter in &chain.filters { if let Some(TypedConfig::HttpConnectionManager(hcm)) = &filter.typed_config { ... } } }`); for each HCM:

```rust
        match (hcm.route_config.is_some(), hcm.rds.is_some()) {
            (false, false) => return Err(ConfigError::MissingRouteSource {
                stat_prefix: hcm.stat_prefix.clone(),
            }),
            (true, true) => return Err(ConfigError::AmbiguousRouteSource {
                stat_prefix: hcm.stat_prefix.clone(),
            }),
            _ => {}
        }
```

  (Only static listeners exist at parse time; dynamic listeners are added later by `load_dynamic_resources` and their HCMs go through this same check — see Task 3 Step 4. Place this as a small helper `fn check_route_sources(bootstrap: &Bootstrap) -> Result<(), ConfigError>` so Task 3 can re-call it over the merged listener set.)

- [ ] **Step 6: Implement the `validate_hcm` Option handling** (`bootstrap.rs:2330`/`:2370-2375`). The existing body reads `hcm.route_config.virtual_hosts` and mutates `&mut hcm.route_config.virtual_hosts`. Guard on the Option — when `None` (an `rds` HCM pre-load), SKIP the inline-route validation entirely:

```rust
    // 20 D1 (C16): inline route_config validation runs only when present.
    // An rds HCM has route_config: None at parse time (the route table is
    // populated post-merge by load_dynamic_resources, then re-validated). The
    // exactly-one-of check ran at parse time (check_route_sources); this fn
    // never re-checks cardinality, so the post-merge both-Some state is valid.
    let Some(route_config) = hcm.route_config.as_mut() else {
        return Ok(()); // rds HCM, pre-load — nothing inline to validate yet
    };
    if route_config.virtual_hosts.is_empty() { /* existing EmptyVirtualHosts error, using route_config.name */ }
    for vh in &mut route_config.virtual_hosts { /* existing body */ }
```

  (Adapt to the actual `validate_hcm` signature/body. The cluster-reference defer-then-revalidate logic that already keys on `cds_configured_but_unloaded()` is unchanged — it now also covers rds-supplied routes after Task 3 populates them.)

- [ ] **Step 7: The D1 migration sweep — fix ALL 11 construction sites + the read sites** (same commit). In `envoy-http1/src/hcm.rs` (`:2525`, `:2596`, `:3233`) and `envoy-http2/src/hcm.rs` (`:1012`, `:1163`, `:1286`, `:1739`, `:1822`, `:2256`, `:2320`, `:2605`): change each `route_config: <expr>,` to `route_config: Some(<expr>), rds: None,`. In `envoy-http1/src/hcm.rs:200`: `clone_route_config(&cfg.route_config)` → `clone_route_config(cfg.route_config.as_ref().expect("route_config populated post-load — §5.3 invariant"))` (the post-load invariant: every HCM resolves to `Some` after `load_dynamic_resources`; the `expect` is the structural witness). The vhost match at `:1177` already reads through the cloned `config.route_config` (an `HCMConfig` field, NOT the `Option`) — verify `HCMConfig.route_config` stays a non-Option `Arc<RouteConfiguration>` (it is built from the unwrapped clone), so `:1177` is UNCHANGED. The ~9 `bootstrap.rs` test reads (`:5107` etc.) adapt to `.route_config.as_ref().unwrap().virtual_hosts` (these are tests asserting on parsed inline-route HCMs, which are `Some`).
- [ ] **Step 8: Run, verify pass + the whole workspace compiles.** Run: `cargo test -p envoy-config && cargo build --workspace --all-targets`. Expected: PASS (the migration sweep restores the workspace build; inline-route fixtures parse identically).
- [ ] **Step 9: clippy + fmt + standalone builds + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p envoy-config && cargo build -p envoy-cluster && cargo build -p envoy-http1 && cargo build -p envoy-http2
git add crates/
git commit -m "phase 20 Task 1: rds schema + route_config->Option + exactly-one-of validator + D1 migration sweep [ADR-0051, ADR-0052]"
```

---

### Task 2: `envoy-config` — the RDS file parser (`rds.rs`)

**Files:**
- Create: `crates/envoy-config/src/rds.rs`
- Modify: `crates/envoy-config/src/lib.rs` (`pub mod rds;`)
- Test: `crates/envoy-config/src/rds.rs` test module

- [ ] **Step 1: Write failing tests.** (a) **bare `resources:` envelope (L1):** a YAML with `resources:` listing one `@type`-tagged RouteConfiguration `local_route` (2 routes) → `parse_rds_file("/x.yaml", &s)` returns `Ok(vec![rc])` with `rc.name == "local_route"`, `rc.virtual_hosts.len() == 1`. (b) **full `DiscoveryResponse` envelope (L1):** the same wrapped with `version_info: v1` + `resources:` → also `Ok(vec![rc])` (version ignored). (c) **multiple RouteConfigurations:** a file with two RouteConfigurations → `Ok(vec![rc1, rc2])` (name-selection is Task 3, not here). (d) **non-RouteConfiguration `@type` rejected:** a resource tagged `...v3.Cluster` → `Err(RdsParseError)` (the serde `@type` tag rejects). (e) **malformed YAML → `RdsParseError`** (L4). (f) **missing `@type` → `RdsParseError`** (L1 — Envoy `update_failure`; envoy-rust fatal-at-parse).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config rds_parse`. Expected: FAIL.
- [ ] **Step 3: Implement `rds.rs`** (mirror `lds.rs` — the C5/C17 decision: a new sibling module, NOT the deferred `xds_file.rs` consolidation):

```rust
//! 20 D2 (ADR-0051/0052): the RDS file parser. Mirrors lds.rs/cds.rs — the
//! @type-tagged envelope with RouteConfiguration resources. Always-YAML
//! (serde_yaml, regardless of extension — the ADR-0049 decision-1 posture;
//! the Envoy-side container path is structurally .yaml). The named-resource
//! selection (route_config_name) happens at merge time (lib.rs), not here.
//! M19-1 (the xds_file.rs consolidation) deferred per PLAN C17.
use crate::bootstrap::RouteConfiguration;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "@type")]
enum RdsResource {
    #[serde(rename = "type.googleapis.com/envoy.config.route.v3.RouteConfiguration")]
    RouteConfiguration(RouteConfiguration),
}

#[derive(Debug, Deserialize)]
struct RdsFile {
    #[serde(default)]
    #[allow(dead_code)]
    version_info: Option<String>, // accepted-and-ignored (L1)
    resources: Vec<RdsResource>,
}

pub fn parse_rds_file(path: &str, contents: &str) -> Result<Vec<RouteConfiguration>, crate::ConfigError> {
    let file: RdsFile =
        serde_yaml::from_str(contents).map_err(|e| crate::ConfigError::RdsParseError {
            path: path.to_string(),
            message: e.to_string(),
        })?;
    Ok(file
        .resources
        .into_iter()
        .map(|RdsResource::RouteConfiguration(rc)| rc)
        .collect())
}
```

  (Verify the `RdsFile` struct shape against how `lds.rs`/`cds.rs` deserialize `version_info` — match the existing `LdsFile`/`CdsFile` field handling exactly. Add `pub mod rds;` to `lib.rs` beside `pub mod lds;`.)

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-config rds_parse`. Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-config/
git commit -m "phase 20 Task 2: RDS file parser (rds.rs) [ADR-0051]"
```

---

### Task 3: `load_dynamic_resources` RDS pass — HCM walk + name-selection + effective-`route_config` population + the §5.7 ordering + the consumer migration

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (`load_dynamic_resources` `:571-656`)
- Test: `crates/envoy-config/src/lib.rs` test module (use `tempfile` — the existing dev-dep)

- [ ] **Step 1: Write failing tests** (use `tempfile` for RDS/CDS files). (a) **the RDS pass loads + populates:** a bootstrap with a static listener whose HCM has `rds` pointing at a temp RDS file (RouteConfiguration `local_route`, route `/static` → static cluster `static_backend`) + 1 static cluster → `load_dynamic_resources` succeeds; the HCM's `route_config == Some(rc)` with `rc.name == "local_route"`; `rds` is still `Some` (kept — §5.3/C16). (b) **missing RDS file is fatal (L4):** `RdsFileError`. (c) **malformed RDS file is fatal (L4):** `RdsParseError`. (d) **`route_config_name` mismatch is fatal (L6):** RDS file defines `other_route`, HCM wants `local_route` → `RdsRouteConfigNotFound { name: "local_route", .. }`. (e) **the §5.7 RDS+CDS composition resolves (L7):** a bootstrap with BOTH `cds_config` (temp CDS file → cluster `dynamic_backend`) AND a static-listener HCM with `rds` (RDS file routing `/dynamic` → `dynamic_backend`, no `validate_clusters`) → `load_dynamic_resources` succeeds (the RDS route to the CDS cluster resolves because clusters merge BEFORE the post-merge re-validation). (f) **unresolved RDS route is fatal (the L7 narrow divergence):** an RDS route to cluster `nope` (in NEITHER list) → `UnknownCluster`, NOT a panic. (g) **post-merge re-validation runs once:** verify an rds HCM's populated route_config goes through `validate_hcm` (e.g. an RDS RouteConfiguration with an empty virtual_hosts → the existing inline-route error fires post-merge).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config load_dynamic`. Expected: FAIL.
- [ ] **Step 3: Implement the RDS pass** in `load_dynamic_resources`, AFTER the LDS branch (`:642`) and BEFORE the post-merge `validate()` (`:653`):

```rust
    // ---- RDS pass (phase 20, ADR-0051/0052; §6.2 L1/L4/L6/L7) ----
    // Walk every HCM across the EFFECTIVE listener set (static + LDS-merged
    // dynamic); for each rds-configured HCM, read its file, name-select the
    // RouteConfiguration, and populate the effective route_config. Runs AFTER
    // the CDS+LDS merges so an RDS route may reference a dynamic cluster (§5.7);
    // the post-merge validate() below re-enforces references against the full
    // effective state. The HCM walk covers dynamic_listeners too so the design
    // is LDS+RDS-composition-ready (§4 defers the bilateral fixture).
    let (static_listeners, dynamic_listeners) = (
        &mut bootstrap.static_resources.listeners,
        &mut bootstrap.dynamic_listeners,
    );
    for listener in static_listeners.iter_mut().chain(dynamic_listeners.iter_mut().flatten()) {
        for chain in &mut listener.filter_chains {
            for filter in &mut chain.filters {
                let Some(TypedConfig::HttpConnectionManager(hcm)) = filter.typed_config.as_mut() else {
                    continue;
                };
                let Some(rds) = hcm.rds.as_ref() else { continue };
                let path = rds.config_source.path_config_source.path.clone();
                let name = rds.route_config_name.clone();
                let contents = std::fs::read_to_string(&path)
                    .map_err(|source| ConfigError::RdsFileError { path: path.clone(), source })?;
                let mut parsed = rds::parse_rds_file(&path, &contents)?;
                let selected = parsed
                    .iter()
                    .position(|rc| rc.name == name)
                    .map(|i| parsed.remove(i))
                    .ok_or(ConfigError::RdsRouteConfigNotFound { name, path })?;
                hcm.route_config = Some(selected); // §5.3: uniform downstream shape
            }
        }
    }
    // ---- §5.7: ONE post-merge re-validation after the CDS + LDS + RDS merges ----
    if bootstrap.dynamic_clusters.is_some()
        || bootstrap.dynamic_listeners.is_some()
        || /* any HCM had rds */ has_rds_hcm(bootstrap)
    {
        bootstrap::validate(bootstrap)?;
    }
    Ok(())
```

  (The `validate()` gate currently keys on `dynamic_clusters || dynamic_listeners`; extend it so an rds-only bootstrap — cds/lds both absent — still re-validates its now-populated route_config. Add a small `has_rds_hcm(&Bootstrap) -> bool` helper walking the static+dynamic listeners, OR set a `bool` flag inside the RDS pass loop and reuse it. The borrow of `bootstrap` for the split-listener walk must end before the `validate(bootstrap)` call — collect the flag, then drop the mutable borrow.)

- [ ] **Step 4: Re-run the exactly-one-of check over the merged listener set.** After the RDS population (so dynamic LDS-supplied listeners' HCMs are also checked), the `check_route_sources` helper from Task 1 Step 5 must be reachable over `all_listeners()`. Since LDS listeners are merged in Task-3's predecessor (the LDS branch already ran), and an LDS-supplied HCM could itself be neither/both — call `check_route_sources` over the effective set inside `load_dynamic_resources` BEFORE the RDS pass populates anything (a dynamic listener's HCM with neither source must fail, a both must fail). Add a test: an LDS file whose listener's HCM has neither route source → `MissingRouteSource`.
- [ ] **Step 5: Run, verify pass.** Run: `cargo test -p envoy-config && cargo build --workspace --all-targets`. Expected: PASS.
- [ ] **Step 6: clippy + fmt + standalone builds + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p envoy-config && cargo build -p envoy-cluster && cargo build -p envoy-http1 && cargo build -p envoy-http2
git add crates/envoy-config/
git commit -m "phase 20 Task 3: load_dynamic_resources RDS pass + §5.7 ordering + effective-route_config population [ADR-0051, ADR-0052]"
```

---

### Task 4: Per-HCM `http.<stat_prefix>.rds.<route_config_name>.*` stats (conditional registration via `register_rds_stats`)

**Files:**
- Modify: `crates/envoy-listener/src/lib.rs` (new free function `register_rds_stats` + tests)
- Modify: `crates/envoy-bin/src/main.rs` (the call site, after `register_lds_stats`)
- Test: `crates/envoy-listener/src/lib.rs` test module

> **Simplification (L3):** all 5 stat values are deterministic at initial load (`update_attempt`/`update_success`/`config_reload` = 1; `update_failure`/`update_rejected` = 0 — the all-fatal posture makes non-zero structurally unreachable). So `register_rds_stats` registers-and-sets directly, exactly like `register_lds_stats` — NO handle threading to a separate increment site (resolving SPEC §0-finding-3's worry).

- [ ] **Step 1: Write failing tests.** (a) **conditional registration (the §5.2 inertness invariant):** `register_rds_stats` on a bootstrap whose HCMs all carry inline `route_config` (incl. one WITH `cds_config`/`lds_config` — the fixture-0026/0027 inertness witness) → NO stat whose name contains `.rds.` exists in the registry afterward. (b) **the 5-name subset on an rds HCM:** a bootstrap with a static listener whose HCM has `rds: { route_config_name: local_route, .. }` and `stat_prefix: ingress_http` → `http.ingress_http.rds.local_route.update_attempt == 1`, `.update_success == 1`, `.update_failure == 0`, `.update_rejected == 0`, `.config_reload == 1`. (c) **per-HCM keying:** two listeners, two HCMs with distinct `stat_prefix` (`a`/`b`) + distinct `route_config_name` (`r1`/`r2`), both rds → both name families register (`http.a.rds.r1.*` AND `http.b.rds.r2.*`). (Construct bootstraps directly — no file I/O at this layer; the route_config can be left None or populated, the predicate is `rds.is_some()`.)
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-listener rds_stats`. Expected: FAIL.
- [ ] **Step 3: Implement `register_rds_stats`** (mirror `register_lds_stats` `:369-393`, but per-HCM, walking filter chains):

```rust
/// 20 D4 (ADR-0051/0052; §6.2 L3/L10): the per-HCM http.<stat_prefix>.rds.<route_config_name>.*
/// stat family — registered ONLY for HCMs whose `rds` is configured (the §5.2
/// per-HCM conditional-registration discipline; inline-route HCMs emit no rds.*
/// names). All RDS load failures are fatal pre-registration (the L4 all-fatal
/// posture), so update_failure/update_rejected register at 0 and never tick;
/// config_reload ticks 1 at initial load (L3). Called once from envoy-bin
/// main(), after load_dynamic_resources + register_lds_stats.
pub fn register_rds_stats(
    bootstrap: &envoy_config::Bootstrap,
    registry: &envoy_stats::StatsRegistry,
) -> Result<(), ListenerError> {
    for listener in bootstrap.all_listeners() {
        for chain in &listener.filter_chains {
            for filter in &chain.filters {
                let Some(envoy_config::TypedConfig::HttpConnectionManager(hcm)) =
                    filter.typed_config.as_ref()
                else {
                    continue;
                };
                let Some(rds) = hcm.rds.as_ref() else { continue };
                let base = format!("http.{}.rds.{}", hcm.stat_prefix, rds.route_config_name);
                let mk = |suffix: &str| {
                    registry
                        .register_counter(&format!("{base}.{suffix}"))
                        .map_err(|e| ListenerError::StatsRegistration(e.to_string()))
                };
                mk("update_attempt")?.add(1);
                mk("update_success")?.add(1);
                mk("config_reload")?.add(1);
                mk("update_failure")?; // registers at 0 (L4)
                mk("update_rejected")?; // registers at 0 (L4)
            }
        }
    }
    Ok(())
}
```

  (Verify `envoy_config::TypedConfig` is the correct re-export path for the network-filter enum + that `all_listeners()` is `pub`. Adapt `ListenerError::StatsRegistration` to its actual variant shape. Confirm `register_counter` is the registry API used by `register_lds_stats`.)

- [ ] **Step 4: Implement the main.rs call site.** In `crates/envoy-bin/src/main.rs`, immediately after the `register_lds_stats` call:

```rust
    // 20 D4: conditional per-HCM http.<prefix>.rds.<name>.* registration (no-op
    // when no HCM uses rds — the §5.2 inertness invariant).
    envoy_listener::register_rds_stats(&bootstrap, &registry)
        .context("registering http.*.rds.* stats")?;
```

- [ ] **Step 5: Run, verify pass.** Run: `cargo test -p envoy-listener && cargo build --workspace --all-targets`. Expected: PASS.
- [ ] **Step 6: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-listener/ crates/envoy-bin/
git commit -m "phase 20 Task 4: conditional per-HCM http.*.rds.* stat family (register_rds_stats) [ADR-0051, ADR-0052]"
```

---

### Task 5: `/config_dump` `RoutesConfigDump` entry (conditional emission, after Listeners)

**Files:**
- Modify: `crates/envoy-admin/src/endpoint.rs` (`ConfigDumpEntry` enum `:301-338`; the render-ordering `:467-551`)
- Test: `crates/envoy-admin/src/endpoint.rs` test module

> **L5 lock-in:** the dynamic entry shape is `{ "@type": ".../RoutesConfigDump", "dynamic_route_configs": [ { "route_config": { "@type": ".../RouteConfiguration", "name", "virtual_hosts" }, "last_updated": "<ts>" } ] }` — NO `version_info`. Conditional emission (only when some HCM has `rds.is_some()`) → on fixture 0028 (cds yes, lds no, rds yes) envoy-rust lands it at `configs[2]`; Envoy has it at `configs[4]` (the index mismatch reconciled in Task 6/7).

- [ ] **Step 1: Write failing tests.** (a) **conditional emission:** a handler whose bootstrap has an rds HCM (route_config populated, rds Some) → `/config_dump` `configs[]` contains an entry with `@type` ending `RoutesConfigDump` whose `dynamic_route_configs[0].route_config.name == <the loaded name>`. (b) **inertness:** a handler with only inline-route HCMs (no rds) → NO `RoutesConfigDump` entry in `configs[]` (fixtures 0014/0026/0027 untouched). (c) **ordering:** on a bootstrap with cds + an rds HCM but no lds → the Routes entry is at `configs[2]` (Bootstrap[0], Clusters[1], Routes[2]); add a cds+lds+rds case asserting Routes at `configs[3]`.
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-admin config_dump_routes`. Expected: FAIL.
- [ ] **Step 3: Implement the enum variant** (`ConfigDumpEntry`, after `Listeners`):

```rust
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.RoutesConfigDump")]
    Routes {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        dynamic_route_configs: Vec<DynamicRouteConfigEntry<'a>>,
    },
```

  with the supporting structs (mirroring `DynamicClusterEntry`/`DynamicListenerEntry`):

```rust
#[derive(Serialize)]
struct DynamicRouteConfigEntry<'a> {
    route_config: RouteConfigBody<'a>,
    last_updated: String,
}
#[derive(Serialize)]
struct RouteConfigBody<'a> {
    #[serde(rename = "@type")]
    type_url: &'static str, // "type.googleapis.com/envoy.config.route.v3.RouteConfiguration"
    name: &'a str,
    virtual_hosts: &'a Vec<envoy_config::VirtualHost>,
}
```

  (Adapt to the actual `VirtualHost`/serialize conventions used by the existing `DynamicClusterEntry`. If `RouteConfiguration` already `Serialize`s in the shape Envoy emits, prefer serializing `&'a RouteConfiguration` directly with a flattened `@type` — match whatever the Clusters/Listeners entries do.)

- [ ] **Step 4: Implement the conditional push** (render-ordering, AFTER the Listeners block `:547`):

```rust
    // 20 D5 (ADR-0051/0052; §6.2 L5): RoutesConfigDump — emitted ONLY when some
    // HCM uses rds (conditional emission; fixtures 0014/0026/0027 untouched).
    // Envoy always emits this section (static_route_configs for inline routes)
    // and positions it at configs[4] after a ScopedRoutesConfigDump; envoy-rust
    // narrows to rds-only and lands it after the (conditional) Listeners entry.
    // The differential index mismatch is reconciled per-side in the harness.
    let dynamic_route_configs: Vec<DynamicRouteConfigEntry> = bootstrap
        .all_listeners()
        .flat_map(|l| l.filter_chains.iter())
        .flat_map(|c| c.filters.iter())
        .filter_map(|f| match f.typed_config.as_ref() {
            Some(envoy_config::TypedConfig::HttpConnectionManager(hcm)) if hcm.rds.is_some() => {
                hcm.route_config.as_ref().map(|rc| DynamicRouteConfigEntry {
                    route_config: RouteConfigBody {
                        type_url: "type.googleapis.com/envoy.config.route.v3.RouteConfiguration",
                        name: &rc.name,
                        virtual_hosts: &rc.virtual_hosts,
                    },
                    last_updated: String::new(), // value_may_differ in the fixture
                })
            }
            _ => None,
        })
        .collect();
    if !dynamic_route_configs.is_empty() {
        configs.push(ConfigDumpEntry::Routes { dynamic_route_configs });
    }
```

  (Verify the `bootstrap` accessor used here matches the enclosing function's binding — the existing Clusters/Listeners blocks show the exact accessor + lifetime shape.)

- [ ] **Step 5: Run, verify pass.** Run: `cargo test -p envoy-admin && cargo build --workspace --all-targets`. Expected: PASS.
- [ ] **Step 6: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-admin/
git commit -m "phase 20 Task 5: /config_dump RoutesConfigDump entry (conditional emission) [ADR-0051, ADR-0052]"
```

---

### Task 6: Harness — `{{RDS_PATH}}` shared-template rendering/mounting + the per-side `JsonSubtreeRule` path override

**Files:**
- Modify: `tests/differential/src/lib.rs` (the dynamic-file machinery `:2187-2251`; `JsonSubtreeRule` `:612-617`; the subtree match `:4190-4223`; the backend/host-gateway scans `:923-945`)
- Test: `tests/differential/src/lib.rs` test module (unit tests for the per-side path resolution; the fixture is Task 7)

- [ ] **Step 1: Write failing tests** for the per-side `JsonSubtreeRule`. (a) **shared `path` unchanged:** a `JsonSubtreeRule { path: "configs.1.x", expected }` with no per-side fields → both sides walk `configs.1.x` (the existing behavior — regression). (b) **per-side override:** `JsonSubtreeRule { path: "", path_envoy: Some("configs.4.y"), path_envoy_rust: Some("configs.2.y"), expected }` → the envoy side walks `configs.4.y`, the envoy-rust side walks `configs.2.y`, both compared to `expected`. (Construct two distinct JSON values and assert the match logic picks the right path per side.)
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p differential json_subtree_per_side`. Expected: FAIL.
- [ ] **Step 3: Extend `JsonSubtreeRule`** (`:612-617`):

```rust
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct JsonSubtreeRule {
    /// Dotted-path key, e.g. `configs.0.bootstrap.node.id`. Shared default;
    /// overridden per-side when path_envoy / path_envoy_rust is present.
    #[serde(default)]
    pub path: String,
    /// 20 D6 (ADR-0052): per-side path override. When present, overrides `path`
    /// for that proxy only (the configs[] index diverges — Envoy emits
    /// ScopedRoutes/Secrets sections + always-on RoutesConfigDump that envoy-rust
    /// does not, so a fixed shared index cannot match both). Mirrors the per-side
    /// text-line allow-list mechanism (allowlist_envoy_only_*).
    #[serde(default)]
    pub path_envoy: Option<String>,
    #[serde(default)]
    pub path_envoy_rust: Option<String>,
    pub expected: serde_yaml::Value,
}
```

- [ ] **Step 4: Update the subtree match** (`:4190-4223`) to resolve the path per side:

```rust
    if let Some(subtree) = required_subtree {
        let envoy_path = subtree.path_envoy.as_deref().unwrap_or(&subtree.path);
        let rust_path = subtree.path_envoy_rust.as_deref().unwrap_or(&subtree.path);
        let envoy_sub = walk_pointer(&envoy_json, envoy_path)
            .with_context(|| format!("envoy required_subtree path {envoy_path:?}"))?;
        let rust_sub = walk_pointer(&rust_json, rust_path)
            .with_context(|| format!("envoy-rust required_subtree path {rust_path:?}"))?;
        // ... existing expected-comparison logic, unchanged ...
    }
```

- [ ] **Step 5: Implement `{{RDS_PATH}}` rendering/mounting** (mirror the SHARED-`cds.yaml` path at `:2187-2215`, NOT the per-side LDS path — L8: one shared `rds.yaml`):

```rust
    let needs_rds =
        upstream_template.contains("{{RDS_PATH}}") || subject_template.contains("{{RDS_PATH}}");
    let rds_template = if needs_rds {
        Some(
            std::fs::read_to_string(fixture_dir.join("rds.yaml"))
                .context("reading rds.yaml (fixture references {{RDS_PATH}})")?,
        )
    } else {
        None
    };
    let subject_rds_path = tmp.path().join("rds-subject.yaml");
    let subject_rds_path_str = subject_rds_path.to_string_lossy().into_owned();
```

  Add `RDS_CONTAINER_PATH` (a `.yaml`-ending constant, mirroring `CDS_CONTAINER_PATH`), set `RDS_PATH` in the per-side kv maps (upstream → container path, subject → host temp path), render the shared `rds.yaml` per-side, write the subject rendition to `subject_rds_path`, and mount the upstream rendition into the Envoy container (`with_copy_to` to `RDS_CONTAINER_PATH`). Add the RDS rendition to the `scan_needs_marker` backend-detection sources AND the `uses_host_gateway` scan sources (the phase-18 carryforward-disposition-2 bug-class lesson: scan ALL rendered sources).
- [ ] **Step 6: Run, verify pass.** Run: `cargo test -p differential --lib`. Expected: PASS (the existing fixtures' shared-path subtree rules unchanged; the new per-side unit tests green).
- [ ] **Step 7: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add tests/differential/
git commit -m "phase 20 Task 6: harness {{RDS_PATH}} shared-template + per-side JsonSubtreeRule path override [ADR-0052]"
```

---

### Task 7: Fixture `0028-xds-file-based-rds` + Docker-gated wrapper

**Files:**
- Create: `tests/fixtures/0028-xds-file-based-rds/{envoy.yaml, envoy-rust.yaml, rds.yaml, cds.yaml, expectations.yaml, README.md}`
- Create: `tests/differential/tests/xds_file_based_rds.rs`

- [ ] **Step 1: Author the fixture configs.** Both sides: `admin` + `node: { id, cluster }` + ONE static cluster `static_backend` (STRICT_DNS → the echo helper, `{{...}}` host/port markers per the 0026/0027 pattern) + ONE static listener (H1 HCM, `stat_prefix: ingress_http`, `rds: { route_config_name: local_route, config_source: { path_config_source: { path: {{RDS_PATH}} } } }`, NO inline `route_config`, `http_filters` ending in router) + `dynamic_resources: { cds_config: { path_config_source: { path: {{CDS_PATH}} } } }`. The Envoy side may carry the established Envoy-only main-config fields (per the 0008/0026/0027 per-side main-config precedent) if needed for byte-exact echo bodies; the RDS file is SHARED (L8). Shared `rds.yaml`: a `resources:` envelope with one `@type`-tagged RouteConfiguration `local_route`, vh `domains: ["*"]`, routes `/static` → `static_backend` + `/dynamic` → `dynamic_backend` (NO `validate_clusters` — L7). Shared `cds.yaml`: the fixture-0026 shape verbatim (cluster `dynamic_backend` → the echo helper). `README.md`: the topology + the L1–L11 lock-ins + the "L3 Envoy-only stat enumeration" note (the Envoy-only `version`/`version_text`/`update_time`/`config_reload_time_ms`/`update_empty`/`init_fetch_timeout`/`update_duration` names, NOT asserted) + the L5 per-side config_dump index rationale.
- [ ] **Step 2: Author `expectations.yaml`** (the §1 probe list; `Driver::Http1KeepAlive`, two GETs over one conn):

```yaml
driver:
  kind: http1_keep_alive
  requests:
    - method: GET
      path: /static
      host: static_backend
      expected_status: 200
      expected_body: { kind: byte_exact, body: "<echo body for /static — copy the 0027 shape>" }
      require_header_present: x-envoy-upstream-service-time
    - method: GET
      path: /dynamic
      host: dynamic_backend
      expected_status: 200
      expected_body: { kind: byte_exact, body: "<echo body for /dynamic>" }
      require_header_present: x-envoy-upstream-service-time
  settle_ms: 200
  expected_stats:
    # RDS load (L3 — the conditional per-HCM 5-name subset):
    - { name: http.ingress_http.rds.local_route.update_attempt,  value: 1 }
    - { name: http.ingress_http.rds.local_route.update_success,  value: 1 }
    - { name: http.ingress_http.rds.local_route.update_failure,  value: 0 }
    - { name: http.ingress_http.rds.local_route.update_rejected, value: 0 }
    - { name: http.ingress_http.rds.local_route.config_reload,   value: 1 }
    # CDS load (the fixture-0026 family; 2 clusters: static_backend + dynamic_backend):
    - { name: cluster_manager.cds.update_attempt,  value: 1 }
    - { name: cluster_manager.cds.update_success,  value: 1 }
    - { name: cluster_manager.cds.update_failure,  value: 0 }
    - { name: cluster_manager.cds.update_rejected, value: 0 }
    - { name: cluster_manager.cluster_added,       value: 2 }
    - { name: cluster_manager.active_clusters,     value: 2 }
    # Data-plane through each cluster:
    - { name: cluster.static_backend.upstream_rq_total,  value: 1 }
    - { name: cluster.dynamic_backend.upstream_rq_total, value: 1 }
    # HCM downstream:
    - { name: http.ingress_http.downstream_rq_total, value: 2 }
    - { name: http.ingress_http.downstream_rq_2xx,   value: 2 }
  admin_scrapes:
    # /config_dump (L5): RoutesConfigDump at envoy configs[4], envoy-rust configs[2]
    # (per-side path override — Envoy interposes ScopedRoutes[3]; envoy-rust gates
    # Listeners off [no lds] so Routes follows Clusters[1]).
    - path: /config_dump
      expected_status: 200
      expected_content_type: application/json
      expected_body_rule:
        kind: json_shape
        required_keys: ["configs"]
        required_subtree:
          path_envoy: configs.4.dynamic_route_configs.0.route_config.name
          path_envoy_rust: configs.2.dynamic_route_configs.0.route_config.name
          expected: local_route
        value_may_differ_keys: ["configs"]
    # /config_dump (fixture-0026 compatibility): ClustersConfigDump still at configs[1]
    # on BOTH sides (not displaced by the rds entry).
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
```

  (Copy the exact `expected_body` echo strings from the working fixture 0027 — the echo helper's response shape is identical. Verify the H1 echo body byte-for-byte against a local run in Step 3.)
- [ ] **Step 3: Author the Docker-gated wrapper** `tests/differential/tests/xds_file_based_rds.rs` (copy the `xds_file_based_lds.rs` wrapper shape). Then run it LOCALLY once: **pre-build `tests/helpers/*` first**, and **do NOT run the Docker suite concurrently with any cargo build** (per `project_flaky_access_log_fixture_0012`):

```bash
cargo build -p http1-echo-server   # pre-build the helper (cold-helper flake class)
cargo test -p differential --test xds_file_based_rds -- --nocapture
```
  Expected: PASS (both probes 200 + stats + config_dump bilateral). If the echo body differs, fix the `expected_body` strings (Step 2) to match the captured bytes.
- [ ] **Step 4: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add tests/fixtures/0028-xds-file-based-rds/ tests/differential/tests/xds_file_based_rds.rs
git commit -m "phase 20 Task 7: fixture 0028-xds-file-based-rds + Docker-gated wrapper [ADR-0051, ADR-0052]"
```

---

### Task 8: In-process backstop (happy path + 6 negative paths)

**Files:**
- Create: `crates/envoy-bin/tests/xds_file_based_rds.rs`

> **Reuse note (M18-9, now N≥4):** the backstop helper block (`reserve_port`, `wait_ready`, `http1_oneshot`, `spawn_envoy_bin`, `write_file`, the bootstrap builder) is copied from `crates/envoy-bin/tests/xds_file_based_lds.rs`. Record the duplication in the file header — the extract-a-test-support-crate item stays a future hardening task (PLAN C17 / phase-19 carryforward).

- [ ] **Step 1: Write the backstop** (copy the LDS backstop's helper block; adapt the bootstrap builder to emit an `rds`-configured HCM). Cover:
  - **(i) happy path:** temp RDS file (`local_route`, `/static` → static cluster, `/dynamic` → CDS cluster) + temp CDS file (`dynamic_backend`) → both probes 200 + the `http.<prefix>.rds.local_route.*` 5-name subset present with the L3 values + `/config_dump` contains a `RoutesConfigDump` whose `dynamic_route_configs[0].route_config.name == "local_route"`.
  - **(ii) missing RDS file → process exits non-zero** (`RdsFileError`; spawn fails to reach ready) — the L4 agrees-with-Envoy class.
  - **(iii) malformed RDS file → exits** (`RdsParseError`) — the L4 envoy-rust-diverges class.
  - **(iv) `route_config_name` mismatch → exits** (`RdsRouteConfigNotFound`) — L6.
  - **(v) RDS route to a cluster in NEITHER list → exits** (`UnknownCluster`) — the L7 narrow divergence.
  - **(vi) both `route_config` AND `rds` → exits** (`AmbiguousRouteSource`) — L9.
  - **(vii) neither route source → exits** (`MissingRouteSource`) — L9.
  - **(viii) inertness witness:** a CDS-only bootstrap (no rds HCM) → `/config_dump` does NOT contain `"RoutesConfigDump"` and `/stats` has no `.rds.` name.
- [ ] **Step 2: Run, verify pass.** Pre-build the helper, then: `cargo test -p envoy-bin --test xds_file_based_rds`. Expected: PASS (8 cases).
- [ ] **Step 3: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-bin/tests/
git commit -m "phase 20 Task 8: in-process RDS backstop (happy + 6 negative + inertness) [ADR-0051, ADR-0052]"
```

---

### Task 9: Fuzz seed `hcm_rds_route_config.yaml` (corpus 30 → 31)

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rds_route_config.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (allow-list the new seed)

> **Atomic-edit lesson (09→19):** the `.gitignore` allow-list line AND the seed file land in the SAME commit (the corpus-consistency discipline; phase-19 carryforward-disposition-1 closed the prior inconsistency — keep it closed).

- [ ] **Step 1: Author the seed** — a minimal bootstrap whose HCM uses `rds: { route_config_name: local_route, config_source: { path_config_source: { path: /etc/envoy-rds/rds.yaml } } }` (NO inline route_config). `parse_bootstrap` parses-and-validates the SCHEMA only — it NEVER reads the referenced RDS file (the file load is `load_dynamic_resources`, a separate entry point the fuzz target does not call); the exactly-one-of passes (rds present, route_config absent); the deferred route-cluster validation is gated by the configured-but-unloaded predicate, as for cds/lds.

```yaml
# 20 Task 9: fuzz seed for the HCM rds schema surface (parse_bootstrap never reads
# the referenced RDS file — parse-and-validate only; route validation defers while
# rds is configured-but-unloaded). Mirrors dynamic_resources_cds/lds.yaml.
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
node: { id: seed, cluster: seed }
static_resources:
  clusters:
    - name: static_backend
      type: STRICT_DNS
      dns_lookup_family: V4_ONLY
      load_assignment:
        cluster_name: static_backend
        endpoints: [{ lb_endpoints: [{ endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 8124 } } } }] }]
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
                rds:
                  route_config_name: local_route
                  config_source:
                    path_config_source: { path: /etc/envoy-rds/rds.yaml }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
```

  (Match the exact `codec_type`/router typed_config shape the other corpus seeds use — copy from `dynamic_resources_cds.yaml`.)
- [ ] **Step 2: Allow-list it** — add `!corpus/parse_bootstrap/hcm_rds_route_config.yaml` (matching the existing `.gitignore` allow-list convention).
- [ ] **Step 3: Verify it parses.** Run a quick parse check (e.g. a one-off `cargo test -p envoy-config` asserting `parse_bootstrap` accepts the seed contents, OR the fuzz harness's corpus replay). Expected: parses clean.
- [ ] **Step 4: commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rds_route_config.yaml crates/envoy-config/fuzz/.gitignore
git commit -m "phase 20 Task 9: fuzz seed hcm_rds_route_config.yaml (corpus 30->31) [ADR-0051]"
```

---

### Task 10: BEHAVIOR_CONTRACT extensions (stat rows + xDS-section RDS extension + RoutesConfigDump row)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `Stat-name mapping` section; the `xDS wire state machine` section; the `Admin endpoint body shapes` section)

- [ ] **Step 1: Add the per-HCM RDS stat rows** to `Stat-name mapping` — the 5-name subset (`http.<stat_prefix>.rds.<route_config_name>.{update_attempt,update_success,update_failure,update_rejected,config_reload}`), all value-exact (1/1/0/0/1 at initial load), with the per-HCM-scoping note + the conditional-registration narrowing (inline-route HCMs emit none) + the Envoy-only enumeration (`version`/`version_text`/`update_time`/`config_reload_time_ms`/`update_empty`/`init_fetch_timeout`/`update_duration`, NOT asserted).
- [ ] **Step 2: Add the `### Filesystem transport (path_config_source) — phase 20 RDS extension` subsection** to `xDS wire state machine`, in the phase-18/19 §(a)–(f) parallel structure, recording L1–L11 from ADR-0052: (a) the RDS envelope (L1) + the `rds`-on-HCM shape; (b) initial-load/readiness (L2); (c) the negative-path 3-way split + the all-fatal extension (L4) + the `route_config_name`-mismatch → `update_rejected` (L6); (d) the exactly-one-of disposition — both/neither fatal on BOTH proxies (L9); (e) the RDS+CDS composition + the no-`validate_clusters`-needed finding (L7) + the defer-then-revalidate narrow divergence; (f) the conditional-emission narrowing vs Envoy's always-emitted `RoutesConfigDump` (L5) + the per-side config_dump index reconciliation (L5).
- [ ] **Step 3: Add the `RoutesConfigDump` row** to `Admin endpoint body shapes` — the `dynamic_route_configs[].{route_config,last_updated}` shape (no `version_info`), conditional emission, the Envoy `configs[4]` / envoy-rust `configs[2]` index divergence + the per-side-path reconciliation.
- [ ] **Step 4: commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 20 Task 10: BEHAVIOR_CONTRACT RDS rows (stats + xDS RDS extension + RoutesConfigDump) [ADR-0051, ADR-0052]"
```

---

### Task 11: State-4 phase-done verification + STATE advance to state-5-next

**Files:**
- Modify: `docs/envoy-rust/phases/20-xds-file-based-rds/PROGRESS.md` (quote every gate output)
- Modify: `docs/envoy-rust/STATE.md` (advance to state-4-complete / state-5-next)

- [ ] **Step 1: Run the full local gate suite, quoting every output into PROGRESS** (the 05.3→19 evidence discipline). Pre-build `tests/helpers/*` before the workspace test run (the cold-helper flake class):

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

- [ ] **Step 2: Run the fuzz short-budget gate** on the extended corpus (the `parse_bootstrap` target; the new `hcm_rds_route_config.yaml` seed included):

```bash
cd crates/envoy-config/fuzz && cargo +nightly fuzz run parse_bootstrap -- -runs=200000 -max_total_time=60   # or the project's CI-budget invocation
```
  Quote the clean-run output into PROGRESS.

- [ ] **Step 3: Push + capture the Docker-gated CI anchor.** Push the branch; capture the CI run that lights up ALL gates simultaneously: fixture 0028 + all 27 pre-existing fixtures (0001–0027) green + h2spec ≥95% + the fuzz gate on the 31-seed corpus + the 5 stable-toolchain gates. Quote the run id + `conclusion=success` into PROGRESS (the phase-18 lesson: the CI-evidence check is load-bearing — do NOT claim done on local green alone).
- [ ] **Step 4: Advance STATE.md** to Active phase `20` state-4-complete / state-5-next (prepend the new active pointer; demote the current state-2 pointer to `_Historical_`; rewrite `## Next expected skill` to the state-5 code-review arc — `superpowers:requesting-code-review` over the phase-20 code range, SERIAL review subagents per concern-cluster, controller spot-verification; update `## Last commit` + `## Last updated`; append the state-4 evidence summary to the `### Phase-20 …` Notes). Commit PROGRESS + STATE.

```bash
git add docs/envoy-rust/phases/20-xds-file-based-rds/PROGRESS.md docs/envoy-rust/STATE.md
git commit -m "phase 20 Task 11: state-4 phase-done verification + STATE advance to state-5-next [ADR-0051, ADR-0052]"
```

---

## Self-review

**1. Spec coverage** (SPEC §3 D1–D8 → tasks):
- D1 (schema: `rds` + `route_config`→Option + exactly-one-of + ConfigError variants) → Task 1. ✓
- D2 (RDS file parser) → Task 2. ✓
- D3 (effective-`route_config` merge + ordering + consumer migration) → Task 3 (+ the consumer migration folded into Task 1's sweep + Task 3). ✓
- D4 (per-HCM `rds.*` stats) → Task 4. ✓
- D5 (`RoutesConfigDump`) → Task 5. ✓
- D6 (harness `{{RDS_PATH}}` + the per-side config_dump reconciliation) → Task 6. ✓
- D7 (fixture 0028 + wrapper) → Task 7. ✓
- D8 (backstop + fuzz seed + BEHAVIOR_CONTRACT) → Tasks 8 + 9 + 10. ✓
- SPEC §1 acceptance (a)–(f) → Task 11 (the state-4 gate). ✓
- SPEC §6.2 (the 11-item verification) → performed at THIS PLAN-write; locked as L1–L11 (ADR-0052). ✓
- SPEC §5 invariants: §5.2 inertness (Tasks 4/5/8 inertness tests), §5.3 every-HCM-resolves (Task 1 Step 7 + Task 3), §5.4 ownership (Task 3 — load-at-config-time), §5.5 config_dump separation + fixture-0026/0027 stability (Task 5 + Task 7 compat assertion), §5.6 one-shot load (Task 3 sync), §5.7 merge ordering (Task 3 Step 3), §5.8 exactly-one-of (Task 1 Step 5 + Task 3 Step 4). ✓

**2. Placeholder scan:** every code step shows the actual code or a precise adapt-to-existing instruction with the anchor line. The fixture echo-body strings (Task 7 Step 2) are the one deliberate "copy from fixture 0027 + verify against a local run" — bilaterally verified at Step 3, not a placeholder.

**3. Type consistency:** `parse_rds_file(path, contents) -> Result<Vec<RouteConfiguration>, ConfigError>` (Task 2) is consumed by Task 3's RDS pass. `Rds { route_config_name, config_source }` (Task 1) is read by Task 3 (`rds.config_source.path_config_source.path`, `rds.route_config_name`), Task 4 (`hcm.stat_prefix` + `rds.route_config_name`), Task 5 (`hcm.rds.is_some()`). `register_rds_stats(bootstrap, registry)` (Task 4) matches the main.rs call (Task 4 Step 4). `JsonSubtreeRule.path_envoy`/`path_envoy_rust` (Task 6) match the fixture (Task 7). `ConfigDumpEntry::Routes { dynamic_route_configs }` (Task 5) matches the fixture's `configs.N.dynamic_route_configs.0.route_config.name` assertion. Consistent.

**Carry-forward to the state-3 executor:** the D1 migration (Task 1) leaves the workspace RED until ALL 11 construction sites + the read sites are fixed in the same commit — do not split it. Clippy PER TASK. The state-4 verification (Task 11) MUST run the 4 standalone-crate builds + capture the CI anchor (do not claim done on local green). The `xds_file.rs` consolidation (M19-1) is DEFERRED by deliberate decision (C17) — record it in the PROGRESS rollovers, do not attempt it mid-phase.
