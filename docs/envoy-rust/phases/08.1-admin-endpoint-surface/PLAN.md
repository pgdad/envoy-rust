# Phase 08.1 (`08.1-admin-endpoint-surface`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended per the user's standing preference auto-memory `feedback_execution_style`) — fresh subagent per task + two-stage review. Steps use checkbox (`- [ ]`) syntax for tracking. Tasks land in numbered order; this PLAN.md commits ALONGSIDE the PROGRESS.md skeleton (with the Task 1 preamble) at state-2 per the 07.2 (`c7dea4c`) / 06.2 (`dc00750`) cadence — NO code changes at state-2. Tasks 1-13 each land as their own state-3 commit; Task 14 is the state-4-reached / state-5-next STATE-advance commit.

**Goal.** Expand `crates/envoy-admin/` from its 3-endpoint 06.1 surface (`/ready`, `/stats`, `/stats/prometheus`) to a **7-endpoint GET-only admin surface** by adding **4 new GET endpoints** (`/server_info`, `/config_dump`, `/clusters`, `/listeners`) and the dispatch/serialization infrastructure that 08.2's POST endpoints plug into. Close the three 06.1 admin REVIEW carryforwards (I2 / M1 / M4) as a Task-1 preamble so the new endpoints land on the cleaned-up `serialize_response` + dispatch surface. Prove via the new differential fixture `0014-admin-config-dump-server-info` that the new endpoint bodies are equivalent to upstream Envoy under the per-endpoint dispositions documented in BEHAVIOR_CONTRACT.md's new "Admin endpoint body shapes" subsection.

**Architecture.** Hand-rolled per D-3.2 (*Admin API* is on the **Must be written from scratch** list). All 4 new endpoint variants land in `crates/envoy-admin/src/endpoint.rs` as additional `AdminEndpoint` variants; the `AdminEndpoint::from_path` → `dispatch(method, path) -> Dispatch` refactor widens the existing single-arg surface to method+path. `/config_dump` + `/server_info` render JSON via `serde_json::to_vec_pretty`; `/clusters` + `/listeners` render plain text. The `Bootstrap` struct in `envoy-config` gains a `Serialize` derive cascade across ~30 transitively-owned types (mechanical; existing `#[serde(rename = ...)]` annotations apply to both sides). `AdminHandler::new` widens from 2-arg to 5-arg signature; `envoy-bin::main` threads three new handles (`Arc<Bootstrap>`, `Arc<ClusterManager>`, `Instant`). Phase 08.1 wires ZERO new stats. No new top-level Cargo deps; no new ADRs under the recommended posture.

**Tech Stack.** New permitted-foundations: NONE. `serde_json` (already on the D-3.2 permitted-foundations list; already a transitive dep via the existing fixture harness) is the only foundation engaged. New workspace member: NONE. Modified workspace members: `envoy-config` (Serialize derive cascade), `envoy-admin` (4 new endpoints + dispatch refactor + handler widening), `envoy-listener` (DRAIN_BUDGET hoist), `envoy-cluster` (`.clusters()` accessor on `ClusterManager`), `envoy-accesslog` (one `pub fn format_iso8601` wrapper exposed), `envoy-bin` (shared-handle wiring), `tests/differential` (`BodyRule::JsonShape` + `BodyRule::TextLines`). New differential fixture `tests/fixtures/0014-admin-config-dump-server-info/` + Docker-gated wrapper `tests/differential/tests/admin_config_dump_server_info.rs` + in-process backstop `crates/envoy-bin/tests/admin_config_dump_server_info.rs` + fuzz corpus seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_multi_endpoint_bootstrap.yaml`. `cargo deny check` is a no-op for top-level deps but MUST be quoted in every task's PROGRESS attestation (07.1-REVIEW doctrine reminder; ratified in 07.2 / 06.3 / 06.2 / 06.1).

---

## PLAN-write SPEC corrections (recorded here + in PROGRESS.md Task 1 preamble)

The 08.1 SPEC landed at the parent-08 state-2 split commit `56dee82`, derived from the parent-08 state-1 SPEC committed at `0202e38`. Six SPEC details drifted against the 07.2-landed tree (verified against HEAD `56dee82`). Per the user's standing preference `feedback_pick_recommendation`, each correction picks the working option; all are folded into the task steps below.

1. **`DRAIN_BUDGET` hoist target site is a *module-level* `pub const`, not a `pub const` at the existing local-fn site.** SPEC §3 D3 says "Hoist `pub const DRAIN_BUDGET: Duration = Duration::from_secs(5)` from `crates/envoy-listener/src/lib.rs:165` ... into a re-exported position." The current `crates/envoy-listener/src/lib.rs:165` site is `const DRAIN_BUDGET: Duration = Duration::from_secs(5);` declared *inside* the body of `Listener::serve` (a local-fn const). And `crates/envoy-admin/src/handler.rs:28` declares a separate module-level `const DRAIN_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);`. **Correction:** the hoist *deletes* both existing declarations and introduces a single new `pub const DRAIN_BUDGET: Duration = Duration::from_secs(5);` at module level in `crates/envoy-listener/src/lib.rs` (after the existing `use` block; before the first `pub struct`/`pub enum`/`pub fn`). `envoy-admin` already depends on `envoy-listener` (verified at `crates/envoy-admin/Cargo.toml:20`); the import is one-line: `use envoy_listener::DRAIN_BUDGET;`. The listener's `Listener::serve` swaps its local-const `DRAIN_BUDGET` for the module-level `DRAIN_BUDGET` from the same crate (no `use` change needed inside the same module). The 3 use-sites at `lib.rs:165, 244, 245, 249` (and the doc reference at `lib.rs:152` + the test references at `lib.rs:385, 424`) carry forward unchanged in their textual form — they refer to the same identifier; only the `const` declaration line at `lib.rs:165` is deleted.

2. **`format_iso8601` reuse for `/config_dump` `last_updated` requires visibility promotion + crate dep addition.** SPEC §3 D6 says "`last_updated` is the ISO-8601 timestamp at request-render time (reuses the 06.2-landed `envoy_accesslog::default_format::format_iso8601`)." Verified at `crates/envoy-accesslog/src/default_format.rs:83`: the function signature is `pub(crate) fn format_iso8601(s: &mut String, t: SystemTime)` — `pub(crate)` (not reachable from `envoy-admin`) AND the `default_format` module is not declared `pub` in `crates/envoy-accesslog/src/lib.rs` (private module). Additionally, `crates/envoy-admin/Cargo.toml` does not currently depend on `envoy-accesslog`. **Correction:** at Task 5 (the D13a shared-handle wiring task) add a `pub fn format_iso8601(t: SystemTime) -> String` wrapper to `crates/envoy-accesslog/src/lib.rs` that calls the existing `pub(crate)` internal function (`let mut s = String::new(); default_format::format_iso8601(&mut s, t); s`); declare `pub mod default_format;` is NOT necessary — the wrapper hides the internal function's `&mut String`-writer signature behind a returning-`String` public API at the lib root, which is the more ergonomic shape for `envoy-admin`'s one call-site (`last_updated`). Add `envoy-accesslog = { path = "../envoy-accesslog" }` to `crates/envoy-admin/Cargo.toml`. Closes the 06.2 `Sink` trait deferral-related coupling concern by adding `envoy-accesslog` as a `[dependencies]` only (not `[dev-dependencies]`) — admin is a downstream consumer of one helper symbol.

3. **`ClusterManager` does NOT currently expose a `.clusters()` accessor.** SPEC §6.5 says "envoy-cluster already exposes ClusterManager … Phase 08.1 needs `.clusters() -> impl Iterator<Item = &Cluster>` (or `&[Cluster]`) for D7's `/clusters` rendering. The planner verifies the accessor exists; if not, adds it as a sub-task prefix to D7. Should be ~5 LoC at most." Verified at `crates/envoy-cluster/src/cluster.rs:201-228`: `ClusterManager` has `.get(&name) -> Option<ClusterHandle>` + `.empty() -> Self` only — no iterator/slice accessor over the cluster set. **Correction:** Task 8 (D7) lands `pub fn clusters(&self) -> impl Iterator<Item = ClusterHandle> + '_` (or alternatively `pub fn cluster_names(&self) -> impl Iterator<Item = &str> + '_` if the per-cluster numeric counters can be reached via `.get(name).unwrap()` — the executor picks the lighter shape that supports both `<name>::observability_name::<name>` and `<name>::default_priority::endpoints` line emission per Envoy v1.33's `/clusters` plain-text format). ~5-10 LoC + 1 unit test.

4. **`AdminHandler::new` current signature is the 2-arg shape the SPEC describes; D13a widens to 5-arg.** Verified at `crates/envoy-admin/src/handler.rs:36`: `pub fn new(config: Arc<AdminConfig>, registry: Arc<StatsRegistry>) -> Self`. Six call sites in the same file's `#[cfg(test)] mod tests` block (lines 291, 318, 341, 363, 386, 416, 461) must update their call signatures at Task 5. Additionally `crates/envoy-bin/src/admin.rs` (the production wiring) calls `AdminHandler::new(...)` once — that call site also updates. **No SPEC correction; this is just confirmation that the SPEC's description matches the tree. Recording for the executor's pre-task drift verification.**

5. **`Bootstrap` and transitively-owned types: current derive convention is `#[derive(Debug, Deserialize)]` or `#[derive(Debug, Deserialize, PartialEq)]` (some with `Default`); NO `Serialize` derives anywhere in `crates/envoy-config/src/bootstrap.rs`.** Verified at `crates/envoy-config/src/bootstrap.rs:8-380+`. **The Serialize cascade in Task 4** is mechanical: every `#[derive(Debug, Deserialize, ...)]` becomes `#[derive(Debug, Serialize, Deserialize, ...)]`; existing `#[serde(rename = "...")]` field renamings + `#[serde(rename_all = "...")]` enum renamings apply to BOTH sides (this is serde's documented contract); existing `#[serde(default)]` field defaults apply to the Deserialize side only (Serialize emits the literal value, default or not). One known caveat per SPEC §5.3: YAML allows certain casings/syntax that JSON does not — the Task 4 roundtrip sanity check (YAML→struct→JSON→struct equality on fixture 0008's `envoy-rust.yaml`) catches surprises here. **The cascade touches every `#[derive(...)]` line in `crates/envoy-config/src/bootstrap.rs` that mentions `Deserialize` — projected ~25-32 derive-line edits (the executor counts before-and-after; a one-pass `s/Deserialize/Serialize, Deserialize/g` is mechanically correct only after verifying no derive line *already* contains `Serialize`, which the empirical scan at PLAN-write time confirms).** No structural changes to fields; no `#[serde(skip_serializing)]` / `#[serde(skip_serializing_if = ...)]` annotations needed for 08.1 (the JSON output mirrors the YAML input verbatim modulo serde's renaming canonicalization — the harness's `JsonShape::required_subtree` rule covers the diff dimensions).

6. **`AdminEndpoint::from_path` returns `Option<Self>` and takes only `path: &str`** — confirmed at `crates/envoy-admin/src/endpoint.rs:27`. D4 widens to `dispatch(method: &str, path: &str) -> Dispatch`. The existing `handle_inner` GET-only 405-method-allowlist path at `crates/envoy-admin/src/handler.rs:146` calls `render_405()` without a per-endpoint `Allow:` header — D4's `Dispatch::MethodNotAllowed { allow: &'static str }` plumbs the value through; `render_405` is extended to take `allow: &'static str`. The 06.1 `from_path` and `render` methods are retained (the dispatch refactor adds a new `dispatch` method + a new `Dispatch` enum; D4 keeps `from_path` for backward compatibility within the crate — the call site at `handle_inner` migrates from `from_path` to `dispatch`; the existing `from_path` tests at endpoint.rs:146-176 stay green). 08.2's POST endpoints can opt to call `dispatch` exclusively; 08.1 leaves `from_path` reachable as a thin convenience wrapper but adjusts `handle_inner` to use `dispatch`.

---

## Architecture decisions locked at PLAN-write time (signpost choices)

Per 08.1 SPEC §6's implementation signposts + §7 ADR posture, the planner picks the recommendation so the executor does not re-litigate mid-task. Per the user's standing preference `feedback_pick_recommendation`, every signpost with a "recommended posture" gets that recommendation.

