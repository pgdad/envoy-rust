# Phase 18 (`18-xds-file-based-cds`) — PROGRESS

> Running log, updated by the executor on each task completion (the 06.2 → 17 cadence).
> One entry per PLAN task; quote the verifying command output. The state-3 arc runs
> `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK
> (NOT deferred to state-4) per `project_state3_arc_skips_clippy`.

**PLAN:** `docs/envoy-rust/phases/18-xds-file-based-cds/PLAN.md`
**SPEC:** `docs/envoy-rust/phases/18-xds-file-based-cds/SPEC.md`
**Scope ADRs:** ADR-0048 (the xDS-family pick + the three §0 findings [filesystem transport needs no protos; initial-load-only compatible with ClusterManager immutability; the Envoy-side surface already allow-listed] + the minimum-viable scope/deferral ledger); ADR-0049 (§6.2 reconciliation — the @type-required/extension-driven envelope [L1]; the all-CDS-load-errors-fatal posture vs Envoy's 3-way warn-and-serve split [L4]; the static-wins collision rule replacing the projected DuplicateClusterName reject [L9]; the node.id + validate_clusters:false bootstrap prerequisites [L12]).

---

## State-2 PLAN-write (this commit)

- Performed the HEAVY SPEC §6.2 empirical verification against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`; Docker; foreground general-purpose subagent; CDS-configured bootstrap + host backend + admin `/stats` + `/config_dump` scrapes; every routing claim cross-checked against backend access logs). Findings L1–L12 locked into PLAN.md "§6.2 empirical lock-ins". **The §0 core findings are CONFIRMED** (the bare `resources:` YAML envelope loads [L1]; readiness implies loaded [L2]; the stat tree + ClustersConfigDump exist as projected [L3/L5]; zero-static-clusters bootstraps are valid [L7]). Four material divergences/absences (**L1** @type required + extension-driven parser; **L4** the negative-path 3-way split [missing-path fatal / parse-error warn-and-serve / semantic-error warn-and-serve] vs the projected binary; **L9** static-WINS on name collision [the projected DuplicateClusterName reject is dropped]; **L12** the node.id/node.cluster + validate_clusters:false bootstrap prerequisites) → **ADR-0049 landed**.
- Performed the PLAN-time SPEC-correction pass (read-only Explore subagent + controller re-verification by direct grep) against HEAD `3acf7367b`. **One anchor drifted** (the fuzz corpus is 28 in the `.gitignore` allow-list / 27 in the test arrays — a pre-existing inconsistency [`cluster_http2_protocol_options.yaml` allow-listed but not in the SUCCESS array]; carried to the state-5 inventory; Task 9 adds the new seed to BOTH). **Four structural corrections** recorded in PLAN.md "PLAN-time SPEC corrections": the config load is SYNC (std::fs, no tokio dep on envoy-config — the fuzz target stays pure); AdminHandler needs NO signature widening (it already holds Arc<Bootstrap>); the cluster-reference checks live at TWO sites (+ a dependent `.expect`); the full downstream-consumer migration set is 7 sites (OutlierManager excluded — it iterates the ClusterManager).
- Evaluated the §6.1 split gate against the §6.2-refined surface (~1480–1600 LoC / 11 tasks; the same at-but-not-over posture as the phase-16/17 no-split decisions; tightly coupled) → **single un-split phase; ADR-0050 does NOT fire.**
- Flipped ROADMAP row `18` `planned → in-progress`. Advanced STATE.md to `18` state-2-complete / state-3-next.

## Task 1 — `envoy-config` schema (`dynamic_resources` + `validate_clusters` + deferred cluster-reference validation)

