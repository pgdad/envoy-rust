# Phase 19 (`19-xds-file-based-lds`) — PROGRESS

> Running log, updated by the executor on each task completion (the 06.2 → 18 cadence).
> One entry per PLAN task; quote the verifying command output. The state-3 arc runs
> `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK
> (NOT deferred to state-4) per `project_state3_arc_skips_clippy`.

**PLAN:** `docs/envoy-rust/phases/19-xds-file-based-lds/PLAN.md`
**SPEC:** `docs/envoy-rust/phases/19-xds-file-based-lds/SPEC.md`
**Scope ADRs:** ADR-0050 (the xDS-family continuation pick [file-based LDS over CDS file watching / RDS / EDS / the blocked gRPC family] + the four §0 findings [every phase-18 extension point is a single-variant reuse; initial-load-only needs no listener-manager refactor; the Envoy-side surface pre-exists as 12 allow-listed names; the LDS+CDS composition exercises the §5.7 merge-ordering invariant] + the minimum-viable scope/deferral ledger). **NO §6.2 reconciliation ADR** — the verification CONFIRMED all three ADR-0051 trigger items (the envelope, the negative-path disposition, the ListenersConfigDump shape/ordering); ADR-0051 remains unconsumed (available to a future phase). ADR-0049's decisions extend to LDS as pre-ratified by ADR-0050 (always-YAML; all-fatal; static-wins; defer-then-revalidate).

---

## State-2 PLAN-write (this commit)

- Performed the HEAVY SPEC §6.2 empirical verification against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`; Docker; foreground general-purpose subagent; LDS+CDS-configured bootstrap [zero static listeners + one static cluster + an LDS file routing `/static` → static cluster, `/dynamic` → CDS cluster] + host backend + admin `/stats` + `/config_dump` scrapes; version string `b0f43d67aa25c1b03c97186a200cc187f4c22db3/1.33.0/Clean/RELEASE/BoringSSL`). Findings L1–L11 locked into PLAN.md "§6.2 empirical lock-ins". **All three ADR-0051 trigger items CONFIRM the projections** (L1 envelope = the CDS-mirror shape; L4 negative-path = the same 3-way Envoy split as CDS → envoy-rust's all-fatal posture extends per ADR-0049 decision 2 / ADR-0050; L5 `configs[]` order = Bootstrap[0] **Clusters[1] Listeners[2]** → **fixture 0026's `configs[1]` assertion HOLDS, no amendment**) → **ADR-0051 does NOT fire**. Refinements locked in: **L6** LDS-delivered route_configs do NOT require `validate_clusters: false` (Envoy skips cluster validation for dynamic listeners; the fixture LDS templates drop it; envoy-rust's defer-then-revalidate posture unchanged — the recorded divergence extends ADR-0049 decision 4 to LDS routes); **L3 ✧** `listener_create_success` is PER-WORKER (never assertable); **L5 ✧** `active_state` carries NO `version_info`; **L7** static-wins listener collision (the CDS L9 mirror); **L11** duplicate-address → `update_rejected`, non-atomic (backstop-design input only).
- Performed the PLAN-time SPEC-correction pass (read-only Explore subagent + controller re-verification by direct grep) against HEAD `8ef6f5b03`. **All 16 anchors CONFIRMED; zero drift** (the corpus is consistent: 29 = 25 SUCCESS + 3 REJECT + 1 minimal). **Five structural corrections** recorded in PLAN.md "PLAN-time SPEC corrections": (1) the D3 consumer sweep is **5 sites** (the SPEC missed the `NoRuntime` gate at `bootstrap.rs:1939`); (2) the `NoRuntime` gate needs the `lds_configured_but_unloaded()` deferral (the `defer_cluster_refs` pattern applied to a second gate); (3) the D4 registration site is a new unit-testable `envoy_listener::register_lds_stats` free function (envoy-listener already deps envoy-config + envoy-stats — `Listener::bind` is the wrong site); (4) corpus arithmetic 29 = 25+3+1 → 30 = 26+3+1; (5) **the LDS templates are PER-SIDE** (`lds-envoy.yaml`/`lds-envoy-rust.yaml`, NOT the projected shared `lds.yaml`) — the LDS payload carries the HCM whose Envoy-only fields (`generate_request_id`/`request_headers_to_remove`) are the established per-side field-set divergence.
- Evaluated the §6.1 split gate against the §6.2-refined surface (**~1250–1450 LoC / 11 tasks**; under the gate with more margin than phases 16/17/18, as the SPEC projected; tightly coupled schema+parse+merge unit) → **single un-split phase; ADR-0052 does NOT fire.**
- Flipped ROADMAP row `19` `planned → in-progress`. Advanced STATE.md to `19` state-2-complete / state-3-next.

## Task 1 — `envoy-config` schema (`lds_config` + the listener side-field + `all_listeners()` + the validator-gate migration)

**Preamble (read before starting):**
- **Goal:** Add `DynamicResources.lds_config: Option<ConfigSource>` (reusing `ConfigSource`/`PathConfigSource` verbatim; `ads_config`/`api_config_source`/`watched_directory` still rejected by `deny_unknown_fields`), the `#[serde(skip)] Bootstrap.dynamic_listeners: Option<Vec<Listener>>` side-field, `Bootstrap::all_listeners()`, `lds_configured_but_unloaded()`, 2 new `ConfigError` variants (`LdsFileError`/`LdsParseError`), the `TooManyListeners` gate migration to the merged list, and the `NoRuntime` gate's LDS deferral. TDD per the PLAN Task 1 test list (8 test groups a–h).
- **§6.2 lock-ins that bind this task:** L4 (the error variants mirror `CdsFileError`/`CdsParseError` — all LDS load errors fatal); the `resource_api_version` V3-or-absent check must cover the new `lds_config` field (PLAN Task 1 Step 3, last paragraph).
- **PLAN-time corrections that bind this task:** Correction 1 (the gates at `bootstrap.rs:1934`/`:1939` are TWO distinct consumer sites); Correction 2 (the `NoRuntime` gate defers iff `lds_configured_but_unloaded()`; the `TooManyListeners` gate needs no deferral — it migrates to `all_listeners().count()` and re-checks naturally post-merge).
- **Anchors (verified at HEAD `8ef6f5b03`):** `DynamicResources` `crates/envoy-config/src/bootstrap.rs:61-66`; `ConfigSource`/`PathConfigSource` `:71-79`; `Bootstrap.dynamic_clusters` + `all_clusters()` `:30-43` + `cds_configured_but_unloaded()` `:49-55` (the patterns to mirror); the gates `:1934-1941`; `ConfigError` (`CdsFileError`/`CdsParseError`) in `lib.rs`.
- **Carry-forward warning:** `Bootstrap` gains a field — any exhaustive `Bootstrap` struct literal in OTHER crates' tests breaks and must be extended with `dynamic_listeners: None` in the SAME commit (the phase-16/17/18 Task-1 workspace-compile lesson; phase 18 hit 2 `Bootstrap` + 26 `RouteConfiguration` literal sites). Run `cargo build --workspace --all-targets` before committing.
- **Verification:** `cargo test -p envoy-config` (PASS; every pre-existing test stays green) + `cargo build --workspace --all-targets` + `cargo build -p envoy-config` (standalone, per `project_isolated_crate_build_blindspot`) + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check`.

_(Task entries are appended below by the state-3 executor — one per task, with quoted verification output + the two-stage review verdicts, per the 06.2 → 18 cadence.)_

---

### Task 1 — COMPLETE (code commit `fda4f8668`)

**Implemented (TDD, RED→GREEN):** `DynamicResources.lds_config: Option<ConfigSource>` (reuses `ConfigSource`/`PathConfigSource` verbatim; `ads_config`/`api_config_source`/`watched_directory` still rejected by `deny_unknown_fields`); `Bootstrap.dynamic_listeners: Option<Vec<Listener>>` (`#[serde(skip)]`); `all_listeners()` + `lds_configured_but_unloaded()` (mirroring `all_clusters()` / `cds_configured_but_unloaded()`); `ConfigError::{LdsFileError, LdsParseError}` (mirroring the Cds pair); the `TooManyListeners` gate migrated to `all_listeners().count()` (merged); the `NoRuntime` gate defers iff `lds_configured_but_unloaded()` and re-enforces post-merge (Corrections 1+2); the `resource_api_version` V3-or-absent check extended to cover BOTH `cds_config` + `lds_config`. 8 new tests (groups a–h). **Carry-forward fixes (the phase-16/17/18 Task-1 lesson):** 4 literal sites in `crates/envoy-cluster/src/cluster.rs` (3 `Bootstrap` + 1 `DynamicResources`) extended. One pre-existing test (`dynamic_resources_rejects_deferred_fields`) dropped its now-obsolete `lds_config`-rejected entry; the `ads_config`/`api_config_source`/`watched_directory` deny-unknown-field gate is retained (test group c reinforces it).

**Verification (quoted):**
- `cargo test -p envoy-config`: **332 passed; 0 failed**.
- `cargo build --workspace --all-targets`: clean.
- `cargo build -p envoy-config` (standalone, per `project_isolated_crate_build_blindspot`): clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.

**Two-stage review:**
- **Spec compliance: ✅** — all 7 requirements + 8 test groups verified by independent code inspection; the removed pre-existing test confirmed legitimately obsolete with deferred-field coverage retained; exact commit message confirmed; only the 3 intended files touched.
- **Code quality: Approve with one Important fix** — a stale `DynamicResources` doc comment (`bootstrap.rs:88-90`) that still claimed `lds_config` was "deliberately NOT a field." **FIXED** and the commit amended (`9fc6ce1` → `fda4f8668`). Two Minor items (resource_api_version double-unwrap; `parse_listener` test helper having no CDS sibling) deliberately skipped as not-worth-churn.

---

### Task 2 — COMPLETE (code commit `3cf3bc4ca`)

**Implemented (TDD, RED→GREEN):** `crates/envoy-config/src/lds.rs` — `parse_lds_file(path, contents) -> Result<Vec<Listener>, ConfigError>` parsing the `@type`-tagged Listener envelope (`type.googleapis.com/envoy.config.listener.v3.Listener`); accepts BOTH the bare `resources:` list and the full DiscoveryResponse (`version_info` accept-and-ignore — envelope is NOT `deny_unknown_fields`); always-YAML; errors map to `LdsParseError`. UNLIKE `parse_cds_file`, it does NOT validate listeners (deferred to Task 3's post-merge re-validation per the §5.7 ordering invariant — documented in module + fn docs). `lib.rs`: `pub mod lds;` + `pub use lds::parse_lds_file;`. 7 tests (groups a–g), faithfully mirroring `cds.rs` and actually stronger (concrete-variant + message-content assertions vs cds.rs's bare `.is_err()`).

**Note — interrupted implementer:** the implementer subagent completed the implementation (file + tests + lib wiring) but hit a server-side 500 error before its self-review/commit. The controller verified all gates and committed; the two-stage review (below) was run with extra rigor to compensate for the missing self-review.

**Verification (quoted):**
- `cargo test -p envoy-config --lib`: **339 passed; 0 failed** (7 new `lds::tests`).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
- `cargo build -p envoy-config` (standalone): clean.
- `cargo fmt --all -- --check`: clean.

**Two-stage review:**
- **Spec compliance: ✅** — signature/module-wiring/envelope/enum/no-validation-difference all verified by independent code inspection; all 7 tests genuine (minimal fixture carries a real HCM filter chain); exact commit message confirmed; no scope creep in the parser itself.
- **Code quality: Approve** — faithful idiomatic mirror of `cds.rs`; the single-variant `.map(|LdsResource::Listener(l)| l)` collect and the `format!`-based DiscoveryResponse test are deliberate sibling-parity choices. Three Minor items, none requiring a fix: (1) the pre-existing untracked `crates/envoy-config/fuzz/Cargo.lock` was swept into this commit by `git add crates/envoy-config/` — a legitimate, conventionally-committed lockfile, left in place to avoid amend-churn; (2)/(3) doc-rationale duplication + `format!` test pattern, both mirror-faithful to cds.rs and kept as-is.

---

### Task 3 — COMPLETE (code commit `cb7e12ba2`, +488/−42)

**Implemented (TDD, RED→GREEN):** restructured `load_dynamic_resources` (`lib.rs`) — removed the early-return-when-CDS-unconfigured; the function now runs the CDS branch, then a parallel LDS branch (read → `LdsFileError`; parse via `lds::parse_lds_file` → `LdsParseError`; L7 merge: static-wins + intra-file-first-wins, each with a `tracing::warn!` paralleling the CDS wording), then exactly ONE post-merge `bootstrap::validate()` gated on `dynamic_clusters.is_some() || dynamic_listeners.is_some()` — the §5.7 ordering invariant (clusters merged BEFORE listener route-references re-validated). Doc comment extended (LDS branch + §5.7 + L6 divergence + the M18-1 on-error-mutation caveat). Per-listener validation loop (`bootstrap.rs`) extended to chain dynamic listeners via a `&mut` split borrow of the two disjoint fields, with the `effective_clusters` snapshot collected BEFORE the split (identity-equivalent when `dynamic_listeners` is None). Consumer migrations: `main.rs` spawn (`first()` → `all_listeners().next()`) + `endpoint.rs` `render_listeners` (`all_listeners()`). 8 envoy-config tests (a–h) + 1 envoy-admin test.

**Sound test-(g) divergence:** omitting `http_filters` fails at PARSE time (required field, no serde default) — would not prove validation-loop coverage. The implementer used `http_filters: []` (parses, then `validate_http_filters` rejects with `EmptyHttpFilters`), genuinely exercising the post-merge per-listener validation path. Verified sound by spec review.

**Verification (quoted):**
- `cargo test -p envoy-config`: **347 passed; 0 failed**. `cargo test -p envoy-admin`: **80 passed; 0 failed**.
- `cargo build --workspace --all-targets`: clean. Standalone `-p envoy-config`/`-p envoy-cluster`/`-p envoy-http1`/`-p envoy-http2`: clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.

**Two-stage review:**
- **Spec compliance: ✅** — restructure verified (CDS behavior preserved, single gated post-merge validate, §5.7 ordering, L7 collision rules); snapshot-before-split-borrow confirmed; both consumer migrations confirmed; all 8 a–h + admin tests genuine; test-(g) reasoning verified sound; exact commit message confirmed; no CDS regression.
- **Code quality: Approve** — zero Critical/Important; faithful CDS-mirror, honest M18-1 caveat, correct split-borrow. Two non-blocking Minor notes (admin-test handler boilerplate could extract `handler_from_bootstrap`; the split-borrow comment slightly overstates borrow-checker necessity since the inline form also compiles) — both left as-is.

---

### Task 4 — COMPLETE (code commit `d24cb52a0`)

**Implemented (TDD, RED→GREEN):** `envoy_listener::register_lds_stats(&Bootstrap, &StatsRegistry) -> Result<(), ListenerError>` — a NO-OP early-return when `dynamic_resources.lds_config` is unconfigured (the §5.2 inertness invariant); otherwise registers the conditional family: `listener_manager.lds.update_attempt`(+1), `lds.update_success`(+1), `lds.update_failure`(0), `lds.update_rejected`(0), `listener_manager.listener_added`(= `all_listeners().count()`, includes static). `total_listeners_active` is NOT registered here (keeps its unconditional 08.2-D14 registration in `Listener::bind`). main.rs call site placed after registry construction AND after `load_dynamic_resources` (so `listener_added` counts merged dynamic listeners). Faithful mirror of the phase-18 `cluster_manager.cds.*` template. 3 tests (a conditional-registration incl. the cds-but-no-lds inertness witness / b 5-name subset / c static-inclusion). API confirmed: `StatsRegistry::register_counter` is idempotent for same-name/same-kind.

**Verification (quoted):**
- `cargo test -p envoy-listener`: **33 passed; 0 failed** (3 new lds tests).
- `cargo build --workspace --all-targets`: clean. `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.

**Two-stage review:**
- **Spec compliance: ✅** — exact stat-name literals (no typos), `lds_config`-specific guard, all 5 values, total_listeners_active carve-out, call-site ordering (after registry + load), and all 3 tests verified by independent inspection; exact commit message.
- **Code quality: Approve** — faithful literal CDS-template mirror; error idiom matches `Listener::bind`; `as u64` cast sound. Three stylistic Minors (guard polarity `.is_none()`+early-return vs CDS `.is_some()`-wrap; hand-built test `Bootstrap` — required because YAML round-trip won't populate the `#[serde(skip)]` side-field; `counter_value` lookup helper) — all justified, no change.

---

### Task 5 — COMPLETE (code commit `10c44ca25`, +436 single file, purely additive)

**Implemented (TDD, RED→GREEN):** `ConfigDumpEntry::Listeners` variant (`@type` = `type.googleapis.com/envoy.admin.v3.ListenersConfigDump`) + 4 serializer structs (`StaticListenerEntry`/`DynamicListenerEntry`/`ListenerActiveState`/`TaggedListener`) mirroring the phase-18 CDS sibling, with the ONE intended divergence: the dynamic entry nests `dynamic_listeners[].active_state.{listener,last_updated}` (an extra level vs the CDS flat `dynamic_active_clusters[].cluster`); inner listener `@type` = `type.googleapis.com/envoy.config.listener.v3.Listener` via `#[serde(flatten)]`; NO `version_info` key (L5 ✧); both vecs `skip_serializing_if = Vec::is_empty`. `StaticListenerEntry` carries `last_updated` (matches Envoy's real static-listener shape; CDS's `StaticClusterEntry` has none). `render_config_dump`: conditional `Listeners` push gated on `lds_config.is_some()`, placed AFTER Clusters (configs[] order Bootstrap[0]/Clusters[1]/Listeners[2]); `last_updated` reuses the single shared render-time ISO-8601 value. 6 tests (a conditional-emission incl. cds-only inertness witness / b 3-entry order-lock + active_state nesting + no-version_info + ISO-8601 / c static_listeners-key-omission + inverse / d §5.5 Bootstrap separation).

**Verification (quoted):**
- `cargo test -p envoy-admin`: **86 passed; 0 failed** (6 new; the fixture-0014/0026 config_dump regression tests intact — zero deletions in the diff).
- `cargo build --workspace --all-targets`: clean. `cargo build -p envoy-admin` (standalone): clean. `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.

**Two-stage review:**
- **Spec compliance: ✅** — exact type-URL strings, active_state nesting, no-version_info, `skip_serializing_if`, lds-specific gating, Clusters-before-Listeners ordering, shared ISO-8601 source; all 4 test groups genuine; no regression to existing config_dump tests (diff purely additive).
- **Code quality: Approve** — faithful CDS-sibling mirror with exactly the intended divergences; healthy ~90 production / ~343 test line split. Minor test-helper/fixture duplication (`handler_from_bootstrap`/`parse_cluster`/`DYNAMIC_BACKEND_CLUSTER` re-copied — direct `Bootstrap` construction genuinely required since `handler_with_bootstrap` can't inject the `#[serde(skip)]` side-fields; consistent with the file's per-module self-containment convention) — a recurring M18-9-class extract-shared-helper item, left as-is.

---

### Task 6 — COMPLETE (code commit `7af301f67`)

**Implemented (TDD, RED→GREEN):** generalized the phase-18 `{{CDS_PATH}}` differential-harness machinery to a second dynamic file `{{LDS_PATH}}`. `upstream.rs`: `LDS_CONTAINER_PATH = "/etc/envoy-lds/lds.yaml"` (`.yaml` per L1) + `upstream::start` gains `lds_file: Option<&Path>` (mounted via `with_copy_to`). `lib.rs`: `needs_lds` detection; PER-SIDE template read (`lds-envoy.yaml` upstream + `lds-envoy-rust.yaml` subject — NOT shared, because the LDS payload carries the HCM with Envoy-only fields the envoy-rust parser rejects; hard error on a missing per-side file); kv-map injection (upstream → container path, subject → host temp path); per-side render + dual residual-marker fail-fast; threaded into `upstream::start`. `uses_host_gateway` generalized to a slice signature `(&[&str])` (all call sites + 4 CDS-render-test assertions migrated). 4 non-Docker render-path tests (a–d).

**The load-bearing two-scan correctness (the phase-18 escaped-Critical bug class) — CONFIRMED correct:** `scan_needs_marker` (backend detection) scans the UNRENDERED upstream LDS template (`{{...BACKEND_PORT}}` markers exist only pre-render); `uses_host_gateway` (line 2631) scans the RENDERED `upstream_lds_yaml` (`host.docker.internal` appears only after `{{BACKEND_HOST}}` substitution). Dataflow ordering verified (rendered LDS at :2604 precedes the scan at :2631). Test (d) is the regression guard: it asserts the negative baselines (main+CDS alone → false for both scans) AND the positive with-LDS cases, proving the LDS source is load-bearing.

**Verification (quoted):**
- `cargo test -p differential --lib`: **123 passed; 0 failed; 1 ignored** (the Docker-gated test).
- `cargo build --workspace --all-targets`: clean. `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.

**Two-stage review:**
- **Spec compliance: ✅** — all 5 requirements + the two-scan correctness verified by code inspection (the highest-priority check: `scan_needs_marker`=unrendered, `uses_host_gateway`=rendered, dataflow ordering confirmed); all 4 tests genuine; all `uses_host_gateway` callers migrated; exact commit message.
- **Code quality: Approve** — faithful CDS-machinery mirror with the per-side read as the one justified divergence; the correctness-critical unrendered-vs-rendered distinction is explicitly commented; test (d) docstring names the phase-18 Critical and explains the negative baseline. Two Minor nits (the LDS render block's tuple `if let` could key on `needs_lds`; `upstream::start` now has two adjacent same-typed `Option<&Path>` params) — both left as-is.

---

### Task 7 — COMPLETE (code commit `1a2e18b85`, 8 files added)

**Implemented:** fixture `0027-xds-file-based-lds` realizing Envoy's canonical LDS+CDS filesystem-dynamic-config topology — `envoy.yaml`/`envoy-rust.yaml` (byte-identical main configs: node + admin + `dynamic_resources` with BOTH lds_config + cds_config + one static `static_backend` cluster + ZERO static listeners), per-side `lds-envoy.yaml`/`lds-envoy-rust.yaml` (the Envoy side carries `generate_request_id: false` + `request_headers_to_remove` for byte-exact echo and binds `0.0.0.0`; the subject side omits those and binds `127.0.0.1`; NEITHER carries `validate_clusters` per L6), shared `cds.yaml` (`dynamic_backend`, 0026 shape), `expectations.yaml` (`http1_keep_alive`: 2 probes `/static`+`/dynamic` → 200 byte-exact echo + `x-envoy-upstream-service-time`; 18 stats incl. `cluster_added`/`active_clusters` = 2 [1 static + 1 dynamic] and `downstream_rq_total`/`2xx` = 2 [2 probes/keep-alive]; 3 admin scrapes locking ListenersConfigDump@configs[2] + ClustersConfigDump@configs[1] + `/listeners` per-side address allow-lists), `README.md`, and the Docker-gated wrapper.

**Differential result: PASS bilaterally on the first run** (`test xds_file_based_lds_fixture ... ok`, 5.43s; envoy-rust bound the LDS-delivered listener on `127.0.0.1`, upstream Envoy v1.33.0 in Docker; both probes 200/200, all 18 stats, all 3 admin scrapes matched). The load-bearing differential proof: a listener that exists ONLY in the LDS file accepts and serves traffic bilaterally. **Regression witnesses (Step 10) all PASS:** `admin_stats_prometheus`, `admin_config_dump_server_info`, `xds_file_based_cds`, `http1_router_upstream` (inertness — zero behavior change).

**Verification (quoted):**
- Docker fixture 0027: **PASS bilaterally**. Regression witnesses: **4/4 PASS**.
- `cargo build --workspace --all-targets`: clean. `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.

**Two-stage review:**
- **Spec compliance: ✅** — every fixture file verified against the PLAN + sibling 0026; ALL expectations.yaml keys confirmed real struct fields (deny_unknown_fields structs — no silently-ignored typos); echo bodies confirmed byte-exact & non-trivial against the `http1-echo-server` helper (only `host` survives the Envoy-side stripping); all 18 stat values correct & complete (esp. cluster_added==2); config_dump indices + /listeners allow-lists correct; harness plumbing confirmed wired (not Docker-green-by-accident); exact commit message.
- **Code quality: Approve with one fix (applied)** — minimal/well-documented per-side LDS divergence (exactly the 3 intended deltas), byte-identical main configs the right call, no stale copy-paste. One Important factual miscount FIXED ("eight lock-in #L3 names" → "six" in expectations.yaml + README, contradicting the adjacent 6-name block) + two Minor doc-clarity items addressed (wrapper Docker-gating note for 0026 parity; the README "four scanned renditions" rewording). Commit amended `3b05d82ad` → `1a2e18b85` (doc-only — bilateral Docker pass stands).

---

### Task 8 — COMPLETE (code commit `8798e7705`, new ~811-line test file)

**Implemented (TDD):** `crates/envoy-bin/tests/xds_file_based_lds.rs` — the in-process backstop covering the paths the differential fixture cannot. 6 subprocess tests (spawn real envoy-bin): (1) `happy_path_dynamic_listener_serves_and_reports` — both probes 200, the 6 listener_manager stats, `/config_dump` ListenersConfigDump at `configs[2]` (by `@type`, not just substring), `/listeners` `dynamic_listener::`; (2) `missing_lds_file_is_fatal` → non-zero exit + `reading LDS file`; (3) `malformed_lds_file_is_fatal` → `parsing LDS file`; (4) `lds_route_to_unknown_cluster_is_fatal` → `unknown cluster 'nope'` (the L6 recorded divergence — envoy-rust fails startup where Envoy 503s at runtime); (5) `static_dynamic_listener_collision_static_wins` — port A serves, port B refused (dynamic skipped), `listener_added == 1` (the L7 static-wins end-to-end proof); (6) `no_lds_config_is_inert` — fixture-0026 topology: zero `listener_manager.lds.*` + no `listener_added` + no `"ListenersConfigDump"` (the §5.2 / fixture-0026 compatibility witness). Helpers copied verbatim from the CDS backstop (M18-9 duplication noted in the header); the shared `assert_fatal_startup` drains both pipes under a 10s budget. No Task-1–7 bug surfaced; no test weakened to pass.

**Verification (quoted):**
- `cargo test -p envoy-bin --test xds_file_based_lds`: **6 passed; 0 failed; 0 ignored** (run serially/isolated per `project_flaky_access_log_fixture_0012`; green twice).
- `cargo build --workspace --all-targets`: clean. `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.

**Two-stage review:**
- **Spec compliance: ✅** — all 6 tests present, correctly named, ZERO `#[ignore]`; negative tests assert non-zero exit AND the specific `thiserror` Display substrings (no false-pass — fresh ports + valid CDS make the LDS fault the only failure cause); collision proves all three conditions; inertness asserts the absence triad; happy-path config_dump checks `configs[2]` by `@type`; exact commit message.
- **Code quality: Approve** — faithful verbatim CDS-backstop mirror; `assert_fatal_startup` a clean 3-path abstraction with correct pipe-drain discipline; flake-resistant (reserved ports, bounded waits, kill_on_drop); the collision "port B refused" check proven race-free against source (the dynamic listener is never bound after the merge drops it). Only cosmetic Minors (a `write!`-vs-`push_str(&format!)` builder nit; an unused admin_port in the fatal tests faithfully mirroring CDS; a slightly-misleading "Host header" comment) — none warranting change.

---

### Task 9 — COMPLETE (code commit `d9a7e2a10`, atomic 3-file edit)

**Implemented:** new fuzz seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/dynamic_resources_lds.yaml` (a bootstrap with BOTH `lds_config` + `cds_config`, zero static listeners, admin, one static cluster — exercises the new schema surface) added ATOMICALLY in one commit to all three sites: the seed file, the `.gitignore` `!corpus/parse_bootstrap/...` allow-list (after the cds sibling), and the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array (`// 19 Task 9`, after the cds entry). Corpus arithmetic: **30 tracked = 30 allow-list = 26 SUCCESS + 3 REJECT + 1 minimal** (the seed PARSES — `parse_bootstrap` is pure / never reads the referenced files, and the `NoRuntime` gate defers on `lds_configured_but_unloaded` with admin present).

**Verification (quoted):**
- `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly`: **1 passed; 0 failed**. `cargo test -p envoy-config` (full): **347 passed; 0 failed**.
- Nightly fuzz sanity: `cargo +nightly fuzz run parse_bootstrap -- -runs=10000 -timeout=10` → **clean** (21042 runs in 8s, no crashes/artifacts). [The full 200k-run gate is Task 11.]
- `cargo build --workspace --all-targets`: clean. `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.

**Two-stage review:**
- **Spec compliance: ✅** — atomicity (all 3 edits in one commit), seed well-formed + in SUCCESS array, `.gitignore`/array styles match siblings, corpus arithmetic verified by `git ls-files`=30 and array counts 26+3+1=30; exact commit message; no stray artifacts.
- **Code quality: Approve** — clean YAML, accurate comment (no `cds` copy-paste leftover), entries follow sibling conventions; two immaterial cosmetic Minors (compact-vs-expanded `admin:` form; `fuzz-lds` node id), no change.

---

### Task 10 — COMPLETE (code commit `e7d436d30`, docs-only +128/-1)

**Implemented:** three `docs/envoy-rust/BEHAVIOR_CONTRACT.md` additions mirroring the phase-18 CDS rows. (1) The "19 entries (file-based LDS)" 6-row stat table (`listener_manager.lds.{update_attempt=1,update_success=1,update_failure=0,update_rejected=0}`, `listener_added=1`, `total_listeners_active=1`) + the §5.2 conditional-registration narrowing paragraph + the L3 Envoy-only enumeration paragraph (21 Envoy names; the 15 unasserted incl. the per-worker `listener_create_success` ✧ caveat; the co-asserted `cluster_added`/`active_clusters`=2). (2) The xDS "Filesystem transport — phase 19 LDS extension" subsection (a)–(f) (envelope/type-URL, readiness+§5.7, the negative-path 3-way-split-vs-all-fatal divergence table, static-wins collision, the L6 validate_clusters/route divergence, the L10 conditionality narrowing). (3) The ListenersConfigDump admin-body-shapes row (conditional, `configs[2]`, `active_state.listener` nesting, NO `version_info`, empty-key omission, bilateral anchor, the LDS-only-bootstrap index caveat) + the `/listeners` annotation. Cross-read against fixture 0027, the backstop, and the emitter source — no discrepancies.

**Verification (quoted):**
- `cargo build --workspace`: clean (vacuous — docs-only; confirms no stray damage).

**Two-stage review:**
- **Spec compliance: ✅** — all 3 additions complete; EVERY stat value cross-checked against fixture 0027's expectations.yaml; the ListenersConfigDump shape cross-checked against the real `crates/envoy-admin/src/endpoint.rs` emitter (not just the PLAN); the negative-path dispositions cross-checked against the backstop; the per-worker caveat + 15-name enumeration accurate; no invented values; exact commit message; docs-only.
- **Code quality: Approve with two fixes (applied)** — excellent structural parallelism with phase 18, clean edit (the `-1` is the expanded `/listeners` row), well-formed markdown, no stale CDS copy-paste. Two Minor prose nits FIXED (a self-contradictory "base `lds.*`" qualifier reworded to "`lds.*` subtree + the base `listener_added` name"; an orphan `✧` glyph removed, keeping the intentional per-worker callout). Commit amended `8ea5c6049` → `e7d436d30`.

---

### Task 11 — state-4 phase-done verification — COMPLETE

The state-3 execution arc (Tasks 1–10) and this state-4 verification completed in one session (the phase-18 cadence); each task landed one code commit + one PROGRESS commit with passing two-stage review (spec-compliance THEN code-quality), TDD, and per-task clippy. **All §7.5 (a)–(e) gates GREEN.**

**§7.5 (e) — stable-toolchain gates (local; quoted):**
- `cargo build --workspace --all-targets`: `Finished` (exit 0).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: `Finished` (exit 0, zero warnings).
- `cargo fmt --all -- --check`: clean (exit 0).
- `cargo test --workspace`: **1097 passed; 0 failed** (across all binaries). [First run surfaced one failure — fixture 0021's in-process backstop (`upstream_h2_connection_pooling`) at `backend ready: ConnectionRefused` — root-caused to the documented cold-helper-compile flake (`project_flaky_access_log_fixture_0012`): the backstop spawns `http2-echo-server` via `cargo run`, whose cold compile (>30s) exceeds the readiness window. Phase 19 touched no H2-pooling/helper code. Cleared by pre-building all 5 `tests/helpers/*` binaries (http1-echo-server alone is 1m14s cold); the full re-run is 1097/0.]
- `cargo deny check`: `advisories ok, bans ok, licenses ok, sources ok` (exit 0).

**§7.5 (e) — 4 standalone-crate builds (`project_isolated_crate_build_blindspot`; quoted):** `cargo build -p envoy-config` / `-p envoy-cluster` / `-p envoy-http1` / `-p envoy-http2` — all `Finished` (exit 0).

**§7.5 (d) — fuzz short-budget gate (local; quoted):** `cargo +nightly fuzz run parse_bootstrap -- -runs=200000 -timeout=10` → `Done 200000 runs in 17 second(s)`, zero crashes/leaks on the 30-seed corpus.

**§7.5 (a)+(b)+(c) — the Docker-gated CI anchor run:** pushed HEAD `759686acd` (Tasks 1–10: 20 commits); **CI run `26903181658` — `conclusion=success`, both jobs green:** `build + test + lint => success` (the full Docker differential suite on ubuntu-latest) and `fuzz (parse_bootstrap, 30s) => success`. Confirmed in the CI log: `test xds_file_based_lds_fixture ... ok` (fixture 0027 — the load-bearing differential proof, green on Linux), `test xds_file_based_cds_fixture ... ok` (the fixture-0026 regression witness), the full differential suite green (all 27 Docker-gated fixtures 0001–0027 simultaneously), and `h2spec_pass_rate_gate ... ok` (the ≥95% conformance gate maintained). (a) fixture 0027 green ✅ / (b) all 26 pre-existing fixtures still green ✅ / (c) h2spec ≥95% ✅.

**ADR posture:** ledger head stays **ADR-0050** (count 51); the state-3 arc landed NO new ADR (no §6.1 mid-execution split fired — ADR-0052 unconsumed; ADR-0051 remains free). **ADR-0014 in force; ADR-0028 open.**

Per §5.1, the state-5 code review (`superpowers:requesting-code-review` → REVIEW.md) is a SEPARATE next session. STATE advanced to state-4-complete / state-5-next at this commit.