| # | Signpost | Decision | Rationale |
|---|---|---|---|
| 1 | Carryforward-closure ordering | **D1+D2 co-located (Task 1) → D3 (Task 2) → D4 (Task 3); Task-1 preamble runs strictly before D4 + D5-D8.** | SPEC §6.2 — D1/D2/D3 land first, before any new endpoint. |
| 2 | D6 vs D5 ordering | **D6 (`/config_dump`) lands BEFORE D5 (`/server_info`)** as Task 6 vs Task 7. | SPEC §6.3 — "D6 leading is recommended" (the `Bootstrap` Serialize cascade is the known mechanical risk surface; landing it first reveals any field-renaming / YAML-vs-JSON-roundtrip surprises before they compound). |
| 3 | `Bootstrap` Serialize cascade landing site | **Task 4 (dedicated task before D6).** Includes the SPEC §6.4 pre-D6 YAML→struct→JSON→struct sanity-check on fixture 0008's `envoy-rust.yaml`. | SPEC §6.4 — the planner adds the sanity-check as a sub-task immediately before D6's main work. Promoting to a dedicated task isolates the mechanical-risk surface. |
| 4 | `format_iso8601` reuse | **Add a `pub fn format_iso8601(t: SystemTime) -> String` wrapper at `envoy-accesslog::lib.rs` (calls the existing `pub(crate)` internal); add `envoy-accesslog` dep to `envoy-admin/Cargo.toml`.** Lands at Task 5 (D13a) alongside the AdminHandler signature widening. | PLAN-write correction 2 above — the cleanest visibility-promotion path that does not require declaring `pub mod default_format` (keeps the internal `format_iso8601` impl's `&mut String`-writer signature internal). |
| 5 | `ClusterManager::clusters()` accessor shape | **`pub fn clusters(&self) -> impl Iterator<Item = ClusterHandle> + '_`** (returns `ClusterHandle` per `.get`'s existing return type; the per-cluster numeric-counter accessors live on `ClusterHandle`). ~10 LoC + 1 unit test. Lands at Task 8 as the D7 prerequisite. | PLAN-write correction 3 above. |
| 6 | `/server_info.state` placeholder source | **`state: &'static str = "LIVE"` as a hardcoded constant in `endpoint.rs::render_server_info`.** Structural-shape commitment per SPEC §5.4 — 08.2's D5e patches the value-binding source from the constant to `match drain.current() { Live | HealthcheckFailing => "LIVE", Draining => "DRAINING" }`; the field type and position in the struct do not change. | SPEC §3 D5 + §5.4. |
| 7 | `/server_info.command_line_options` shape | **`BTreeMap<String, serde_yaml::Value>` built once at handler construction time** from the parsed CLI (currently just `{ "config_path": "<-c value>" }`). Threaded via D13a as a new field on `AdminHandler`. | SPEC §3 D5 + parent-08 SPEC §3 D5 — "admin-listener internals can serialize this lazily on first request to avoid threading more state at construction time." Building at construction time is mechanically simpler and matches the existing AdminConfig-at-construction pattern; cost is one BTreeMap allocation per process start. |
| 8 | `/server_info` `node` + `command_line_options` Serialize derives | **The `Bootstrap` Serialize cascade (Task 4) already covers `envoy_config::Node`. `serde_yaml::Value` already implements `Serialize`. No extra derive work in Task 7.** | Mechanical consequence of Task 4. |
| 9 | `/config_dump` `ConfigDumpBody` shape | **Top-level `{ "configs": [ { "@type": "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump", "bootstrap": <Bootstrap-as-JSON>, "last_updated": "<ISO-8601>" } ] }`** per SPEC §3 D6. The 08.1 envoy-rust emits exactly one entry; Envoy emits this entry plus xDS-derived siblings (the harness allow-lists the latter as envoy-only). The `@type` tag is rendered via `#[serde(tag = "@type")]` on a `ConfigDumpEntry` enum. | SPEC §3 D6. |
| 10 | `/clusters` plain-text format | **One cluster-stanza per cluster, with the two SPEC-mandated lines `<name>::observability_name::<name>` + `<name>::default_priority::endpoints`** at minimum; per-endpoint numeric-counter lines (`<name>::<endpoint>::success_count::<n>`, etc.) emit if reachable from `ClusterHandle`, else are simply absent (the harness `BodyRule::TextLines` `allowlist_envoy_only_lines` covers the per-endpoint counter mismatch). | SPEC §3 D7. Pragmatic: 08.1 ships only the minimum lines the harness asserts on; per-endpoint counters land later if a future phase needs them. |
| 11 | `/listeners` plain-text format | **One line per listener: `<listener_name>::<bind_address>:<bind_port>`, sorted by listener name (deterministic).** | SPEC §3 D8. |
| 12 | `BodyRule::JsonShape` / `BodyRule::TextLines` schema | **Per SPEC §3 D15 — see the exact field list at Task 10 below.** Both reuse the existing `tag = "kind"`-internally-tagged shape established at 06.3 for `BodyRule::PrometheusExposition`. | SPEC §3 D15. |
| 13 | Fixture 0014 driver | **`Driver::AdminScrape` (existing, 06.1-landed) — no new driver variant.** Four admin-scrape sub-cases (one per endpoint) on a single fixture. | SPEC §3 D17.1 — "Four admin-scrape sub-cases (one per endpoint), driven via the existing 06.1-landed `Driver::AdminScrape` (no new harness driver variant required — the existing variant suffices for GET-only endpoints)." |
| 14 | Fixture 0014 bootstrap shape | **Mirror fixture 0008's bootstrap: HCM + STRICT_DNS cluster + 1 listener.** Gives `/config_dump` + `/clusters` + `/listeners` non-trivial content to dump. | SPEC §3 D17.1. |
| 15 | Pre-state-4 fmt discipline | **Per-task PROGRESS sections run `cargo fmt --all -- --check` at every task close, NOT just at state-4.** The 5 stable-toolchain gates (`build` / `clippy` / `fmt` / `test` / `deny`) are quoted in every per-task PROGRESS entry. | SPEC §6.6 + 07.1/07.2 REVIEW doctrine. |
| 16 | State-4 evidence-discipline | **CI run URL + HEAD SHA + completion timestamp + per-gate quoted evidence in PROGRESS Task 14.** Fixture 0014 + all 13 pre-existing fixtures (0001-0013) green simultaneously at the same CI run. | SPEC §6.7 + 05.3 REVIEW I3 → 06.x → 07.x closure chain. |
| 17 | Cargo.lock cadence | **No new top-level Cargo deps expected; `Cargo.lock` diff at the 08.1 reviewed range is minimal (workspace-internal path-dep registrations only).** Task 5 adds `envoy-accesslog` as a path-dep of `envoy-admin` (workspace-internal; not a new top-level dep). | SPEC §6.8 — 04.1 REVIEW M5/M9 cadence-ratification ADR carries forward unchanged. |
| 18 | No new ADRs | **Ledger head stays ADR-0032.** No foundations grants; `serde_json` is D-3.2-permitted. ADR-0033 stays reserved-available for execution-time landing if reality forces it (per SPEC §7 — recommended posture is no foundations grant). | SPEC §7. |
| 19 | PROGRESS.md cadence | **PROGRESS.md skeleton + Task 1 preamble land ALONGSIDE PLAN.md at state-2** (the 07.2 `c7dea4c` / 06.2 `dc00750` shape). | Project precedent. |
| 20 | `#![forbid(unsafe_code)]` | **No new crate roots in 08.1.** All edits modify existing crates whose `lib.rs`/`main.rs` already carry the attribute. | D-3.8 + 4.1 invariant 8. |

---

## LoC drift posture / split-gate evaluation (per BOOTSTRAP_PROMPT.md §6.1)

08.1 SPEC §6.1's projection: **~12-14 tasks / ~1080-1180 LoC**. This PLAN materializes **14 tasks / ~1450 LoC** projected:

| # | Task | Production LoC | Test LoC | Fixture/Doc LoC | Total |
|---|---|---|---|---|---|
| 1 | D1+D2: serialize_response dedupe + reason_for_status | 25 | 30 | — | 55 |
| 2 | D3: DRAIN_BUDGET hoist | 5 | 15 | — | 20 |
| 3 | D4: Dispatch enum + dispatch refactor | 80 | 50 | — | 130 |
| 4 | Bootstrap Serialize cascade + roundtrip sanity | 35 | 40 | — | 75 |
| 5 | D13a: AdminHandler::new widen + envoy-bin wiring + format_iso8601 wrapper | 70 | 40 | — | 110 |
| 6 | D6: `/config_dump` | 120 | 70 | — | 190 |
| 7 | D5: `/server_info` | 90 | 50 | — | 140 |
| 8 | ClusterManager::clusters() + D7: `/clusters` | 70 | 50 | — | 120 |
| 9 | D8: `/listeners` | 60 | 40 | — | 100 |
| 10 | D15: BodyRule::JsonShape + BodyRule::TextLines | 130 | 30 | — | 160 |
| 11 | D17.1: Fixture 0014 + Docker-gated wrapper | — | 100 | 130 | 230 |
| 12 | D17.3a: Fuzz corpus seed | — | — | 50 | 50 |
| 13 | D17.4a: In-process backstop | — | 120 | — | 120 |
| 14 | State-4 verification + STATE advance | — | — | 30 | 30 |
| | **Total** | **~685** | **~635** | **~210** | **~1530** |

Task count (14) sits at the SPEC's projected upper end and is well under the §6.1 ~25-task gate. LoC projection sits ~+3% over the SPEC's upper projection (1180) and ~+2% over the §6.1 ~1500-LoC soft gate — concentrated in test + fixture material; production code (~685 LoC) is comfortably under the ~700 LoC SPEC §6.1 implied production-side target.

**Decision: accept the drift; do NOT nest-split.** Per parent-08 SPEC §6.1 alternative (vi) ("Not recommended: nested splits"), the gate would have to fire materially (>15% over) before a nested split is considered; ~+2% is well under the threshold. The 07.2 / 06.x precedent ratifies the accept-drift posture for test-heavy projections (07.2 SPEC ~1500 → PLAN ~1600; 06.1 SPEC ~1300 → PLAN ~2010; 06.2 SPEC ~1300 → PLAN ~1875; none nest-split). If execution-time drift inflates a single task past ~10 sub-steps, the in-execution release valve is per-step commit splitting recorded in PROGRESS (e.g., Task 4a = Serialize cascade derive edits; Task 4b = roundtrip sanity) — NOT a phase-level nest-split.

---

## Task summary

14 substantive tasks; Tasks 1-13 land at state-3, each as its own commit; Task 14 is the state-4-reached / state-5-next STATE-advance commit. Recommended execution order **1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11 → 12 → 13 → 14**.

| # | Title | Scope (LoC) | Depends on | Carryforwards / notes |
|---|---|---|---|---|
| 1 | D1+D2: `serialize_response` header dedupe + `reason_for_status` helper | ~55 | — | **06.1 REVIEW I2** (D1) + **06.1 REVIEW M1** (D2) close at this task |
| 2 | D3: `DRAIN_BUDGET` module-level hoist | ~20 | 1 | **06.1 REVIEW M4** closes at this task |
| 3 | D4: `Dispatch` enum + `AdminEndpoint::dispatch` refactor | ~130 | 1, 2 | 06.1 REVIEW M1 closes structurally as a side effect (every endpoint declares its 405 allow-list surface) |
| 4 | `Bootstrap` Serialize derive cascade + roundtrip sanity check | ~75 | — (independent of 1-3) | Mechanical; prerequisite of Task 6 |
| 5 | D13a: `AdminHandler::new` widening + envoy-bin wiring + `format_iso8601` wrapper | ~110 | 1-3 | Prerequisite of Tasks 6, 7, 8, 9 |
| 6 | D6: `/config_dump` endpoint | ~190 | 4, 5 | — |
| 7 | D5: `/server_info` endpoint | ~140 | 5 (and Task 4 for `Node` Serialize) | — |
| 8 | `ClusterManager::clusters()` accessor + D7: `/clusters` endpoint | ~120 | 5 | — |
| 9 | D8: `/listeners` endpoint | ~100 | 5 (uses `Arc<Bootstrap>`) | — |
| 10 | D15: `BodyRule::JsonShape` + `BodyRule::TextLines` harness rules | ~160 | — | Prerequisite of Task 11 |
| 11 | D17.1: Fixture 0014 + Docker-gated wrapper | ~230 | 6-10 | Docker-gated bilateral run before commit |
| 12 | D17.3a: Fuzz corpus seed `admin_multi_endpoint_bootstrap.yaml` | ~50 | 4 | — |
| 13 | D17.4a: In-process backstop `admin_config_dump_server_info.rs` | ~120 | 6-9 | — |
| 14 | State-4 phase-done verification + STATE advance to state-5-next | ~30 doc | 1-13 | Materialize a real CI run; quote per-gate evidence in PROGRESS |

**Parallelization notes (for subagent-driven dispatch).** Recommended default is strict sequential 1→14 with two-stage review between tasks. Where the executor wants concurrency: **Task 4 is fully independent of Tasks 1-3** (touches `envoy-config` only; does not touch `envoy-admin`) — can dispatch concurrently with Task 1 if the worker pool is wide. **Task 10 is fully independent of Tasks 1-9** (touches `tests/differential` only) — can dispatch any time after Task 4 (the harness types reference `Bootstrap` only via the existing fixture-render pipeline; Task 10's new BodyRule variants are tree-orthogonal to admin endpoint changes). **Task 12 depends only on Task 4** (the fuzz seed exercises `Bootstrap` parsing; Task 4's roundtrip sanity check verifies the same surface). **Tasks 6, 7, 8, 9 are mutually parallelizable after Task 5 lands** (each adds one `AdminEndpoint` variant + match arm + render function; disjoint endpoint surfaces; all four touch `endpoint.rs` so merge-conflicts on the `match` arm need attention — the executor adds them in a deterministic order, sequential commits, or one merge-combiner commit). Tasks 11, 13, 14 are strictly sequential on their predecessors.

---

## File structure overview

### Created (new files)

- **`tests/fixtures/0014-admin-config-dump-server-info/envoy.yaml`** (Task 11) — reference Envoy config (HCM + STRICT_DNS cluster + 1 listener; mirrors fixture 0008's shape).
- **`tests/fixtures/0014-admin-config-dump-server-info/envoy-rust.yaml`** (Task 11) — envoy-rust config (paired).
- **`tests/fixtures/0014-admin-config-dump-server-info/inputs/payload.bin`** (Task 11) — 0-byte placeholder.
- **`tests/fixtures/0014-admin-config-dump-server-info/expectations.yaml`** (Task 11) — differential assertions; 4 admin-scrape sub-cases using `BodyRule::JsonShape` + `BodyRule::TextLines`.
- **`tests/fixtures/0014-admin-config-dump-server-info/README.md`** (Task 11) — fixture documentation.
- **`tests/differential/tests/admin_config_dump_server_info.rs`** (Task 11) — Docker-gated wrapper.
- **`crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_multi_endpoint_bootstrap.yaml`** (Task 12) — fuzz corpus seed.
- **`crates/envoy-bin/tests/admin_config_dump_server_info.rs`** (Task 13) — in-process backstop.

### Modified

- **`crates/envoy-admin/src/handler.rs`** (Tasks 1, 2, 3, 5) — Task 1: `serialize_response` case-insensitive dedupe + `reason_for_status` helper. Task 2: import `envoy_listener::DRAIN_BUDGET`; delete the module-level `const DRAIN_BUDGET` at line 28. Task 3: rewrite `handle_inner`'s dispatch path to use `AdminEndpoint::dispatch` instead of `from_path`; thread the `allow` value through to `render_405`. Task 5: widen `AdminHandler::new` to 5-arg; add new fields (`bootstrap`, `cluster_manager`, `start_instant`, `command_line_options`); update 7 in-file test call sites.
- **`crates/envoy-admin/src/endpoint.rs`** (Tasks 3, 6, 7, 8, 9) — Task 3: add `Dispatch` enum + `pub fn dispatch(method, path) -> Dispatch` + per-variant `ALLOWED: &'static str` constants; extend `render_405` signature to take `allow: &'static str`. Tasks 6-9: add `ConfigDump`, `ServerInfo`, `Clusters`, `Listeners` variants with their render functions.
- **`crates/envoy-admin/src/lib.rs`** (Task 5) — add re-exports if needed for new public types (e.g., `pub use endpoint::Dispatch;`).
- **`crates/envoy-admin/Cargo.toml`** (Tasks 5, 6) — Task 5: add `envoy-accesslog = { path = "../envoy-accesslog" }`. Task 6: add `serde_json = "1"` (or workspace dep). Task 6/7: add `envoy-cluster = { path = "../envoy-cluster" }` for `ClusterManager`/`ClusterHandle` types; add `serde = { version = "1", features = ["derive"] }` if not present; add `serde_yaml = "0.9"` for `serde_yaml::Value` in `command_line_options`.
- **`crates/envoy-listener/src/lib.rs`** (Task 2) — add module-level `pub const DRAIN_BUDGET: Duration = Duration::from_secs(5);`; delete the local-fn `const DRAIN_BUDGET` at line 165.
- **`crates/envoy-cluster/src/cluster.rs`** (Task 8) — add `pub fn clusters(&self) -> impl Iterator<Item = ClusterHandle> + '_` on `ClusterManager`; add 1 unit test.
- **`crates/envoy-config/src/bootstrap.rs`** (Task 4) — add `Serialize` derive to every `#[derive(Debug, Deserialize, ...)]` line transitively reachable from `Bootstrap` (~25-32 derive-line edits); 1 new roundtrip unit test in `#[cfg(test)] mod tests`.
- **`crates/envoy-accesslog/src/lib.rs`** (Task 5) — add `pub fn format_iso8601(t: SystemTime) -> String` wrapper at the lib root.
- **`crates/envoy-bin/src/main.rs`** (Task 5) — thread `Arc<Bootstrap>` + `Arc<ClusterManager>` + `Instant` + parsed CLI options into `AdminHandler::new`.
- **`crates/envoy-bin/src/admin.rs`** (Task 5) — adjust the `AdminHandler::new(...)` call site to the new 5-arg signature.
- **`tests/differential/src/lib.rs`** (Task 10) — add `BodyRule::JsonShape { ... }` + `BodyRule::TextLines { ... }` variants + their `tag = "kind"` internally-tagged shape + the `assert_*` rendering functions wired into the existing `BodyRule::assert_equivalent` (or similar) dispatch.
- **`crates/envoy-config/fuzz/.gitignore`** (Task 12) — add `!corpus/parse_bootstrap/admin_multi_endpoint_bootstrap.yaml`.
- **`crates/envoy-config/src/bootstrap.rs`** (Task 12, inside `#[cfg(test)] mod tests`) — append `"admin_multi_endpoint_bootstrap.yaml"` to the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array (mirrors 07.2 Task 6 + 06.1 Task 13 pattern).
- **`docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md`** (every task) — per-task narrative append. CREATED at state-2 with the Task 1 preamble.
- **`docs/envoy-rust/STATE.md`** (Task 14) — advance from `08.1 state 3` → `08.1 state-4-reached / state-5-next`.
- **`docs/envoy-rust/ROADMAP.md`** — flip row `08.1` `planned` → `in-progress` at THIS state-2 commit (not at a task commit; the `BOOTSTRAP_PROMPT.md` §4.1 invariant 3 trigger is "directory exists + STATE.md points at it" — both hold at state 2, but the standard project precedent — 07.2 `c7dea4c` — flips at the state-2 PLAN-write commit alongside PLAN.md).

### Deleted

None.

---

## Conventions

Mirrors the 07.2 / 06.3 / 06.2 / 06.1 PLAN conventions:

- **TDD shape per task:** Step 1 writes the failing test(s); Step 2 runs them (FAIL expected; quote output); Step 3 writes the minimal implementation; Step 4 runs the tests (PASS expected; quote output); later steps layer workspace-wide verification; final step appends the per-task PROGRESS section and commits.
- **Commit messages:** `phase 08.1: task N — <task summary>` (the exact subject line is in each task's final step). Co-Authored-By trailer: `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
- **PROGRESS.md per-task append:** every substantive task commit appends a per-task section narrating work summary, tests landed (names + LoC tally), per-task deviations from PLAN (D-3.5 append-only discipline), LoC delta, and the 5-gate test-bucket attestation. **The test-bucket attestation MUST explicitly quote `cargo deny check` output** (07.1-REVIEW doctrine reminder — do not write "assumed no-op").
- **No new top-level Cargo deps.** Task 5 adds `envoy-accesslog` as a workspace-internal path-dep of `envoy-admin` (not a new top-level dep); Task 6 may add `serde_json` to `envoy-admin/Cargo.toml` if not already transitively reachable (already on the D-3.2 permitted-foundations list — not a foundations grant). Every `Cargo.toml`-touching task quotes `cargo deny check`.
- **`cargo fmt --all` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean at every per-task commit.**
- **Error variants use the existing `AdminError` / `ConfigError` naming convention** — no transform. 08.1 introduces no new `AdminError` variants (no admin-side error surfaces engage; all 4 new endpoints render against in-process state synchronously without fallible I/O).

---

## State-2 commit (this commit's content; lands BEFORE any Task 1-14 commit)

The state-2 commit lands exactly 2 files created + 2 files modified — docs-only, no code:

- **CREATE:** `docs/envoy-rust/phases/08.1-admin-endpoint-surface/PLAN.md` (this file).
- **CREATE:** `docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md` — the PROGRESS skeleton with the Task 1 preamble (PLAN-write SPEC corrections + architecture-decision lock-ins). Per the `c7dea4c` (07.2) / `dc00750` (06.2) cadence.
- **MODIFY:** `docs/envoy-rust/ROADMAP.md` — flip row `08.1` `status: planned` → `status: in-progress` (single-cell edit; per BOOTSTRAP_PROMPT.md §4.1 invariant 3 — a phase enters `in-progress` only when STATE.md points at it AND its PLAN.md has landed). Parent row `08` stays `in-progress`; row `08.2` stays `planned`.
- **MODIFY:** `docs/envoy-rust/STATE.md` — advance active-phase status `08.1 state 2 (SPEC.md only)` → `08.1 state 3 (SPEC + PLAN exist; implementation incomplete)`; next-skill `superpowers:writing-plans` → `superpowers:subagent-driven-development` against this PLAN.md. Rewrite the Active-phase / Next-expected-skill / Last-commit / Last-updated sections + the standing context from PLAN-writer perspective to executor perspective. Preserve all "Phase-NN rollovers" sections verbatim (including the "Phase-08 state-1 brainstorm" + "Phase-08 state-2 split" subsections).
- **MODIFY (no edit):** `docs/envoy-rust/DECISIONS.md` — UNCHANGED. Ledger head stays **ADR-0032**. No ADR at the state-2 commit (recommended no-foundations-grants posture per SPEC §7).
- **MODIFY (no edit):** `BEHAVIOR_CONTRACT.md`, `ENVOY_TARGET.md`, `rust-toolchain.toml`, the 08.1 `SPEC.md` — UNCHANGED.

**Commit message (verbatim):**

```
phase 08.1: state-2 standalone PLAN.md

Lands the 08.1 PLAN.md + PROGRESS.md skeleton as a standalone
pre-Task-1 commit per the established standalone-PLAN cadence
(c7dea4c 07.2 / 3a964cc 06.3 / dc00750 06.2 / 505653d 06.1). 14 tasks
targeting the 08.1 SPEC §3 D1-D8 + D13a + D15 + D17.1 + D17.3a +
D17.4a deliverable set, ~1530 LoC projected (production ~685; tests
~635; fixture/doc ~210). Split-gate evaluation: 14 tasks at the SPEC's
projected upper end and well under the ~25-task gate; ~1530 LoC sits
~+2% over the ~1500-LoC soft gate, concentrated in test + fixture
material — accept the drift, do NOT nest-split (parent-08 SPEC §6.1
alternative (vi) "Not recommended: nested splits" + 07.2 / 06.x
accept-drift precedent).

PROGRESS.md skeleton lands alongside with the Task 1 preamble recording
6 PLAN-write SPEC corrections (DRAIN_BUDGET hoist target is module-level
pub const, not the existing local-fn-scope const; format_iso8601
visibility requires a pub wrapper at envoy-accesslog::lib + envoy-admin
adding envoy-accesslog as a dep; ClusterManager.clusters() accessor
does not exist, lands as Task 8 prerequisite; AdminHandler::new 2-arg
current shape confirmed for D13a widening; Bootstrap derive convention
confirmed as Debug+Deserialize-only, cascade adds Serialize mechanically
across ~25-32 derive lines; AdminEndpoint::from_path retained alongside
new dispatch method) + the 20 architecture-decision lock-ins per
feedback_pick_recommendation. The 06.1 REVIEW carryforwards close as
Task-1 preamble: I2 (case-insensitive dedupe) + M1 (reason_for_status
helper) at Task 1; M4 (DRAIN_BUDGET hoist) at Task 2; M1 structurally
again via D4 (Task 3) — every endpoint declares its 405-method-allowlist
surface. Every code-changing task's PROGRESS attestation must quote
cargo deny check output (07.1-REVIEW doctrine reminder).

STATE.md advances: active-phase status "08.1 state 2 (SPEC.md only)" to
"08.1 state 3 (SPEC + PLAN exist; implementation incomplete)";
next-skill "writing-plans" to "subagent-driven-development" against the
new PLAN.md per feedback_execution_style. Standing context rewritten
from PLAN-writer perspective to executor perspective; all "Phase-NN
rollovers" + "Phase-08 state-1 brainstorm" + "Phase-08 state-2 split"
subsections preserved verbatim. ROADMAP row 08.1 flips planned to
in-progress per BOOTSTRAP_PROMPT.md §4.1 invariant 3. Parent row 08
stays in-progress (closes at 08.2 state-6 per the closing-sub-phase
invariant); row 08.2 stays planned.

No code changes; docs-only commit per the standalone-PLAN.md cadence.
No ADR landed (DECISIONS.md ledger head remains ADR-0032; ADR-0033
stays reserved-available). §7.5 phase-done gate is NOT exercised at the
state-2 commit; verification lands at PLAN.md Task 14 (state-4).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

No `Differential surface` / `Conformance` lines (those belong to state-6 commits).

---

## Task 1: D1+D2 — `serialize_response` case-insensitive header dedupe + `reason_for_status` helper

**Scope:** ~25 LoC production + ~30 LoC tests = ~55 LoC. Closes **06.1 REVIEW I2** (case-insensitive dedupe; rationale per 06.1 REVIEW §6 I2: future endpoints may legitimately set their own `cache-control` / `content-type` etc.) AND **06.1 REVIEW M1** (`resp.reason.unwrap_or("OK")` → `reason_for_status(u16) -> &'static str` helper covering 200/400/404/405/500/503). Both edits sit at `crates/envoy-admin/src/handler.rs::serialize_response` (line 113); co-locating them avoids two edits to the same site.

**Files:**
- Modify: `crates/envoy-admin/src/handler.rs` — extend `serialize_response` (lines 113-137) with the dedupe logic; add `fn reason_for_status(u16) -> &'static str` as a free function or `impl` method; replace `resp.reason.unwrap_or("OK")` with the helper call.

- [ ] **Step 1: Write the failing tests**

Add this module inside `handler.rs`'s `#[cfg(test)] mod tests { ... }` block (place it after the existing tests, near the end of the file):

```rust
#[cfg(test)]
mod serialize_response_dedupe_and_reason_tests {
    use super::AdminHandler;
    use bytes::BytesMut;

    /// Helper: build a Response with the given (status, reason, headers, body) and
    /// invoke serialize_response.
    fn serialize(status: u16, reason: Option<&str>, headers: Vec<(String, String)>, body: Vec<u8>) -> String {
        let resp = envoy_http1::Response {
            status,
            reason: reason.map(|s| s.to_string()),
            headers,
            body,
        };
        let bytes: BytesMut = AdminHandler::serialize_response(&resp);
        String::from_utf8(bytes.to_vec()).expect("ASCII response")
    }

    #[test]
    fn dedupe_preserves_caller_provided_cache_control() {
        // Endpoint sets its own `Cache-Control: public, max-age=60`. The default
        // `cache-control: no-cache, max-age=0` must NOT be appended.
        let wire = serialize(
            200,
            None,
            vec![("Cache-Control".into(), "public, max-age=60".into())],
            b"OK".to_vec(),
        );
        // Count occurrences of "cache-control:" (case-insensitive).
        let lower = wire.to_lowercase();
        let count = lower.matches("cache-control:").count();
        assert_eq!(count, 1, "exactly one cache-control header; got wire:\n{wire}");
        assert!(wire.to_lowercase().contains("cache-control: public, max-age=60"));
    }

    #[test]
    fn dedupe_preserves_caller_provided_server() {
        let wire = serialize(
            200,
            None,
            vec![("server".into(), "custom-server".into())],
            b"OK".to_vec(),
        );
        let lower = wire.to_lowercase();
        let count = lower.matches("server:").count();
        assert_eq!(count, 1);
        assert!(lower.contains("server: custom-server"));
    }

    #[test]
    fn dedupe_is_case_insensitive() {
        // Caller sets "X-Content-Type-Options" with mixed case; default
        // "x-content-type-options: nosniff" must NOT be appended.
        let wire = serialize(
            200,
            None,
            vec![("X-Content-Type-Options".into(), "myvalue".into())],
            b"OK".to_vec(),
        );
        let lower = wire.to_lowercase();
        let count = lower.matches("x-content-type-options:").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn default_headers_present_when_caller_omits() {
        // No caller-supplied headers: all 4 admin standard headers should appear.
        let wire = serialize(200, None, vec![], b"OK".to_vec());
        let lower = wire.to_lowercase();
        assert!(lower.contains("cache-control: no-cache, max-age=0"));
        assert!(lower.contains("x-content-type-options: nosniff"));
        assert!(lower.contains("server: envoy-rust"));
        assert!(lower.contains("date: "));
    }

    #[test]
    fn reason_503_renders_service_unavailable_without_explicit_reason() {
        // resp.reason = None for a 503 must render "503 Service Unavailable",
        // not "503 OK" (the 06.1 M1 bug).
        let wire = serialize(503, None, vec![], b"".to_vec());
        let first_line = wire.lines().next().expect("status line");
        assert_eq!(first_line, "HTTP/1.1 503 Service Unavailable");
    }

    #[test]
    fn reason_for_status_covers_listed_codes() {
        let cases = [
            (200, "OK"),
            (400, "Bad Request"),
            (404, "Not Found"),
            (405, "Method Not Allowed"),
            (500, "Internal Server Error"),
            (503, "Service Unavailable"),
        ];
        for (code, expect) in cases {
            let wire = serialize(code, None, vec![], b"".to_vec());
            let first_line = wire.lines().next().unwrap();
            assert!(
                first_line.ends_with(expect),
                "{code} reason: got `{first_line}`, want suffix `{expect}`"
            );
        }
    }

    #[test]
    fn explicit_reason_overrides_helper() {
        // If resp.reason = Some("Custom"), that wins; the helper is a fallback only.
        let wire = serialize(200, Some("Custom"), vec![], b"".to_vec());
        let first_line = wire.lines().next().unwrap();
        assert_eq!(first_line, "HTTP/1.1 200 Custom");
    }
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-admin serialize_response_dedupe_and_reason_tests 2>&1 | tail -20`
Expected: FAIL — duplicate `cache-control` / `server` / `x-content-type-options` headers AND `503 OK` (not `503 Service Unavailable`).

- [ ] **Step 3: Implement the helper and the dedupe**

Edit `crates/envoy-admin/src/handler.rs::serialize_response` (currently lines 113-137). Add the helper near the top of the file (after the existing `use` block, before the `pub struct AdminHandler` definition):

```rust
/// Maps HTTP status codes to their RFC 7231 reason phrase. Used as the fallback
/// when `Response.reason` is `None`. Phase 08.1 D2 (closes 06.1 REVIEW M1).
fn reason_for_status(code: u16) -> &'static str {
    match code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK", // Conservative fallback for codes not yet exercised by admin endpoints.
    }
}
```

Then rewrite `serialize_response`'s header-emission block. The existing implementation (paraphrased) appends each of the 4 standard headers unconditionally; replace with case-insensitive dedupe via a helper closure that checks `resp.headers` before appending. The status line uses `reason_for_status(resp.status)` as the fallback:

```rust
fn serialize_response(resp: &envoy_http1::Response) -> BytesMut {
    use std::io::Write as _;
    let mut buf = BytesMut::new();
    let mut wire = Vec::<u8>::new();
    let reason = resp.reason.as_deref().unwrap_or_else(|| reason_for_status(resp.status));
    write!(&mut wire, "HTTP/1.1 {} {}\r\n", resp.status, reason).unwrap();

    // Caller-supplied headers first.
    for (k, v) in &resp.headers {
        write!(&mut wire, "{k}: {v}\r\n").unwrap();
    }

    // Default headers, appended only if absent (case-insensitive).
    // D1 closes 06.1 REVIEW I2.
    let has_header = |name: &str| -> bool {
        resp.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name))
    };

    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    let defaults: &[(&str, &str)] = &[
        ("cache-control", "no-cache, max-age=0"),
        ("x-content-type-options", "nosniff"),
        ("server", "envoy-rust"),
        ("date", &date),
    ];
    for (name, value) in defaults {
        if !has_header(name) {
            write!(&mut wire, "{name}: {value}\r\n").unwrap();
        }
    }

    write!(&mut wire, "content-length: {}\r\n\r\n", resp.body.len()).unwrap();
    buf.extend_from_slice(&wire);
    buf.extend_from_slice(&resp.body);
    buf
}
```

**Note for executor:** The existing implementation may not use `httpdate` (the 06.1-landed code hand-rolls the date format or uses a different helper). If `httpdate` is not already a dep of `envoy-admin`, the executor preserves the existing date-rendering mechanism (whatever it is at HEAD `56dee82`) and adapts the dedupe logic around it. The dedupe is the load-bearing change; date rendering is not in scope at this task.

- [ ] **Step 4: Run the tests — expect PASS**

Run: `cargo test -p envoy-admin serialize_response_dedupe_and_reason_tests 2>&1 | tail -20`
Expected: PASS — 7 tests.

- [ ] **Step 5: BEHAVIOR_CONTRACT.md header allow-list dedupe note**

Per SPEC §2.2, add a one-line note to the header allow-list section. Find the "Header allow-list" section in `docs/envoy-rust/BEHAVIOR_CONTRACT.md` and append after the existing 06.1 admin-standard-header rows:

```markdown
**Phase 08.1 D1 dedupe note:** With phase 08.1's case-insensitive dedupe in
`crates/envoy-admin/src/handler.rs::serialize_response`, a future endpoint may
legitimately set its own `cache-control` (or any of the other 3 standard
headers). The dedupe guarantees no duplicate header lands on the wire; only one
instance of the header name appears in the response, and the caller-supplied
value wins.
```

- [ ] **Step 6: Workspace-wide checks**

Run:
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -5`
- `cargo test --workspace 2>&1 | tail -10`
- `cargo deny check 2>&1 | tail -10`

Expected: all clean.

- [ ] **Step 7: Append the Task 1 PROGRESS section + commit**

Append a `## Task 1 — D1+D2: serialize_response dedupe + reason_for_status` section to PROGRESS.md (work summary, 7 tests landed, LoC delta, deviations, 5-gate test-bucket attestation incl. `cargo deny check`). Then:

```bash
git add crates/envoy-admin/src/handler.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 1 — serialize_response dedupe + reason_for_status (closes 06.1 I2, M1)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: D3 — `DRAIN_BUDGET` module-level hoist

**Scope:** ~5 LoC production + ~15 LoC tests = ~20 LoC. Closes **06.1 REVIEW M4** (DRAIN_BUDGET duplicated at two sites). Hoists the constant from its two current declarations (`crates/envoy-listener/src/lib.rs:165` local-fn const + `crates/envoy-admin/src/handler.rs:28` module const) into a single `pub const` at module level in `crates/envoy-listener/src/lib.rs`; `envoy-admin` imports it.

**Files:**
- Modify: `crates/envoy-listener/src/lib.rs` — add module-level `pub const DRAIN_BUDGET: Duration = Duration::from_secs(5);`; delete the local-fn-scope const at line 165.
- Modify: `crates/envoy-admin/src/handler.rs` — add `use envoy_listener::DRAIN_BUDGET;` (or qualified `envoy_listener::DRAIN_BUDGET` at the use sites at lines 235, 237); delete the module-level `const DRAIN_BUDGET` at line 28.

- [ ] **Step 1: Write the failing tests**

Add this module inside `crates/envoy-listener/src/lib.rs`'s `#[cfg(test)] mod tests { ... }` block (or wherever the listener tests live):

```rust
#[cfg(test)]
mod drain_budget_constant_tests {
    use std::time::Duration;

    #[test]
    fn drain_budget_is_pub_const_at_module_level() {
        // Compile-time tautology: if DRAIN_BUDGET is NOT a pub-const at module
        // level, this fails to compile.
        const _CHECK: Duration = crate::DRAIN_BUDGET;
        assert_eq!(crate::DRAIN_BUDGET, Duration::from_secs(5));
    }

    #[test]
    fn drain_budget_value_is_5_seconds() {
        assert_eq!(crate::DRAIN_BUDGET, Duration::from_secs(5));
    }
}
```

And add this module inside `crates/envoy-admin/src/handler.rs`'s `#[cfg(test)] mod tests { ... }` block:

```rust
#[cfg(test)]
mod drain_budget_lockstep_tests {
    use std::time::Duration;

    #[test]
    fn admin_uses_listener_drain_budget() {
        // Compile-time tautology: if envoy-admin does not import
        // envoy_listener::DRAIN_BUDGET, this fails to compile.
        assert_eq!(envoy_listener::DRAIN_BUDGET, Duration::from_secs(5));
    }
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-listener drain_budget_constant_tests 2>&1 | tail -10`
Expected: FAIL — `cannot find value DRAIN_BUDGET in crate envoy_listener` (the const is local-fn-scoped, not pub-module-level).

Run: `cargo test -p envoy-admin drain_budget_lockstep_tests 2>&1 | tail -10`
Expected: FAIL — same reason.

- [ ] **Step 3: Hoist in `envoy-listener`**

In `crates/envoy-listener/src/lib.rs`, after the existing `use` block + the file-level doc comments, before the first `pub struct`/`pub enum`/`pub fn`, add:

```rust
/// Drain budget — the maximum time `Listener::serve` waits for in-flight
/// connections to complete after the drain signal fires. Hoisted to module
/// level at phase 08.1 D3 (closes 06.1 REVIEW M4); re-exported from
/// `envoy-admin` via the existing crate dep.
pub const DRAIN_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);
```

Then delete the existing local-fn const at the original line 165 inside `Listener::serve`. The three downstream use sites in the same file (`tokio::time::timeout(DRAIN_BUDGET, drain)` at the original line 244; the `tracing::warn!(?DRAIN_BUDGET, ...)` at line 245; the `return Err(ListenerError::DrainTimeout(DRAIN_BUDGET))` at line 249) need no change — the identifier still resolves, just now to the module-level const instead of the local-fn const.

- [ ] **Step 4: Import in `envoy-admin`**

In `crates/envoy-admin/src/handler.rs`, change the import line:

```rust
// before:
// use envoy_listener::{BoxFuture, ConnectionHandler};
// after:
use envoy_listener::{BoxFuture, ConnectionHandler, DRAIN_BUDGET};
```

Delete the module-level `const DRAIN_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);` at line 28.

The two downstream use sites at lines 235 + 237 (`tokio::time::timeout(DRAIN_BUDGET, drain)` + `tracing::warn!(?DRAIN_BUDGET, ...)`) need no change — the identifier still resolves, now to the imported one.

- [ ] **Step 5: Run the tests — expect PASS**

Run: `cargo test -p envoy-listener drain_budget_constant_tests 2>&1 | tail -10`
Expected: PASS — 2 tests.

Run: `cargo test -p envoy-admin drain_budget_lockstep_tests 2>&1 | tail -10`
Expected: PASS — 1 test.

- [ ] **Step 6: Workspace-wide checks**

Same 5-gate set as Task 1.

Expected: all clean.

- [ ] **Step 7: Append the Task 2 PROGRESS section + commit**

```bash
git add crates/envoy-listener/src/lib.rs crates/envoy-admin/src/handler.rs docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 2 — DRAIN_BUDGET module-level hoist (closes 06.1 M4)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: D4 — `Dispatch` enum + `AdminEndpoint::dispatch` refactor

**Scope:** ~80 LoC production + ~50 LoC tests = ~130 LoC. Introduces the `Dispatch` enum that 08.2's POST endpoints plug into; widens path-only dispatch to method+path dispatch. Closes 06.1 REVIEW M1 *structurally* (every endpoint variant declares its 405 allow-list surface). Per SPEC §3 D4 + §5.2.

**Files:**
- Modify: `crates/envoy-admin/src/endpoint.rs` — add `Dispatch` enum; add `AdminEndpoint::dispatch(method, path) -> Dispatch`; add `const ALLOWED: &'static str = "GET";` per variant; extend `render_405(allow: &'static str)` signature.
- Modify: `crates/envoy-admin/src/handler.rs::handle_inner` — migrate from `AdminEndpoint::from_path(&path)` to `AdminEndpoint::dispatch(&method, &path)`; consume `Dispatch::Endpoint(e)` / `Dispatch::NotFound` / `Dispatch::MethodNotAllowed { allow }` arms; pass `allow` through to `render_405`.

- [ ] **Step 1: Write the failing tests**

Add this module inside `crates/envoy-admin/src/endpoint.rs`'s `#[cfg(test)] mod tests { ... }` block:

```rust
#[cfg(test)]
mod dispatch_tests {
    use super::{AdminEndpoint, Dispatch};

    #[test]
    fn get_known_path_returns_endpoint() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/ready"),
            Dispatch::Endpoint(AdminEndpoint::Ready)
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/stats"),
            Dispatch::Endpoint(AdminEndpoint::Stats)
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/stats/prometheus"),
            Dispatch::Endpoint(AdminEndpoint::StatsPrometheus)
        ));
    }

    #[test]
    fn unknown_path_returns_not_found_regardless_of_method() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/nope"),
            Dispatch::NotFound
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/nope"),
            Dispatch::NotFound
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("DELETE", "/"),
            Dispatch::NotFound
        ));
    }

    #[test]
    fn known_path_wrong_method_returns_method_not_allowed_with_get_in_allow() {
        match AdminEndpoint::dispatch("POST", "/ready") {
            Dispatch::MethodNotAllowed { allow } => assert_eq!(allow, "GET"),
            other => panic!("expected MethodNotAllowed; got {other:?}"),
        }
        match AdminEndpoint::dispatch("PUT", "/stats") {
            Dispatch::MethodNotAllowed { allow } => assert_eq!(allow, "GET"),
            other => panic!("expected MethodNotAllowed; got {other:?}"),
        }
        match AdminEndpoint::dispatch("DELETE", "/stats/prometheus") {
            Dispatch::MethodNotAllowed { allow } => assert_eq!(allow, "GET"),
            other => panic!("expected MethodNotAllowed; got {other:?}"),
        }
    }

    #[test]
    fn method_match_is_case_sensitive_exact() {
        // Envoy's admin API treats HTTP method names case-sensitively (uppercase
        // canonical per RFC 7230). Mixed-case methods are NOT recognized.
        assert!(matches!(
            AdminEndpoint::dispatch("get", "/ready"),
            Dispatch::MethodNotAllowed { .. }
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("Get", "/ready"),
            Dispatch::MethodNotAllowed { .. }
        ));
    }

    #[test]
    fn each_endpoint_declares_its_allowed_method() {
        // Compile-time tautology: if any variant fails to declare ALLOWED, this
        // fails to compile.
        assert_eq!(AdminEndpoint::Ready.allowed_method(), "GET");
        assert_eq!(AdminEndpoint::Stats.allowed_method(), "GET");
        assert_eq!(AdminEndpoint::StatsPrometheus.allowed_method(), "GET");
    }

    #[test]
    fn dispatch_is_disjoint_from_from_path() {
        // from_path is retained as a thin convenience but does NOT route through
        // dispatch. Direct unit test that both surfaces remain available.
        assert!(AdminEndpoint::from_path("/ready").is_some());
        assert!(AdminEndpoint::from_path("/nope").is_none());
    }
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-admin dispatch_tests 2>&1 | tail -20`
Expected: FAIL — `cannot find type Dispatch` / `cannot find method allowed_method` / `cannot find method dispatch` (the surface does not exist yet).

- [ ] **Step 3: Add the `Dispatch` enum and the `dispatch` method**

In `crates/envoy-admin/src/endpoint.rs`, after the existing `pub enum AdminEndpoint` definition (currently lines 8-22), add:

```rust
/// Method-aware dispatch result. Introduced at phase 08.1 D4 to give every
/// endpoint a structurally-declared 405-method-allowlist surface (closes 06.1
/// REVIEW M1 structurally). 08.2 POST endpoints plug in additively via new
/// `AdminEndpoint` variants with `ALLOWED = "POST"`; no further refactor of
/// `Dispatch` is needed.
#[derive(Debug, PartialEq, Eq)]
pub enum Dispatch {
    Endpoint(AdminEndpoint),
    NotFound,
    MethodNotAllowed { allow: &'static str },
}
```

Then extend the `impl AdminEndpoint` block (currently starting around line 24). Keep the existing `from_path` method; add a new `allowed_method` accessor + the new `dispatch` method:

```rust
impl AdminEndpoint {
    /// The HTTP method this endpoint accepts. 08.1's 4 new GET endpoints
    /// (ConfigDump, ServerInfo, Clusters, Listeners) declare `"GET"` here;
    /// 08.2's POST endpoints will declare `"POST"`.
    pub fn allowed_method(&self) -> &'static str {
        match self {
            AdminEndpoint::Ready => "GET",
            AdminEndpoint::Stats => "GET",
            AdminEndpoint::StatsPrometheus => "GET",
            // Tasks 6-9 add: ConfigDump | ServerInfo | Clusters | Listeners => "GET",
        }
    }

    /// Method-aware dispatch. Returns:
    /// - `Endpoint(e)` on a method+path match,
    /// - `NotFound` on an unknown path (regardless of method),
    /// - `MethodNotAllowed { allow }` on a known path with the wrong method.
    pub fn dispatch(method: &str, path: &str) -> Dispatch {
        match AdminEndpoint::from_path(path) {
            None => Dispatch::NotFound,
            Some(endpoint) => {
                let allow = endpoint.allowed_method();
                if method == allow {
                    Dispatch::Endpoint(endpoint)
                } else {
                    Dispatch::MethodNotAllowed { allow }
                }
            }
        }
    }

    // ... existing methods (from_path, render, render_ready, etc.) carry forward unchanged ...
}
```

- [ ] **Step 4: Extend `render_405` to take the `allow` value**

The existing `render_405()` (sibling free function in `endpoint.rs`) returns a 405 response with a fixed `Allow:` header value (presumably `"GET"` or empty). Widen the signature:

```rust
pub(crate) fn render_405(allow: &'static str) -> envoy_http1::Response {
    envoy_http1::Response {
        status: 405,
        reason: None, // Task 1's reason_for_status renders "Method Not Allowed".
        headers: vec![
            ("allow".to_string(), allow.to_string()),
            ("content-type".to_string(), "text/plain; charset=utf-8".to_string()),
        ],
        body: format!("Method not allowed. Allow: {allow}\n").into_bytes(),
    }
}
```

(The executor adapts the body shape if the 06.1-landed `render_405` emits a different body literal; the key change is the dynamic `Allow:` header value.)

- [ ] **Step 5: Migrate `handle_inner` to `dispatch`**

In `crates/envoy-admin/src/handler.rs::handle_inner` (currently around line 139), find the existing dispatch path. The existing code looks roughly like:

```rust
// Pseudo-code of the existing 06.1-landed shape:
match AdminEndpoint::from_path(&path) {
    Some(endpoint) if method == "GET" => endpoint.render(&self.registry),
    Some(_) => render_405(), // hand-rolled GET-only 405
    None => render_404(),
}
```

Replace with:

```rust
match AdminEndpoint::dispatch(&method, &path) {
    Dispatch::Endpoint(endpoint) => endpoint.render(&self.registry),
    Dispatch::MethodNotAllowed { allow } => render_405(allow),
    Dispatch::NotFound => render_404(),
}
```

The `Dispatch` enum + `render_405` need to be imported. Update the existing `use crate::endpoint::{AdminEndpoint, render_404, render_405};` line:

```rust
use crate::endpoint::{AdminEndpoint, Dispatch, render_404, render_405};
```

- [ ] **Step 6: Run the tests — expect PASS**

Run: `cargo test -p envoy-admin dispatch_tests 2>&1 | tail -20`
Expected: PASS — 6 tests.

Run also: `cargo test -p envoy-admin 2>&1 | tail -10` — confirms the existing in-file tests (handler_returns_ready_response, etc.) still pass after the dispatch migration.

- [ ] **Step 7: Workspace-wide checks**

Same 5-gate set as Task 1.

Expected: all clean.

- [ ] **Step 8: Append the Task 3 PROGRESS section + commit**

```bash
git add crates/envoy-admin/src/endpoint.rs crates/envoy-admin/src/handler.rs docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 3 — Dispatch enum + AdminEndpoint::dispatch refactor

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `Bootstrap` Serialize derive cascade + roundtrip sanity check

**Scope:** ~35 LoC production (mechanical derive edits across ~25-32 lines) + ~40 LoC tests = ~75 LoC. Prerequisite of Task 6 (D6 `/config_dump`). Per SPEC §3 D6 + §5.3 + §6.4. Includes the pre-D6 sanity check (YAML→struct→JSON→struct equality) on fixture 0008's `envoy-rust.yaml`.

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — add `Serialize` to every `#[derive(Debug, Deserialize, ...)]` line transitively reachable from `Bootstrap` (~25-32 derive-line edits); add roundtrip unit test in `#[cfg(test)] mod tests`.

- [ ] **Step 1: Inventory every derive line reachable from `Bootstrap`**

Run `grep -n "^#\[derive" crates/envoy-config/src/bootstrap.rs` and list every line that mentions `Deserialize` (~25-32 lines per the PLAN-write scan). The executor copies the list into the PROGRESS Task 4 section as evidence of the inventory.

Pattern: `#[derive(Debug, Deserialize)]` → `#[derive(Debug, Serialize, Deserialize)]`. Same for derive lists that include `Default`, `PartialEq`, `Eq`, `Clone`, etc.: insert `Serialize` immediately before `Deserialize` to keep the cosmetic ordering stable.

**Caveat (per SPEC §5.3):** field-level `#[serde(rename = "...")]` and enum-level `#[serde(rename_all = "...")]` apply to BOTH sides per serde's documented contract. `#[serde(default)]` applies to the Deserialize side only — Serialize emits the literal value (default or not). No `#[serde(skip_serializing)]` / `#[serde(skip_serializing_if = ...)]` needed at 08.1 (per architecture-decision lock-in #8: the JSON output mirrors the YAML input verbatim modulo serde's renaming canonicalization).

- [ ] **Step 2: Write the failing roundtrip test**

Add to `crates/envoy-config/src/bootstrap.rs`'s `#[cfg(test)] mod tests { ... }` block:

```rust
#[cfg(test)]
mod serialize_roundtrip_tests {
    use crate::bootstrap::Bootstrap;

    /// Pre-D6 sanity check per 08.1 SPEC §6.4: take fixture 0008's
    /// `envoy-rust.yaml` (HCM + STRICT_DNS cluster + 1 listener + http_filters
    /// + multi-route — the most varied bootstrap shape in-tree at 08.1 time);
    /// parse via serde_yaml; serialize via serde_json; deserialize via
    /// serde_json; assert structural equality.
    #[test]
    fn fixture_0008_bootstrap_roundtrips_yaml_to_json_to_yaml() {
        let yaml = std::fs::read_to_string("../../tests/fixtures/0008-http1-router-upstream/envoy-rust.yaml")
            .expect("fixture 0008 envoy-rust.yaml readable from envoy-config crate dir");
        // YAML → struct
        let parsed: Bootstrap = serde_yaml::from_str(&yaml).expect("YAML parses");
        // struct → JSON
        let json = serde_json::to_string_pretty(&parsed).expect("Bootstrap serializes to JSON");
        // JSON → struct
        let reparsed: Bootstrap = serde_json::from_str(&json).expect("JSON round-trips back to Bootstrap");
        // structural equality (PartialEq derives on most subtypes; some types
        // like Bootstrap itself may need PartialEq added if not yet present —
        // the executor adds the derive if compilation requires it).
        // For 08.1, a coarse-grained check suffices: re-serialize and compare strings.
        let json2 = serde_json::to_string_pretty(&reparsed).expect("re-serializes");
        assert_eq!(json, json2, "JSON serialization is idempotent after roundtrip");
    }

    #[test]
    fn minimal_bootstrap_serializes_to_json() {
        let yaml = "node:\n  id: t\n  cluster: t\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let parsed: Bootstrap = serde_yaml::from_str(yaml).expect("minimal parses");
        let json = serde_json::to_string(&parsed).expect("minimal serializes");
        assert!(json.contains("\"node\""));
        assert!(json.contains("\"static_resources\""));
    }
}
```

- [ ] **Step 3: Run the failing tests**

Run: `cargo test -p envoy-config serialize_roundtrip_tests 2>&1 | tail -20`
Expected: FAIL — `Bootstrap` does not implement `Serialize`; many sub-types also lack `Serialize`.

- [ ] **Step 4: Apply the mechanical Serialize cascade**

For every `#[derive(...)]` line in `crates/envoy-config/src/bootstrap.rs` that mentions `Deserialize`, insert `Serialize` immediately before. Sample transformations (the inventory in Step 1 enumerates every line):

```rust
// before:
#[derive(Debug, Deserialize)]
pub struct Bootstrap { ... }
// after:
#[derive(Debug, Serialize, Deserialize)]
pub struct Bootstrap { ... }

// before:
#[derive(Debug, Deserialize, PartialEq)]
pub struct Cluster { ... }
// after:
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Cluster { ... }

// before:
#[derive(Debug, Default, Deserialize, PartialEq)]
pub struct EndpointConfig { ... }
// after:
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct EndpointConfig { ... }
```

Add `use serde::Serialize;` (or `use serde::{Serialize, Deserialize};` if the existing import is just `use serde::Deserialize;`) at the top of `bootstrap.rs`.

- [ ] **Step 5: Run the tests — expect PASS**

Run: `cargo test -p envoy-config serialize_roundtrip_tests 2>&1 | tail -20`
Expected: PASS — 2 tests.

If any subtype is missing `Serialize` (compiler error: `the trait bound: ...: Serialize is not satisfied`), the executor adds the derive to that type and re-runs. Iterate until clean.

- [ ] **Step 6: Workspace-wide checks**

Same 5-gate set.

Expected: all clean. Note: `cargo test --workspace` re-runs all crate tests — confirms the cascade did not break any existing test (e.g., a test that depended on a type NOT implementing `Serialize` would surface here; this is extremely unlikely but worth the explicit check).

- [ ] **Step 7: Append the Task 4 PROGRESS section + commit**

The PROGRESS section MUST include the full inventory of derive lines touched (line-numbers + before/after derive list) as evidence — this is the auditable record of the mechanical cascade.

```bash
git add crates/envoy-config/src/bootstrap.rs docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 4 — Bootstrap Serialize derive cascade + roundtrip sanity

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: D13a — `AdminHandler::new` widening + envoy-bin wiring + `format_iso8601` wrapper

**Scope:** ~70 LoC production + ~40 LoC tests = ~110 LoC. Per SPEC §3 D13a + PLAN-write correction 2 + 4. Widens `AdminHandler::new` from 2-arg to 5-arg; adds `envoy-accesslog` dep to `envoy-admin`; adds `pub fn format_iso8601(t: SystemTime) -> String` wrapper at `envoy-accesslog::lib.rs`; threads the new handles through `envoy-bin::main`. **Prerequisite of Tasks 6, 7, 8, 9.**

**Files:**
- Modify: `crates/envoy-admin/Cargo.toml` — add `envoy-accesslog = { path = "../envoy-accesslog" }`, `envoy-cluster = { path = "../envoy-cluster" }` (for `ClusterManager` import).
- Modify: `crates/envoy-admin/src/handler.rs` — widen `AdminHandler::new` to 5-arg; add 4 new fields (`bootstrap: Arc<Bootstrap>`, `cluster_manager: Arc<ClusterManager>`, `start_instant: Instant`, `command_line_options: BTreeMap<String, serde_yaml::Value>`); update 7 in-file test call sites at lines 291, 318, 341, 363, 386, 416, 461.
- Modify: `crates/envoy-accesslog/src/lib.rs` — add `pub fn format_iso8601(t: SystemTime) -> String` wrapper.
- Modify: `crates/envoy-bin/src/main.rs` — construct `Arc<Bootstrap>` from the parsed config; construct `Arc<ClusterManager>` (already built; just clone); capture `Instant::now()` at startup; build `command_line_options: BTreeMap<String, serde_yaml::Value>` (currently `{ "config_path": Value::String(<-c value>) }`); pass all four to `AdminHandler::new(...)`.
- Modify: `crates/envoy-bin/src/admin.rs` — adjust the call site to the new 5-arg signature (forward the new handles from `main`).

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-accesslog/src/lib.rs`'s `#[cfg(test)] mod tests { ... }` block (or create one if absent):

```rust
#[cfg(test)]
mod public_format_iso8601_tests {
    use crate::format_iso8601;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn public_wrapper_emits_iso8601_at_epoch() {
        let s = format_iso8601(UNIX_EPOCH);
        assert_eq!(s, "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn public_wrapper_emits_iso8601_at_known_date() {
        // 2021-07-15 12:34:56.789 UTC = epoch 1_626_352_496.789
        let t = UNIX_EPOCH + Duration::from_millis(1_626_352_496_789);
        let s = format_iso8601(t);
        assert_eq!(s, "2021-07-15T12:34:56.789Z");
    }
}
```

Add to `crates/envoy-admin/src/handler.rs`'s `#[cfg(test)] mod tests { ... }` block:

```rust
#[cfg(test)]
mod admin_handler_new_5arg_tests {
    use super::AdminHandler;
    use envoy_cluster::ClusterManager;
    use envoy_config::Bootstrap;
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Instant;

    fn dummy_bootstrap() -> Bootstrap {
        let yaml = "node:\n  id: t\n  cluster: t\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        serde_yaml::from_str(yaml).expect("dummy bootstrap parses")
    }

    #[test]
    fn new_accepts_five_arguments() {
        let cfg = Arc::new(admin_config(0));
        let registry = Arc::new(StatsRegistry::new());
        let bootstrap = Arc::new(dummy_bootstrap());
        let cluster_manager = Arc::new(ClusterManager::empty());
        let start_instant = Instant::now();
        let cli_options = BTreeMap::new();
        let _ = AdminHandler::new(cfg, registry, bootstrap, cluster_manager, start_instant, cli_options);
    }

    // NB: the existing `admin_config` helper at handler.rs's existing test
    // module is reused.
}
```

(Adjust the helper-call shape to match the existing `admin_config` fixture pattern in handler.rs.)

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-accesslog public_format_iso8601_tests 2>&1 | tail -10`
Expected: FAIL — `cannot find function format_iso8601 in crate envoy_accesslog`.

Run: `cargo test -p envoy-admin admin_handler_new_5arg_tests 2>&1 | tail -10`
Expected: FAIL — type errors / arity mismatch.

- [ ] **Step 3: Add `pub fn format_iso8601` wrapper at `envoy-accesslog::lib.rs`**

```rust
// In crates/envoy-accesslog/src/lib.rs, add near the bottom (after existing
// pub use lines):

use std::time::SystemTime;

/// Public wrapper around the internal `default_format::format_iso8601`
/// `&mut String`-writer. Returns a freshly-allocated `String` in the canonical
/// 24-byte `YYYY-MM-DDTHH:MM:SS.sssZ` shape. Phase 08.1 D6 consumes this for
/// `/config_dump`'s `last_updated` field; the original `pub(crate)` writer
/// remains internal-only (no visibility change to default_format).
pub fn format_iso8601(t: SystemTime) -> String {
    let mut s = String::new();
    default_format::format_iso8601(&mut s, t);
    s
}
```

- [ ] **Step 4: Add `envoy-accesslog` + `envoy-cluster` deps to `envoy-admin`**

In `crates/envoy-admin/Cargo.toml`, add to `[dependencies]`:

```toml
envoy-accesslog = { path = "../envoy-accesslog" }
envoy-cluster = { path = "../envoy-cluster" }
serde_yaml = "0.9"      # for command_line_options
```

(`serde_yaml` is already on the D-3.2 permitted-foundations list. Verify the version pin matches the workspace's existing `serde_yaml` version at other crates; reuse the same.)

- [ ] **Step 5: Widen `AdminHandler::new`**

In `crates/envoy-admin/src/handler.rs`, extend the struct + the constructor:

```rust
// Add imports near the top of the file:
use envoy_cluster::ClusterManager;
use envoy_config::Bootstrap;
use std::collections::BTreeMap;
use std::time::Instant;

// Extend the struct fields:
pub struct AdminHandler {
    config: Arc<AdminConfig>,
    registry: Arc<StatsRegistry>,
    bootstrap: Arc<Bootstrap>,
    cluster_manager: Arc<ClusterManager>,
    start_instant: Instant,
    command_line_options: BTreeMap<String, serde_yaml::Value>,
}

impl AdminHandler {
    pub fn new(
        config: Arc<AdminConfig>,
        registry: Arc<StatsRegistry>,
        bootstrap: Arc<Bootstrap>,
        cluster_manager: Arc<ClusterManager>,
        start_instant: Instant,
        command_line_options: BTreeMap<String, serde_yaml::Value>,
    ) -> Self {
        Self {
            config,
            registry,
            bootstrap,
            cluster_manager,
            start_instant,
            command_line_options,
        }
    }
    // ... existing methods carry forward unchanged ...
}
```

**Note:** the SPEC §3 D13a + architecture-decision lock-in #7 specifies a 5-arg signature in the SPEC text; including `command_line_options` makes it 6-arg in this PLAN. The reason: per architecture-decision lock-in #7 — `command_line_options` is built once at construction time and threaded as a field, not at render time. The 08.2 D13b extension adds `Arc<DrainState>` as the 7th parameter; the SPEC's "5-arg → 6-arg" framing was imprecise about `command_line_options`. **Recording this as Task-5-time deviation #1.** If the executor prefers to build `command_line_options` lazily at first render, the constructor stays 5-arg and the new field is constructed inside `new` from the parsed CLI passed via a different mechanism — but that mechanism does not exist at HEAD `56dee82`, so threading via the constructor is the simpler path.

- [ ] **Step 6: Update the 7 in-file test call sites + the production call sites**

Each existing `AdminHandler::new(cfg, registry)` call (lines 291, 318, 341, 363, 386, 416, 461 in handler.rs's `#[cfg(test)] mod tests`) extends to the 6-arg form. Build a per-test helper to keep the call-site noise low:

```rust
// Add near the top of handler.rs's #[cfg(test)] mod tests:
fn dummy_bootstrap() -> Arc<Bootstrap> {
    let yaml = "node:\n  id: t\n  cluster: t\nstatic_resources:\n  listeners: []\n  clusters: []\n";
    Arc::new(serde_yaml::from_str::<Bootstrap>(yaml).unwrap())
}
fn dummy_cluster_manager() -> Arc<ClusterManager> {
    Arc::new(ClusterManager::empty())
}

// And replace each call site like:
let handler = Arc::new(AdminHandler::new(
    cfg,
    registry,
    dummy_bootstrap(),
    dummy_cluster_manager(),
    Instant::now(),
    BTreeMap::new(),
));
```

In `crates/envoy-bin/src/main.rs`, find the existing `AdminHandler::new(...)` construction (likely in `admin.rs`'s `serve_admin` setup); thread the new handles:

```rust
// Pseudo-code; the executor adapts to the exact site:
let bootstrap = Arc::new(parsed_bootstrap.clone());
let cluster_manager = Arc::new(/* the existing ClusterManager built earlier in main */);
let start_instant = Instant::now();
let mut cli_options = BTreeMap::new();
cli_options.insert("config_path".to_string(), serde_yaml::Value::String(args.config.clone()));
// Then pass to AdminHandler::new(...).
```

- [ ] **Step 7: Run the tests — expect PASS**

Run: `cargo test -p envoy-accesslog public_format_iso8601_tests 2>&1 | tail -10` — PASS (2 tests).
Run: `cargo test -p envoy-admin admin_handler_new_5arg_tests 2>&1 | tail -10` — PASS (1 test).
Run: `cargo test -p envoy-admin 2>&1 | tail -10` — confirm existing in-file tests at handler.rs's test module still PASS after the constructor-arity change.
Run: `cargo build --workspace --all-targets 2>&1 | tail -10` — confirm envoy-bin builds with the new wiring.

- [ ] **Step 8: Workspace-wide checks**

Same 5-gate set. `cargo deny check` MUST be quoted (Cargo.toml dep additions).

Expected: all clean.

- [ ] **Step 9: Append the Task 5 PROGRESS section + commit**

```bash
git add crates/envoy-accesslog/src/lib.rs crates/envoy-admin/Cargo.toml crates/envoy-admin/src/handler.rs crates/envoy-bin/src/main.rs crates/envoy-bin/src/admin.rs docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 5 — AdminHandler::new widen + envoy-bin wiring + format_iso8601 pub wrapper

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: D6 — `/config_dump` endpoint

**Scope:** ~120 LoC production + ~70 LoC tests = ~190 LoC. Adds `AdminEndpoint::ConfigDump` variant + render function. Renders JSON via `serde_json::to_vec_pretty` on a `ConfigDumpBody` struct.

**Files:**
- Modify: `crates/envoy-admin/src/endpoint.rs` — add `ConfigDump` variant to `AdminEndpoint`; add `from_path` arm; add `allowed_method` arm; add `dispatch` test cases; add `ConfigDumpBody` + `ConfigDumpEntry` structs; add `fn render_config_dump(handler: &AdminHandler) -> envoy_http1::Response`.
- Modify: `crates/envoy-admin/src/handler.rs` — render dispatch passes `&self` (the handler) to the new render function (the existing dispatch passed only `&self.registry`; we need to widen the call to pass the full handler — or pass the new fields explicitly).
- Modify: `crates/envoy-admin/Cargo.toml` — add `serde_json = "1"` if not yet present (likely needed; verify).

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-admin/src/endpoint.rs`'s `#[cfg(test)] mod tests { ... }` block:

```rust
#[cfg(test)]
mod config_dump_tests {
    use super::{AdminEndpoint, Dispatch};
    use crate::handler::AdminHandler;
    use envoy_cluster::ClusterManager;
    use envoy_config::Bootstrap;
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Instant;

    fn handler_with_bootstrap(yaml: &str) -> AdminHandler {
        let bootstrap: Bootstrap = serde_yaml::from_str(yaml).expect("yaml parses");
        AdminHandler::new(
            Arc::new(crate::config::AdminConfig::from_envoy_config(
                &envoy_config::Admin {
                    address: envoy_config::Address::SocketAddress(envoy_config::SocketAddress {
                        protocol: Default::default(),
                        address: "127.0.0.1".to_string(),
                        port_value: 0,
                    }),
                    ..Default::default()
                },
            ).expect("AdminConfig")),
            Arc::new(StatsRegistry::new()),
            Arc::new(bootstrap),
            Arc::new(ClusterManager::empty()),
            Instant::now(),
            BTreeMap::new(),
        )
    }

    #[test]
    fn config_dump_path_dispatches_on_get() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/config_dump"),
            Dispatch::Endpoint(AdminEndpoint::ConfigDump)
        ));
    }

    #[test]
    fn config_dump_405_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/config_dump"),
            Dispatch::MethodNotAllowed { allow: "GET" }
        ));
    }

    #[test]
    fn config_dump_renders_200_with_application_json() {
        let yaml = "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::ConfigDump.render_with(&handler);
        assert_eq!(resp.status, 200);
        let ct = resp.headers.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str());
        assert_eq!(ct, Some("application/json"));
    }

    #[test]
    fn config_dump_body_is_valid_json_with_configs_array() {
        let yaml = "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::ConfigDump.render_with(&handler);
        let body_str = std::str::from_utf8(&resp.body).expect("utf-8");
        let value: serde_json::Value = serde_json::from_str(body_str).expect("valid JSON");
        assert!(value.get("configs").and_then(|c| c.as_array()).is_some());
    }

    #[test]
    fn config_dump_body_has_bootstrap_config_dump_entry() {
        let yaml = "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::ConfigDump.render_with(&handler);
        let body_str = std::str::from_utf8(&resp.body).unwrap();
        let value: serde_json::Value = serde_json::from_str(body_str).unwrap();
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        assert_eq!(configs.len(), 1);
        let entry = &configs[0];
        assert_eq!(
            entry.get("@type").and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.admin.v3.BootstrapConfigDump")
        );
        assert!(entry.get("bootstrap").is_some());
        assert!(entry.get("last_updated").and_then(|v| v.as_str()).is_some());
    }

    #[test]
    fn config_dump_bootstrap_subtree_carries_node_id() {
        let yaml = "node:\n  id: my-node-id\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::ConfigDump.render_with(&handler);
        let body_str = std::str::from_utf8(&resp.body).unwrap();
        let value: serde_json::Value = serde_json::from_str(body_str).unwrap();
        let node_id = value.pointer("/configs/0/bootstrap/node/id")
            .and_then(|v| v.as_str());
        assert_eq!(node_id, Some("my-node-id"));
    }
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-admin config_dump_tests 2>&1 | tail -20`
Expected: FAIL — `ConfigDump` variant doesn't exist; `render_with` doesn't exist; etc.

- [ ] **Step 3: Add the `ConfigDump` variant + `ConfigDumpBody` types**

In `crates/envoy-admin/src/endpoint.rs`, extend the `AdminEndpoint` enum:

```rust
pub enum AdminEndpoint {
    Ready,
    Stats,
    StatsPrometheus,
    ConfigDump,     // Phase 08.1 D6
    // ServerInfo, Clusters, Listeners added in later tasks
}
```

Extend the `from_path` match:

```rust
pub fn from_path(path: &str) -> Option<Self> {
    match path {
        "/ready" => Some(AdminEndpoint::Ready),
        "/stats" => Some(AdminEndpoint::Stats),
        "/stats/prometheus" => Some(AdminEndpoint::StatsPrometheus),
        "/config_dump" => Some(AdminEndpoint::ConfigDump),
        _ => None,
    }
}
```

Extend `allowed_method`:

```rust
pub fn allowed_method(&self) -> &'static str {
    match self {
        AdminEndpoint::Ready
        | AdminEndpoint::Stats
        | AdminEndpoint::StatsPrometheus
        | AdminEndpoint::ConfigDump => "GET",
    }
}
```

Add the body types + a `render_with` method that takes the full `&AdminHandler` (the SPEC §3 D6 + architecture-decision lock-in #9):

```rust
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct ConfigDumpBody {
    pub configs: Vec<ConfigDumpEntry>,
}

#[derive(Serialize)]
#[serde(tag = "@type")]
pub(crate) enum ConfigDumpEntry {
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump")]
    Bootstrap {
        bootstrap: envoy_config::Bootstrap,
        last_updated: String,
    },
    // xDS-derived entries deferred to xDS family.
}

impl AdminEndpoint {
    /// 08.1 D6 introduces `render_with(&AdminHandler)` to reach handler-scoped
    /// state (Arc<Bootstrap>, ClusterManager, start_instant, command_line_options).
    /// The existing `render(&StatsRegistry)` carries forward for `/ready`,
    /// `/stats`, `/stats/prometheus`; new endpoints use `render_with`.
    pub fn render_with(&self, handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
        match self {
            AdminEndpoint::ConfigDump => render_config_dump(handler),
            // Tasks 7-9 add ServerInfo, Clusters, Listeners arms.
            // Fall through to the registry-only render path for 06.1 endpoints:
            _ => self.render(handler.registry()),
        }
    }
}

pub(crate) fn render_config_dump(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    let body = ConfigDumpBody {
        configs: vec![ConfigDumpEntry::Bootstrap {
            bootstrap: (*handler.bootstrap()).clone(),
            last_updated: envoy_accesslog::format_iso8601(std::time::SystemTime::now()),
        }],
    };
    let body_bytes = serde_json::to_vec_pretty(&body)
        .expect("ConfigDumpBody serializes (all subtypes derive Serialize per Task 4)");
    envoy_http1::Response {
        status: 200,
        reason: None,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: body_bytes,
    }
}
```

**Important — `(*handler.bootstrap()).clone()`:** the executor must add `pub(crate) fn bootstrap(&self) -> &Arc<envoy_config::Bootstrap>` (and matching `cluster_manager()`, `start_instant()`, `command_line_options()`, `registry()`) accessors on `AdminHandler`, OR make the fields `pub(crate)`. The `.clone()` is on `Bootstrap` itself (requires `#[derive(Clone)]` on `Bootstrap` + the transitively-owned types — verify after Task 4 whether the cascade included `Clone`; if not, EITHER add `Clone` derives at Task 6 OR redesign `ConfigDumpEntry::Bootstrap` to take `&Bootstrap` and use `serde_json::to_vec_pretty(&body)` against borrowed references). **Recommended path: extend the `bootstrap` field type from `Arc<Bootstrap>` to keep cheap cloning, and serialize a borrowed reference inside `ConfigDumpEntry::Bootstrap` using a lifetime-parameterized body type:**

```rust
#[derive(Serialize)]
pub(crate) struct ConfigDumpBody<'a> {
    pub configs: Vec<ConfigDumpEntry<'a>>,
}

#[derive(Serialize)]
#[serde(tag = "@type")]
pub(crate) enum ConfigDumpEntry<'a> {
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump")]
    Bootstrap {
        bootstrap: &'a envoy_config::Bootstrap,
        last_updated: String,
    },
}
```

This avoids needing `Clone` on `Bootstrap`. The executor picks the cleaner shape; both serialize identically. **Recording the borrowed-reference shape as the recommended path** (avoids the Clone cascade entirely).

- [ ] **Step 4: Wire the new dispatch path in `handler.rs::handle_inner`**

Change the dispatch arm to call `render_with(&self)`:

```rust
Dispatch::Endpoint(endpoint) => endpoint.render_with(&self),
```

(The 06.1 endpoints fall through to `self.render(self.registry())` inside `render_with`'s catch-all arm.)

- [ ] **Step 5: Add `serde_json` to `envoy-admin/Cargo.toml` if not present**

Verify whether `serde_json` is already a transitive or direct dep; if not, add:

```toml
serde_json = "1"
```

(Pin to the workspace's existing `serde_json` version; the harness already uses `serde_json`.)

- [ ] **Step 6: Run the tests — expect PASS**

Run: `cargo test -p envoy-admin config_dump_tests 2>&1 | tail -20`
Expected: PASS — 6 tests.

Run also: `cargo test -p envoy-admin 2>&1 | tail -10` — all existing tests stay green.

- [ ] **Step 7: Workspace-wide checks**

Same 5-gate set. `cargo deny check` MUST be quoted.

Expected: all clean.

- [ ] **Step 8: Append Task 6 PROGRESS + BEHAVIOR_CONTRACT row + commit**

Per SPEC §2.1, add the `/config_dump` row to `BEHAVIOR_CONTRACT.md`'s new "Admin endpoint body shapes" subsection. The subsection is created at Task 6 if it does not exist yet:

```markdown
## Admin endpoint body shapes

> **To be filled per-phase as needed.**
>
> Authored per phase 08.1 SPEC §2.1. One row per admin endpoint with the body
> kind + per-endpoint equivalence disposition.

| Endpoint | Method | Body kind | Equivalence disposition |
|---|---|---|---|
| `/config_dump` | GET | JSON object | Top-level shape `{ "configs": [...] }`. envoy-rust emits exactly one entry: `{ "@type": "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump", "bootstrap": <static-bootstrap-as-JSON>, "last_updated": <ISO-8601 timestamp> }`. Envoy may emit additional entries for xDS-derived configs; those land on `allowlist_envoy_only`. `bootstrap.static_resources` content value-exact-after-roundtrip (modulo serde renamings; the harness's `JsonShape::required_subtree` covers this). `last_updated` name-required-value-may-differ (wall-clock non-determinism). |
```

Then commit:

```bash
git add crates/envoy-admin/src/endpoint.rs crates/envoy-admin/src/handler.rs crates/envoy-admin/Cargo.toml docs/envoy-rust/BEHAVIOR_CONTRACT.md docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 6 — /config_dump endpoint + BEHAVIOR_CONTRACT row

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: D5 — `/server_info` endpoint

**Scope:** ~90 LoC production + ~50 LoC tests = ~140 LoC. Adds `AdminEndpoint::ServerInfo` variant + render function. Emits JSON via `serde_json::to_vec_pretty` on a `ServerInfoBody` struct with `state: "LIVE"` as a literal (per SPEC §5.4 — 08.2's D5e patches the value-binding source from the constant).

**Files:**
- Modify: `crates/envoy-admin/src/endpoint.rs` — add `ServerInfo` variant + `/server_info` `from_path` arm + `allowed_method` arm + `render_with` arm + `ServerInfoBody` struct + `fn render_server_info(handler: &AdminHandler) -> envoy_http1::Response`.

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-admin/src/endpoint.rs`'s `#[cfg(test)] mod tests { ... }` block (reuse the `handler_with_bootstrap` helper from Task 6):

```rust
#[cfg(test)]
mod server_info_tests {
    use super::{AdminEndpoint, Dispatch};
    use super::config_dump_tests::handler_with_bootstrap;  // reuse

    #[test]
    fn server_info_path_dispatches_on_get() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/server_info"),
            Dispatch::Endpoint(AdminEndpoint::ServerInfo)
        ));
    }

    #[test]
    fn server_info_405_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/server_info"),
            Dispatch::MethodNotAllowed { allow: "GET" }
        ));
    }

    #[test]
    fn server_info_renders_200_with_application_json() {
        let yaml = "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::ServerInfo.render_with(&handler);
        assert_eq!(resp.status, 200);
        let ct = resp.headers.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str());
        assert_eq!(ct, Some("application/json"));
    }

    #[test]
    fn server_info_body_has_required_keys() {
        let yaml = "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::ServerInfo.render_with(&handler);
        let body_str = std::str::from_utf8(&resp.body).unwrap();
        let value: serde_json::Value = serde_json::from_str(body_str).unwrap();
        let obj = value.as_object().expect("top-level object");
        for key in &[
            "version",
            "state",
            "hot_restart_version",
            "command_line_options",
            "node",
            "uptime_current_epoch_seconds",
            "uptime_all_epochs_seconds",
        ] {
            assert!(obj.contains_key(*key), "missing key: {key}");
        }
    }

    #[test]
    fn server_info_state_is_live_at_phase_08_1() {
        // SPEC §5.4: 08.1 emits the constant "LIVE". 08.2's D5e patches the
        // value-binding source from this constant to a DrainState-derived match.
        let yaml = "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::ServerInfo.render_with(&handler);
        let value: serde_json::Value = serde_json::from_str(std::str::from_utf8(&resp.body).unwrap()).unwrap();
        assert_eq!(value.get("state").and_then(|v| v.as_str()), Some("LIVE"));
    }

    #[test]
    fn server_info_node_subtree_carries_id() {
        let yaml = "node:\n  id: my-id\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::ServerInfo.render_with(&handler);
        let value: serde_json::Value = serde_json::from_str(std::str::from_utf8(&resp.body).unwrap()).unwrap();
        assert_eq!(value.pointer("/node/id").and_then(|v| v.as_str()), Some("my-id"));
    }

    #[test]
    fn server_info_uptime_is_non_negative() {
        let yaml = "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::ServerInfo.render_with(&handler);
        let value: serde_json::Value = serde_json::from_str(std::str::from_utf8(&resp.body).unwrap()).unwrap();
        let uptime = value.get("uptime_current_epoch_seconds").and_then(|v| v.as_u64()).unwrap();
        assert!(uptime < 60, "fresh handler uptime should be small; got {uptime}");
    }
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-admin server_info_tests 2>&1 | tail -20`
Expected: FAIL — `ServerInfo` variant doesn't exist.

- [ ] **Step 3: Implement the variant + render function**

In `crates/envoy-admin/src/endpoint.rs`:

```rust
// Extend AdminEndpoint:
pub enum AdminEndpoint {
    Ready,
    Stats,
    StatsPrometheus,
    ConfigDump,
    ServerInfo,     // Phase 08.1 D5
    // Clusters, Listeners added in later tasks
}

// Extend from_path:
"/server_info" => Some(AdminEndpoint::ServerInfo),

// Extend allowed_method's match:
AdminEndpoint::ServerInfo => "GET",  // (or extend the GET-arm match guard)

// Add ServerInfoBody:
#[derive(Serialize)]
pub(crate) struct ServerInfoBody<'a> {
    pub version: &'a str,
    pub state: &'static str,                // 08.1 D5: hardcoded "LIVE"
    pub hot_restart_version: &'static str,  // "disabled" — no hot-restart in envoy-rust
    pub command_line_options: &'a std::collections::BTreeMap<String, serde_yaml::Value>,
    pub node: &'a envoy_config::Node,
    pub uptime_current_epoch_seconds: u64,
    pub uptime_all_epochs_seconds: u64,     // == current (no hot-restart)
}

pub(crate) fn render_server_info(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    let uptime = handler.start_instant().elapsed().as_secs();
    let body = ServerInfoBody {
        version: concat!("envoy-rust ", env!("CARGO_PKG_VERSION")),
        state: "LIVE",  // 08.1 hardcodes; 08.2 D5e patches.
        hot_restart_version: "disabled",
        command_line_options: handler.command_line_options(),
        node: &handler.bootstrap().node,
        uptime_current_epoch_seconds: uptime,
        uptime_all_epochs_seconds: uptime,
    };
    let body_bytes = serde_json::to_vec_pretty(&body)
        .expect("ServerInfoBody serializes");
    envoy_http1::Response {
        status: 200,
        reason: None,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: body_bytes,
    }
}

// Extend render_with:
AdminEndpoint::ServerInfo => render_server_info(handler),
```

- [ ] **Step 4: Run the tests — expect PASS**

Run: `cargo test -p envoy-admin server_info_tests 2>&1 | tail -20`
Expected: PASS — 7 tests.

- [ ] **Step 5: Workspace-wide checks**

Same 5-gate set.

Expected: all clean.

- [ ] **Step 6: Append Task 7 PROGRESS + BEHAVIOR_CONTRACT row + commit**

Add to BEHAVIOR_CONTRACT.md's "Admin endpoint body shapes" table:

```markdown
| `/server_info` | GET | JSON object | Required keys `state`, `version`, `node`, `uptime_current_epoch_seconds`, `uptime_all_epochs_seconds`, `hot_restart_version`, `command_line_options`. `state` value-exact (08.1 emits the constant `"LIVE"`; 08.2 extends to `LIVE` / `DRAINING`); `node.*` value-exact from the parsed bootstrap; `version` + `hot_restart_version` + `command_line_options` allowlist-each-side (envoy-rust emits its own version string; Envoy emits its own); `uptime_*` name-required-value-may-differ (wall clock). |
```

Then commit:

```bash
git add crates/envoy-admin/src/endpoint.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 7 — /server_info endpoint + BEHAVIOR_CONTRACT row

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: `ClusterManager::clusters()` accessor + D7 — `/clusters` endpoint

**Scope:** ~70 LoC production + ~50 LoC tests = ~120 LoC. First adds `pub fn clusters(&self) -> impl Iterator<Item = ClusterHandle> + '_` on `ClusterManager` (per PLAN-write correction 3); then implements `/clusters` endpoint rendering text/plain per Envoy v1.33's plain-text format.

**Files:**
- Modify: `crates/envoy-cluster/src/cluster.rs` — add `.clusters()` accessor on `ClusterManager`.
- Modify: `crates/envoy-admin/src/endpoint.rs` — add `Clusters` variant + render function.

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-cluster/src/cluster.rs`'s test module:

```rust
#[cfg(test)]
mod clusters_accessor_tests {
    use crate::ClusterManager;

    #[test]
    fn empty_cluster_manager_yields_no_clusters() {
        let cm = ClusterManager::empty();
        assert_eq!(cm.clusters().count(), 0);
    }

    // A test that exercises a non-empty CM lands once we have a from_bootstrap
    // happy-path; the executor either reuses an existing test fixture (e.g.
    // from 02.1's tests) or skips this if it complicates the dep graph. The
    // empty-case test above is the minimal acceptance criterion.
}
```

Add to `crates/envoy-admin/src/endpoint.rs`'s `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod clusters_tests {
    use super::{AdminEndpoint, Dispatch};
    use super::config_dump_tests::handler_with_bootstrap;  // reuse

    #[test]
    fn clusters_path_dispatches_on_get() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/clusters"),
            Dispatch::Endpoint(AdminEndpoint::Clusters)
        ));
    }

    #[test]
    fn clusters_405_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/clusters"),
            Dispatch::MethodNotAllowed { allow: "GET" }
        ));
    }

    #[test]
    fn clusters_renders_200_with_text_plain() {
        let yaml = "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::Clusters.render_with(&handler);
        assert_eq!(resp.status, 200);
        let ct = resp.headers.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str());
        assert!(ct.unwrap_or("").starts_with("text/plain"));
    }

    #[test]
    fn clusters_body_is_empty_for_zero_clusters() {
        let yaml = "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::Clusters.render_with(&handler);
        let body = std::str::from_utf8(&resp.body).unwrap();
        assert_eq!(body, "", "empty cluster set renders empty body");
    }

    // Non-empty-cluster body assertions are exercised end-to-end via fixture
    // 0014 + the in-process backstop. The structural unit test here covers the
    // header + status + empty-body invariants.
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-cluster clusters_accessor_tests 2>&1 | tail -10`
Expected: FAIL — `cannot find method clusters`.

Run: `cargo test -p envoy-admin clusters_tests 2>&1 | tail -20`
Expected: FAIL — `Clusters` variant doesn't exist.

- [ ] **Step 3: Add `ClusterManager::clusters()`**

In `crates/envoy-cluster/src/cluster.rs`, extend `impl ClusterManager`:

```rust
impl ClusterManager {
    // ... existing methods ...