**Preamble (read before starting):**
- **Goal:** Add `Bootstrap.dynamic_resources: Option<DynamicResources>` (+ the `#[serde(skip)] dynamic_clusters: Option<Vec<Cluster>>` side-field + `all_clusters()` + `cds_configured_but_unloaded()`), the `DynamicResources`/`ConfigSource`/`PathConfigSource` structs (all `deny_unknown_fields`; lds/ads/api_config_source/watched_directory rejected), `RouteConfiguration.validate_clusters: Option<bool>` (parse-and-accept), 3 new `ConfigError` variants (`UnsupportedResourceApiVersion`/`CdsFileError`/`CdsParseError`), and the deferred cluster-reference validation (the two `UnknownCluster` sites + the `Http2ClusterFromHttp1Listener` gate defer iff `cds_configured_but_unloaded()`). TDD per the PLAN Task 1 test list.
- **§6.2 lock-ins that bind this task:** L8 (`resource_api_version` accepts `"V3"`/absent, rejects others); L12b (the `validate_clusters` field is parse-and-accept; envoy-rust's own enforcement is defer-then-revalidate, NOT Envoy's literal runtime-503 semantics); L7 (zero-static-clusters bootstraps are valid — already true via `#[serde(default)]` on `StaticResources.clusters`).
- **Anchors (verified at HEAD `3acf7367b`):** `Bootstrap` `crates/envoy-config/src/bootstrap.rs:8-29` (the xDS-reservation comment at `:19-24` is UPDATED by this task to note phase 18 consumed the reservation); `RouteConfiguration` `:908-912`; `validate` `:1851`; the tcp_proxy cluster check `:2013-2020`; the HCM route check + H2-from-H1 gate `:2205-2227` (the `.expect` at `:2214` must be restructured to `if let` so deferred references cannot panic); `ConfigError` `lib.rs:44-60+`.
- **Carry-forward warning:** `Bootstrap` gained fields — any exhaustive `Bootstrap` struct literal in OTHER crates' tests breaks and must be extended with `dynamic_resources: None, dynamic_clusters: None` in the SAME commit (the phase-16/17 Task-1 workspace-compile lesson). Run `cargo build --workspace --all-targets` before committing.
- **Verification:** `cargo test -p envoy-config` (PASS; the existing UnknownCluster tests at `:3691`/`:6009`/`:6023` must stay green) + `cargo build --workspace --all-targets` + `cargo build -p envoy-config` (standalone, per `project_isolated_crate_build_blindspot`) + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check`.

_(Task entries are appended below by the state-3 executor — one per task, with quoted verification output + the two-stage review verdicts, per the 06.2 → 17 cadence.)_

### Task 1 — COMPLETE (code commit `ce7abce5b`)

**Landed:** `Bootstrap.dynamic_resources: Option<DynamicResources>` (+ `#[serde(skip)] dynamic_clusters: Option<Vec<Cluster>>` side-field + `all_clusters()` + `pub(crate) cds_configured_but_unloaded()`); `DynamicResources`/`ConfigSource`/`PathConfigSource` structs (all `deny_unknown_fields` — lds_config/ads_config/api_config_source/watched_directory rejected, proven by test); `RouteConfiguration.validate_clusters: Option<bool>` (parse-and-accept, L12b doc comment); 3 new `ConfigError` variants (`UnsupportedResourceApiVersion`/`CdsFileError`/`CdsParseError`) + lib.rs re-exports; the `resource_api_version` V3-or-absent validator check; the deferred cluster-reference validation (`defer_cluster_refs` snapshot before the `&mut` listener loop, threaded into BOTH UnknownCluster sites [tcp_proxy + `validate_hcm` new bool param] + the `Http2ClusterFromHttp1Listener` gate's `.expect` restructured to `if let` — structurally panic-free on deferred references); the Node xDS-reservation comment updated (phase 18 consumed the reservation). Workspace-compile fold-in: 2 `Bootstrap` literals (envoy-cluster) + 26 `RouteConfiguration` literal sites (envoy-http1/envoy-http2 hcm.rs incl. `clone_route_config` propagation) extended in the same commit.

**Verification (quoted):**
- `cargo test -p envoy-config` → `test result: ok. 309 passed; 0 failed; 0 ignored` (was 302; +7 new tests — the 6 PLAN-prescribed + a tcp_proxy deferral sibling)
- `cargo build --workspace --all-targets` → clean; `cargo build -p envoy-config` (standalone) → clean
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean; `cargo fmt --all -- --check` → clean

**Two-stage review:**
- **Spec compliance:** 1 issue found (the Node xDS-reservation comment at `bootstrap.rs:89-94` not updated as the PLAN requires) → **fixed in-task** (commit amended `84e796ba1` → `ce7abce5b`); re-verified compliant.
- **Code quality:** zero Critical; **1 Important (forward-looking, dispositioned per-PLAN):** the two reference-check sites consult `static_resources.clusters` (not `all_clusters()`), which Task 3's post-merge re-validation needs — **this is exactly PLAN Task 3 Step 3b** (the check sites migrate to the merged list at Task 3); carried as a binding Task-3 input, NOT a Task-1 defect. Minor notes: `all_clusters()` is defined-but-unused until Task 3 (per-plan); the bool threading through `validate_hcm` judged appropriate (no enum needed).

### Task 2 — COMPLETE (code commit `33ecf55cd`)

**Landed:** new `crates/envoy-config/src/cds.rs` — `parse_cds_file(path: &str, contents: &str) -> Result<Vec<Cluster>, ConfigError>` via the internally-tagged `#[serde(tag = "@type")]` `CdsResource` enum (the ADR-0014 TypedConfig pattern; single `Cluster` variant renamed to the full type URL). Envelope (`CdsFile`) is accept-and-ignore (NO deny_unknown_fields — `version_info`/`type_url`/`nonce` tolerated, L1); per-resource `Cluster` keeps deny_unknown_fields strictness (L4 recorded divergence); malformed YAML / missing `@type` / wrong `@type` → `CdsParseError { path, message }` with serde line/column detail; every parsed cluster runs `validate_cluster`. **Step 3b refactor:** `validate()`'s per-cluster loop body extracted verbatim (order-preserving) into `pub(crate) fn validate_cluster(cluster: &Cluster)` — shared single source of truth between `validate()` and `parse_cds_file`; cross-cluster/listener checks stay in `validate()`. lib.rs: `pub mod cds;` + `pub use cds::parse_cds_file;`.

**Verification (quoted):**
- `cargo test -p envoy-config` → `test result: ok. 317 passed; 0 failed; 0 ignored` (was 309; +8 new cds tests)
- `cargo build -p envoy-config` (standalone) → clean; `cargo build --workspace --all-targets` → clean
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean; `cargo fmt --all -- --check` → clean

**Two-stage review:**
- **Spec compliance:** ✅ compliant (reviewer additionally probe-verified that deny_unknown_fields genuinely fires through the internally-tagged enum on an otherwise-valid cluster — serde_yaml 0.9.34 honors the inner strictness). Implementer deviation accepted: test fixtures carry `lb_policy: ROUND_ROBIN` (mandatory field in the real Cluster schema; the PLAN's draft YAML omitted it).
- **Code quality:** zero Critical / zero Important / 3 Minors — **2 fixed in-task pre-push** (the `rejects_unknown_fields_in_resource` test tightened to a fully-valid-cluster fixture + `CdsParseError`-message assertion so it isolates the deny_unknown_fields gate; the DiscoveryResponse-envelope test now also asserts the cluster name); 1 optional polish (message-assertions on the remaining negative tests) carried to the state-5 inventory.
