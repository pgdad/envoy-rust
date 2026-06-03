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