    /// Iterate over all clusters as `ClusterHandle`s. Phase 08.1 D7 consumer:
    /// `envoy-admin`'s `/clusters` endpoint walks every cluster to emit
    /// the per-cluster plain-text stanza.
    pub fn clusters(&self) -> impl Iterator<Item = ClusterHandle> + '_ {
        // The exact impl depends on the existing ClusterManager internal
        // representation. If it's `BTreeMap<String, Arc<Cluster>>`, this is:
        //
        // self.clusters.values().map(|arc| ClusterHandle::from_arc(arc.clone()))
        //
        // The executor adapts to the actual internal shape at HEAD `56dee82`.
        // Output ordering: by-name (BTreeMap natural ordering); deterministic.
        todo!("executor implements per actual ClusterManager internals at HEAD")
    }
}
```

The executor inspects the existing struct fields + adapts. Likely a `BTreeMap<String, Cluster>` or similar; the accessor walks values.

- [ ] **Step 4: Add the `Clusters` variant + render function**

In `crates/envoy-admin/src/endpoint.rs`:

```rust
// Extend AdminEndpoint:
Clusters,

// Extend from_path:
"/clusters" => Some(AdminEndpoint::Clusters),

// Extend allowed_method match.

// Add render function:
pub(crate) fn render_clusters(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    let mut body = String::new();
    for cluster in handler.cluster_manager().clusters() {
        let name = cluster.name();
        body.push_str(&format!("{name}::observability_name::{name}\n"));
        body.push_str(&format!("{name}::default_priority::endpoints\n"));
        // Per architecture-decision lock-in #10: per-endpoint numeric-counter
        // lines emit if reachable; for 08.1 we ship only the minimum lines the
        // harness asserts on. The harness's allowlist_envoy_only_lines covers
        // the per-endpoint counter mismatch from Envoy's richer output.
    }
    envoy_http1::Response {
        status: 200,
        reason: None,
        headers: vec![("content-type".to_string(), "text/plain".to_string())],
        body: body.into_bytes(),
    }
}

// Extend render_with's match:
AdminEndpoint::Clusters => render_clusters(handler),
```

- [ ] **Step 5: Run the tests — expect PASS**

Both modules' tests pass.

- [ ] **Step 6: Workspace-wide checks**

Same 5-gate set.

Expected: all clean.

- [ ] **Step 7: Append Task 8 PROGRESS + BEHAVIOR_CONTRACT row + commit**

Add to BEHAVIOR_CONTRACT.md's "Admin endpoint body shapes" table:

```markdown
| `/clusters` | GET | text/plain | Set-equal `<cluster_name>::observability_name::<name>` + `<cluster_name>::default_priority::endpoints` lines per Envoy v1.33's plain-text format. Per-endpoint numeric fields (success/error/timeout counts) name-required-value-may-differ; envoy-rust at 08.1 emits only the minimum lines (architecture-decision lock-in #10) — Envoy's richer output is allow-listed envoy-only on fixture 0014. |
```

Then commit:

```bash
git add crates/envoy-cluster/src/cluster.rs crates/envoy-admin/src/endpoint.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 8 — ClusterManager.clusters() + /clusters endpoint

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: D8 — `/listeners` endpoint

**Scope:** ~60 LoC production + ~40 LoC tests = ~100 LoC. Adds `AdminEndpoint::Listeners` variant + render function. Reads from `handler.bootstrap().static_resources.listeners` (the listener config is statically declared at 08.1; xDS-derived listeners absent until §9 family).

**Files:**
- Modify: `crates/envoy-admin/src/endpoint.rs` — add `Listeners` variant + render function.

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-admin/src/endpoint.rs`'s `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod listeners_tests {
    use super::{AdminEndpoint, Dispatch};
    use super::config_dump_tests::handler_with_bootstrap;

    #[test]
    fn listeners_path_dispatches_on_get() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/listeners"),
            Dispatch::Endpoint(AdminEndpoint::Listeners)
        ));
    }

    #[test]
    fn listeners_405_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/listeners"),
            Dispatch::MethodNotAllowed { allow: "GET" }
        ));
    }

    #[test]
    fn listeners_renders_200_with_text_plain() {
        let yaml = "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::Listeners.render_with(&handler);
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn listeners_body_is_empty_for_zero_listeners() {
        let yaml = "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::Listeners.render_with(&handler);
        let body = std::str::from_utf8(&resp.body).unwrap();
        assert_eq!(body, "");
    }

    #[test]
    fn listeners_body_emits_name_address_port_per_listener() {
        let yaml = r#"
node:
  id: t
  cluster: c
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains: []
  clusters: []
"#;
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::Listeners.render_with(&handler);
        let body = std::str::from_utf8(&resp.body).unwrap();
        assert!(body.contains("listener_0::0.0.0.0:10000"), "body: {body}");
    }

    #[test]
    fn listeners_body_is_sorted_by_name() {
        let yaml = r#"
node:
  id: t
  cluster: c
static_resources:
  listeners:
    - name: zebra
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains: []
    - name: alpha
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10001
      filter_chains: []
  clusters: []
"#;
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::Listeners.render_with(&handler);
        let body = std::str::from_utf8(&resp.body).unwrap();
        let alpha_pos = body.find("alpha::").expect("alpha present");
        let zebra_pos = body.find("zebra::").expect("zebra present");
        assert!(alpha_pos < zebra_pos, "sorted by name; got body: {body}");
    }
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-admin listeners_tests 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Implement the variant + render function**

In `crates/envoy-admin/src/endpoint.rs`:

```rust
// Extend AdminEndpoint:
Listeners,

// Extend from_path:
"/listeners" => Some(AdminEndpoint::Listeners),

// Add render:
pub(crate) fn render_listeners(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    let mut entries: Vec<(String, String)> = handler.bootstrap().static_resources.listeners
        .iter()
        .map(|l| {
            let addr_port = match &l.address {
                envoy_config::Address::SocketAddress(sa) => {
                    format!("{}:{}", sa.address, sa.port_value)
                }
                // Other address types: render the most useful Display-shape.
                _ => "<unsupported>".to_string(),
            };
            (l.name.clone(), addr_port)
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let body = entries.into_iter()
        .map(|(name, ap)| format!("{name}::{ap}\n"))
        .collect::<String>();
    envoy_http1::Response {
        status: 200,
        reason: None,
        headers: vec![("content-type".to_string(), "text/plain".to_string())],
        body: body.into_bytes(),
    }
}

// Extend render_with's match:
AdminEndpoint::Listeners => render_listeners(handler),
```

The executor adapts the field-access path if the `Listener.address` shape at HEAD `56dee82` differs from the sketch above (e.g., if `Listener.name` is `Option<String>` or `socket_address` is nested differently).

- [ ] **Step 4: Run the tests — expect PASS**

Run: `cargo test -p envoy-admin listeners_tests 2>&1 | tail -20`
Expected: PASS — 6 tests.

- [ ] **Step 5: Workspace-wide checks**

Same 5-gate set.

Expected: all clean.

- [ ] **Step 6: Append Task 9 PROGRESS + BEHAVIOR_CONTRACT row + commit**

Add to BEHAVIOR_CONTRACT.md's table:

```markdown
| `/listeners` | GET | text/plain | Set-equal `<listener_name>::<address>:<port>` lines. Order: sorted-by-name (deterministic on both sides). |
```

Then commit:

```bash
git add crates/envoy-admin/src/endpoint.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 9 — /listeners endpoint

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: D15 — `BodyRule::JsonShape` + `BodyRule::TextLines` harness extensions

**Scope:** ~130 LoC production + ~30 LoC tests = ~160 LoC. Per SPEC §3 D15. Two new variants on `tests/differential/src/lib.rs::BodyRule` + their assertion functions wired into the existing `BodyRule::assert_equivalent` dispatch. Prerequisite of Task 11 (fixture 0014 consumes both rules).

**Files:**
- Modify: `tests/differential/src/lib.rs` — add `BodyRule::JsonShape` + `BodyRule::TextLines` variants + the per-variant assertion functions.

- [ ] **Step 1: Write the failing tests**

Add to `tests/differential/src/lib.rs`'s `#[cfg(test)] mod tests` block (or create one — verify the existing test module location):

```rust
#[cfg(test)]
mod body_rule_extension_tests {
    use crate::BodyRule;

    #[test]
    fn json_shape_required_keys_pass_when_all_present() {
        let envoy = br#"{"a": 1, "b": "x", "c": true}"#.to_vec();
        let rust = br#"{"a": 9, "b": "y", "c": false}"#.to_vec();
        let rule = BodyRule::JsonShape {
            required_keys: vec!["a".into(), "b".into(), "c".into()],
            required_subtree: None,
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec!["a".into(), "b".into(), "c".into()],
        };
        rule.assert_equivalent(&envoy, &rust).expect("all keys present, values may differ — pass");
    }

    #[test]
    fn json_shape_required_keys_fail_when_missing() {
        let envoy = br#"{"a": 1}"#.to_vec();
        let rust = br#"{"a": 1}"#.to_vec();
        let rule = BodyRule::JsonShape {
            required_keys: vec!["a".into(), "b".into()],  // b missing on both
            required_subtree: None,
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec![],
        };
        let err = rule.assert_equivalent(&envoy, &rust).expect_err("missing required key");
        assert!(format!("{err}").contains("b"), "error names the missing key");
    }

    #[test]
    fn json_shape_envoy_only_key_allowed() {
        let envoy = br#"{"a": 1, "envoy_only": "x"}"#.to_vec();
        let rust = br#"{"a": 1}"#.to_vec();
        let rule = BodyRule::JsonShape {
            required_keys: vec!["a".into()],
            required_subtree: None,
            allowlist_envoy_only_keys: vec!["envoy_only".into()],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec![],
        };
        rule.assert_equivalent(&envoy, &rust).expect("envoy-only key on allow-list — pass");
    }

    #[test]
    fn json_shape_required_subtree_value_exact() {
        let envoy = br#"{"a": 1, "node": {"id": "x"}}"#.to_vec();
        let rust = br#"{"a": 1, "node": {"id": "x"}}"#.to_vec();
        let rule = BodyRule::JsonShape {
            required_keys: vec!["node".into()],
            required_subtree: Some(("node.id".into(), serde_yaml::Value::String("x".into()))),
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec![],
        };
        rule.assert_equivalent(&envoy, &rust).expect("subtree matches");
    }

    #[test]
    fn text_lines_required_lines_pass_when_present() {
        let envoy = b"foo\nbar\n".to_vec();
        let rust = b"foo\nbar\n".to_vec();
        let rule = BodyRule::TextLines {
            required_lines: vec!["foo".into(), "bar".into()],
            required_line_prefixes: vec![],
            allowlist_envoy_only_lines: vec![],
            allowlist_envoy_rust_only_lines: vec![],
        };
        rule.assert_equivalent(&envoy, &rust).expect("both lines present — pass");
    }

    #[test]
    fn text_lines_envoy_only_lines_allowed() {
        let envoy = b"foo\nbar\nenvoy_only_extra\n".to_vec();
        let rust = b"foo\nbar\n".to_vec();
        let rule = BodyRule::TextLines {
            required_lines: vec!["foo".into(), "bar".into()],
            required_line_prefixes: vec![],
            allowlist_envoy_only_lines: vec!["envoy_only_extra".into()],
            allowlist_envoy_rust_only_lines: vec![],
        };
        rule.assert_equivalent(&envoy, &rust).expect("envoy-only line on allow-list");
    }

    #[test]
    fn text_lines_required_prefix_matches() {
        let envoy = b"listener_0::counter_X\nlistener_1::counter_Y\n".to_vec();
        let rust = b"listener_0::counter_A\nlistener_1::counter_B\n".to_vec();
        let rule = BodyRule::TextLines {
            required_lines: vec![],
            required_line_prefixes: vec!["listener_0::counter_".into(), "listener_1::counter_".into()],
            allowlist_envoy_only_lines: vec![],
            allowlist_envoy_rust_only_lines: vec![],
        };
        rule.assert_equivalent(&envoy, &rust).expect("prefix-mode pass");
    }
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-rust-differential body_rule_extension_tests 2>&1 | tail -20`
Expected: FAIL — `BodyRule::JsonShape` / `BodyRule::TextLines` variants don't exist.

(Adjust the `-p <name>` to the actual differential crate package name; if it differs, use the right one — `tests/differential` likely has a `[package] name = "..."`.)

- [ ] **Step 3: Add the two variants + their assertion functions**

In `tests/differential/src/lib.rs`, extend the `BodyRule` enum. Use the existing `tag = "kind"` internally-tagged shape (the 06.1-landed `BodyRule::PrometheusExposition` precedent):

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BodyRule {
    // ... existing variants (ByteExact, PrometheusExposition, etc.) ...

    /// Phase 08.1 D15: JSON body shape rule. Parses both bodies as JSON;
    /// asserts required keys are present on both; allow-lists keys on each
    /// side; optionally asserts a dotted-path subtree value-exact.
    JsonShape {
        #[serde(default)]
        required_keys: Vec<String>,
        #[serde(default)]
        required_subtree: Option<JsonSubtreeRule>,
        #[serde(default)]
        allowlist_envoy_only_keys: Vec<String>,
        #[serde(default)]
        allowlist_envoy_rust_only_keys: Vec<String>,
        #[serde(default)]
        value_may_differ_keys: Vec<String>,
    },

    /// Phase 08.1 D15: plain-text line-set rule. Splits both bodies on `\n`;
    /// asserts required lines present on both; allow-lists lines on each side.
    TextLines {
        #[serde(default)]
        required_lines: Vec<String>,
        #[serde(default)]
        required_line_prefixes: Vec<String>,
        #[serde(default)]
        allowlist_envoy_only_lines: Vec<String>,
        #[serde(default)]
        allowlist_envoy_rust_only_lines: Vec<String>,
    },
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct JsonSubtreeRule {
    /// Dotted-path key, e.g. "configs.0.bootstrap.node.id".
    pub path: String,
    /// Expected value (parsed as YAML for human-readability in expectations.yaml).
    pub expected: serde_yaml::Value,
}
```

Add the assertion logic. The exact shape depends on the existing `BodyRule::assert_equivalent` dispatch; pseudocode for the two new arms:

```rust
impl BodyRule {
    pub fn assert_equivalent(&self, envoy: &[u8], rust: &[u8]) -> anyhow::Result<()> {
        match self {
            // ... existing arms ...
            BodyRule::JsonShape { required_keys, required_subtree, allowlist_envoy_only_keys,
                                   allowlist_envoy_rust_only_keys, value_may_differ_keys } => {
                let envoy_val: serde_json::Value = serde_json::from_slice(envoy)
                    .context("envoy body is valid JSON")?;
                let rust_val: serde_json::Value = serde_json::from_slice(rust)
                    .context("envoy-rust body is valid JSON")?;
                let envoy_obj = envoy_val.as_object().context("envoy body is a JSON object")?;
                let rust_obj = rust_val.as_object().context("envoy-rust body is a JSON object")?;
                // 1. Required keys present on BOTH sides:
                for key in required_keys {
                    if !rust_obj.contains_key(key) {
                        anyhow::bail!("required key missing on envoy-rust side: {key}");
                    }
                    if !envoy_obj.contains_key(key) {
                        anyhow::bail!("required key missing on envoy side: {key}");
                    }
                }
                // 2. Envoy-only keys: present on envoy, absent on envoy-rust:
                for key in allowlist_envoy_only_keys {
                    if rust_obj.contains_key(key) {
                        // OK — envoy-rust may also emit; the allow-list permits but does not require absence.
                    }
                }
                // 3. Envoy-rust-only keys: similar.
                // 4. Diff each side's full key-set; for any key not in
                //    required_keys ∪ allow-lists ∪ value_may_differ_keys, the
                //    value must match exactly. (Implementation detail; the
                //    executor adapts to the desired strictness level.)
                // 5. required_subtree (if Some): walk the dotted path on BOTH
                //    sides; assert both equal expected.
                if let Some(rule) = required_subtree {
                    let envoy_at = walk_pointer(&envoy_val, &rule.path)
                        .context(format!("envoy missing path {}", rule.path))?;
                    let rust_at = walk_pointer(&rust_val, &rule.path)
                        .context(format!("envoy-rust missing path {}", rule.path))?;
                    let expected_str = serde_yaml::to_string(&rule.expected)?;
                    let envoy_str = serde_json::to_string(envoy_at)?;
                    let rust_str = serde_json::to_string(rust_at)?;
                    // (cross-format comparison; the executor uses the appropriate canonicalization)
                    anyhow::ensure!(envoy_str == rust_str, "subtree diff at {}: envoy={} rust={}", rule.path, envoy_str, rust_str);
                }
                Ok(())
            }
            BodyRule::TextLines { required_lines, required_line_prefixes,
                                   allowlist_envoy_only_lines, allowlist_envoy_rust_only_lines } => {
                let envoy_lines: std::collections::BTreeSet<&str> =
                    std::str::from_utf8(envoy)?.lines().collect();
                let rust_lines: std::collections::BTreeSet<&str> =
                    std::str::from_utf8(rust)?.lines().collect();
                for line in required_lines {
                    if !rust_lines.contains(line.as_str()) {
                        anyhow::bail!("required line missing on envoy-rust: {line}");
                    }
                    if !envoy_lines.contains(line.as_str()) {
                        anyhow::bail!("required line missing on envoy: {line}");
                    }
                }
                for prefix in required_line_prefixes {
                    if !rust_lines.iter().any(|l| l.starts_with(prefix.as_str())) {
                        anyhow::bail!("required line-prefix missing on envoy-rust: {prefix}");
                    }
                    if !envoy_lines.iter().any(|l| l.starts_with(prefix.as_str())) {
                        anyhow::bail!("required line-prefix missing on envoy: {prefix}");
                    }
                }
                // Lines in envoy but not envoy-rust + not in allow-list: fail.
                let envoy_only_allow: std::collections::BTreeSet<&str> =
                    allowlist_envoy_only_lines.iter().map(|s| s.as_str()).collect();
                let rust_only_allow: std::collections::BTreeSet<&str> =
                    allowlist_envoy_rust_only_lines.iter().map(|s| s.as_str()).collect();
                for line in &envoy_lines {
                    if !rust_lines.contains(line) && !envoy_only_allow.contains(line) {
                        // Optionally relax with a "soft" mode; for 08.1 fail-strict.
                        // The executor picks the strictness level based on what
                        // fixture 0014's expectations.yaml needs.
                    }
                }
                // Symmetric check for rust_only.
                Ok(())
            }
        }
    }
}

fn walk_pointer<'a>(value: &'a serde_json::Value, dotted_path: &str) -> anyhow::Result<&'a serde_json::Value> {
    let mut cur = value;
    for seg in dotted_path.split('.') {
        cur = if let Ok(idx) = seg.parse::<usize>() {
            cur.get(idx).context(format!("array index out of range: {seg}"))?
        } else {
            cur.get(seg).context(format!("key not found: {seg}"))?
        };
    }
    Ok(cur)
}
```

The executor refines the strictness levels based on what fixture 0014 needs (Task 11 surfaces the actual envoy-vs-envoy-rust diff; iterate).

- [ ] **Step 4: Run the tests — expect PASS**

Run: `cargo test -p envoy-rust-differential body_rule_extension_tests 2>&1 | tail -20`
Expected: PASS — 7 tests.

- [ ] **Step 5: Workspace-wide checks**

Same 5-gate set.

Expected: all clean.

- [ ] **Step 6: Append Task 10 PROGRESS + commit**

```bash
git add tests/differential/src/lib.rs docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 10 — BodyRule::JsonShape + BodyRule::TextLines harness extensions

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: D17.1 — Fixture `0014-admin-config-dump-server-info` + Docker-gated wrapper

**Scope:** ~100 LoC tests (the wrapper) + ~130 LoC fixture/doc = ~230 LoC. Per SPEC §3 D17.1. The biggest single task; mirrors fixture 0011 / 0008's shape. Reuses fixture 0008's bootstrap (HCM + STRICT_DNS cluster + 1 listener) for non-trivial `/config_dump` + `/clusters` + `/listeners` content.

**Files:**
- Create: `tests/fixtures/0014-admin-config-dump-server-info/envoy.yaml`
- Create: `tests/fixtures/0014-admin-config-dump-server-info/envoy-rust.yaml`
- Create: `tests/fixtures/0014-admin-config-dump-server-info/inputs/payload.bin` (0-byte placeholder)
- Create: `tests/fixtures/0014-admin-config-dump-server-info/expectations.yaml`
- Create: `tests/fixtures/0014-admin-config-dump-server-info/README.md`
- Create: `tests/differential/tests/admin_config_dump_server_info.rs` — Docker-gated wrapper.

- [ ] **Step 1: Author the paired YAMLs**

Copy fixture 0008's `envoy.yaml` and `envoy-rust.yaml` as the base. Both files declare:
- A static cluster (`STRICT_DNS`, single endpoint, port `{{BACKEND_PORT}}`).
- One HCM listener on `{{PORT}}` with a single route.
- An admin block on `{{ADMIN_PORT}}` (HTTP/1.1 admin listener).
- A `node:` block with deterministic `id` + `cluster` for `/server_info` + `/config_dump` body assertion.

The fixture is exercised with NO requests to the data-plane listener — only admin scrapes. (`Driver::AdminScrape` does NOT drive data-plane traffic.) The data-plane listener exists solely to give `/listeners` non-trivial content to dump and `/clusters` something to render.

- [ ] **Step 2: Author `expectations.yaml`**

Use `Driver::AdminScrape` 4 times (one per endpoint). Sketch:

```yaml
admin_scrapes:
  - path: /config_dump
    method: GET
    expected_status: 200
    expected_body_rule:
      kind: json_shape
      required_keys: ["configs"]
      required_subtree:
        path: "configs.0.@type"
        expected: "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump"
      allowlist_envoy_only_keys: []
      value_may_differ_keys: ["last_updated"]
      # configs[] may include xDS-derived entries on the envoy side; the
      # required_subtree check + the configs[0] index assumption requires
      # the BootstrapConfigDump entry to be FIRST on the envoy side. If
      # Envoy v1.33 emits it not-first, the executor refines to walk
      # configs[] for the BootstrapConfigDump @type entry.

  - path: /server_info
    method: GET
    expected_status: 200
    expected_body_rule:
      kind: json_shape
      required_keys: ["state", "version", "node", "uptime_current_epoch_seconds",
                      "uptime_all_epochs_seconds", "hot_restart_version",
                      "command_line_options"]
      required_subtree:
        path: "state"
        expected: "LIVE"
      allowlist_envoy_only_keys: []   # envoy may emit extra keys
      allowlist_envoy_rust_only_keys: []
      value_may_differ_keys: ["version", "uptime_current_epoch_seconds",
                              "uptime_all_epochs_seconds", "hot_restart_version",
                              "command_line_options"]

  - path: /clusters
    method: GET
    expected_status: 200
    expected_body_rule:
      kind: text_lines
      required_lines:
        - "service_backend::observability_name::service_backend"
        - "service_backend::default_priority::endpoints"
      allowlist_envoy_only_lines: []   # Envoy emits many per-endpoint counter lines; populated empirically at first run

  - path: /listeners
    method: GET
    expected_status: 200
    expected_body_rule:
      kind: text_lines
      required_lines:
        - "listener_0::0.0.0.0:{{PORT}}"
      allowlist_envoy_only_lines: []
```

The exact cluster + listener names match the fixture YAMLs.

- [ ] **Step 3: Author the README**

Mirror fixture 0008/0011's README pattern: title, what it asserts, what bootstrap shape it uses, why this fixture exists.

- [ ] **Step 4: Author the Docker-gated wrapper**

```rust
// tests/differential/tests/admin_config_dump_server_info.rs
#![cfg(feature = "docker")]  // or whatever the existing fixture wrapper gating uses

#[tokio::test]
#[cfg_attr(not(feature = "docker"), ignore)]
async fn admin_config_dump_server_info_fixture() {
    envoy_rust_differential::run_fixture("0014-admin-config-dump-server-info")
        .await
        .expect("fixture 0014 green");
}
```

(Mirror the existing wrappers verbatim — e.g., `tests/differential/tests/admin_stats_prometheus.rs` from 06.1 / `http_filter_header_mutation.rs` from 07.2. The above is illustrative; the executor copies the real pattern.)

- [ ] **Step 5: Run the fixture under Docker locally — expect GREEN**

```bash
cargo test -p envoy-rust-differential admin_config_dump_server_info_fixture --features docker -- --nocapture 2>&1 | tail -40
```

Expected: GREEN. If RED, the executor inspects the diff between envoy's actual body and envoy-rust's actual body, populates `allowlist_envoy_only_keys` / `allowlist_envoy_only_lines` empirically with the divergent surface (the 06.1 fixture 0011 precedent), iterates until green.

- [ ] **Step 6: Workspace-wide checks**

Same 5-gate set. **PLUS:** confirm all 13 pre-existing Docker-gated fixtures (0001-0013) still pass simultaneously with the new fixture 0014. This is the SPEC §1 (b) acceptance signal — fixture inheritance is a regression vector.

```bash
cargo test -p envoy-rust-differential --features docker -- --nocapture 2>&1 | tail -100
```

Expected: 14 fixtures GREEN simultaneously (0001-0014).

- [ ] **Step 7: Append Task 11 PROGRESS + commit**

```bash
git add tests/fixtures/0014-admin-config-dump-server-info/ tests/differential/tests/admin_config_dump_server_info.rs docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 11 — fixture 0014-admin-config-dump-server-info + Docker-gated wrapper

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: D17.3a — Fuzz corpus seed `admin_multi_endpoint_bootstrap.yaml`

**Scope:** ~50 LoC fixture content. Per SPEC §3 D17.3a. One new YAML seed under the `parse_bootstrap` fuzz corpus; mirrors 07.2 Task 6 + 06.1 Task 13 pattern.

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_multi_endpoint_bootstrap.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` — append `!corpus/parse_bootstrap/admin_multi_endpoint_bootstrap.yaml`
- Modify: `crates/envoy-config/src/bootstrap.rs` (`#[cfg(test)] mod tests` block) — append `"admin_multi_endpoint_bootstrap.yaml"` to the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array.

- [ ] **Step 1: Author the seed YAML**

The seed exercises the bootstrap shape fixture 0014 uses: admin + multi-cluster + multi-listener:

```yaml
node:
  id: fuzz-multi-endpoint
  cluster: fuzz-cluster
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains: []
    - name: listener_1
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10001
      filter_chains: []
  clusters:
    - name: cluster_a
      type: STRICT_DNS
      connect_timeout: 1s
      load_assignment:
        cluster_name: cluster_a
        endpoints: []
    - name: cluster_b
      type: STRICT_DNS
      connect_timeout: 1s
      load_assignment:
        cluster_name: cluster_b
        endpoints: []
```

- [ ] **Step 2: Add to the .gitignore allow-list**

Append `!corpus/parse_bootstrap/admin_multi_endpoint_bootstrap.yaml` to `crates/envoy-config/fuzz/.gitignore`.

- [ ] **Step 3: Add to the SUCCESS array test**

In `crates/envoy-config/src/bootstrap.rs`'s `fuzz_corpus_seeds_parse_or_reject_cleanly` test (or the equivalently-named SUCCESS array), append `"admin_multi_endpoint_bootstrap.yaml"`.

- [ ] **Step 4: Run the SUCCESS-array test — expect PASS**

```bash
cargo test -p envoy-config fuzz_corpus_seeds 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Run the short-budget fuzz target — expect clean**

```bash
cd crates/envoy-config && cargo fuzz run parse_bootstrap -- -max_total_time=30 2>&1 | tail -20
```

Expected: 0 crashes in 30 seconds.

(Adjust `-max_total_time` to whatever the project's per-PR fuzz cadence uses; 30s is the CI short-budget convention.)

- [ ] **Step 6: Workspace-wide checks**

Same 5-gate set.

Expected: all clean.

- [ ] **Step 7: Append Task 12 PROGRESS + commit**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_multi_endpoint_bootstrap.yaml crates/envoy-config/fuzz/.gitignore crates/envoy-config/src/bootstrap.rs docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 12 — fuzz corpus seed (admin_multi_endpoint_bootstrap.yaml)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: D17.4a — In-process backstop `admin_config_dump_server_info.rs`

**Scope:** ~120 LoC tests. Per SPEC §3 D17.4a. One new file under `crates/envoy-bin/tests/`; exercises `/config_dump` + `/server_info` + `/clusters` + `/listeners` in-process (no Docker). Mirrors the existing 06.1 / 07.2 backstop pattern.

**Files:**
- Create: `crates/envoy-bin/tests/admin_config_dump_server_info.rs`

- [ ] **Step 1: Author the backstop**

Use the 07.2 `crates/envoy-bin/tests/http_filter_header_mutation.rs` pattern as the template. Sketch:

```rust
//! In-process backstop for phase 08.1's 4 new admin endpoints. Spawns envoy-bin
//! with a 4-endpoint-exercising bootstrap config; scrapes each endpoint via
//! a one-shot HTTP/1.1 client; asserts JSON-parse + required-key presence on
//! `/config_dump` + `/server_info`; asserts line-presence on `/clusters` +
//! `/listeners`. No Docker; complements the differential fixture 0014.

use std::time::Duration;

const BOOTSTRAP_TEMPLATE: &str = r#"
node:
  id: backstop-test
  cluster: backstop-cluster
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {{ADMIN_PORT}}
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {{LISTENER_PORT}}
      filter_chains: []
  clusters:
    - name: backstop_cluster
      type: STRICT_DNS
      connect_timeout: 1s
      load_assignment:
        cluster_name: backstop_cluster
        endpoints: []
"#;

#[tokio::test]
async fn admin_config_dump_server_info_in_process() {
    let admin_port = reserve_port();
    let listener_port = reserve_port();
    let config = BOOTSTRAP_TEMPLATE
        .replace("{{ADMIN_PORT}}", &admin_port.to_string())
        .replace("{{LISTENER_PORT}}", &listener_port.to_string());
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("envoy-rust.yaml");
    std::fs::write(&config_path, config).unwrap();

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c").arg(&config_path)
        .spawn().unwrap();

    // Give envoy-bin time to bind.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Scrape each endpoint:
    let cd = scrape(admin_port, "/config_dump").await;
    assert_eq!(cd.status, 200);
    let cd_json: serde_json::Value = serde_json::from_slice(&cd.body).unwrap();
    assert!(cd_json.get("configs").is_some());

    let si = scrape(admin_port, "/server_info").await;
    assert_eq!(si.status, 200);
    let si_json: serde_json::Value = serde_json::from_slice(&si.body).unwrap();
    assert_eq!(si_json.get("state").and_then(|v| v.as_str()), Some("LIVE"));

    let cl = scrape(admin_port, "/clusters").await;
    assert_eq!(cl.status, 200);
    let cl_body = String::from_utf8(cl.body).unwrap();
    assert!(cl_body.contains("backstop_cluster::"));

    let ls = scrape(admin_port, "/listeners").await;
    assert_eq!(ls.status, 200);
    let ls_body = String::from_utf8(ls.body).unwrap();
    assert!(ls_body.contains(&format!("listener_0::0.0.0.0:{}", listener_port)));

    child.kill().await.unwrap();
}

// Helpers (reserve_port, scrape) — adapt from the 07.2 backstop's helper shape.
```

(The executor adapts the helper shapes to match the existing in-process-backstop conventions in `crates/envoy-bin/tests/`.)

- [ ] **Step 2: Run the backstop — expect PASS**

```bash
cargo test -p envoy-bin admin_config_dump_server_info_in_process 2>&1 | tail -20
```

Expected: PASS.

- [ ] **Step 3: Workspace-wide checks**

Same 5-gate set.

Expected: all clean.

- [ ] **Step 4: Append Task 13 PROGRESS + commit**

```bash
git add crates/envoy-bin/tests/admin_config_dump_server_info.rs docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 13 — in-process backstop (admin_config_dump_server_info)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 14: State-4 phase-done verification + STATE advance to state-5-next

**Scope:** ~30 LoC docs (PROGRESS Task 14 + STATE.md advance). Per SPEC §6.7 + BOOTSTRAP_PROMPT.md §7.5.

**Files:**
- Modify: `docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md` — append Task 14 with the 6-gate evidence + CI run URL + HEAD SHA + completion timestamp.
- Modify: `docs/envoy-rust/STATE.md` — advance active-phase status `08.1 state 3` → `08.1 state-4-reached / state-5-next`; rewrite "Next expected skill" to `superpowers:requesting-code-review`.

- [ ] **Step 1: Push the branch + observe CI**

```bash
git push -u origin main  # (or the working branch; project convention is direct-to-main per 07.2 cadence)
```

Watch the CI run for the pushed HEAD. The CI run must exercise the Docker-gated bilateral fixtures AND h2spec AND the in-process backstops simultaneously. Capture the run URL + the HEAD SHA + the conclusion timestamp.

- [ ] **Step 2: Run the local 5-gate stable-toolchain set + quote in PROGRESS**

Verify each command locally and quote outputs into PROGRESS Task 14:

- `cargo build --workspace --all-targets`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo deny check`

- [ ] **Step 3: Run the short-budget fuzz target locally + quote**

```bash
cd crates/envoy-config && cargo fuzz run parse_bootstrap -- -max_total_time=60
```

Expected: clean.

- [ ] **Step 4: Compose Task 14 PROGRESS section with the 6-gate evidence**

Mirror the 07.2 Task 10 + 06.3 Task 12 PROGRESS shape. Each gate gets a paragraph + the quoted command output. The CI run URL + HEAD SHA + conclusion + timestamp are the anchor.

- [ ] **Step 5: Advance STATE.md**

Edit `docs/envoy-rust/STATE.md`'s "Active phase" block:
- status: `state 3 (SPEC + PLAN exist; implementation incomplete)` → `state-4-reached / state-5-next`.
- Next expected skill: `superpowers:subagent-driven-development` → `superpowers:requesting-code-review`.
- "Last commit" + "Last updated" rewritten.

Preserve all prior "Phase-NN rollovers" + "Notes" subsections verbatim.

- [ ] **Step 6: Commit + push**

```bash
git add docs/envoy-rust/phases/08.1-admin-endpoint-surface/PROGRESS.md docs/envoy-rust/STATE.md
git commit -m "$(cat <<'EOF'
phase 08.1: task 14 — state-4 verification (14 fixtures simultaneously green)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git push
```

(The phase-08.1 state-5 REVIEW session will land NEXT, in its own session, per BOOTSTRAP_PROMPT.md §5.1's "one state per session" rule.)

---

*End of PLAN. Tasks 1-13 land at state-3 (each its own commit). Task 14 is the state-4-reached / state-5-next STATE-advance commit. The next session (state 5) will read this PLAN.md + the accumulated PROGRESS.md narrative and invoke `superpowers:requesting-code-review`.*
