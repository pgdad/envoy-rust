# Phase 08.1 (`08.1-admin-endpoint-surface`) — PROGRESS

> Per-task narrative log of phase 08.1 execution. Created at state-2
> alongside `PLAN.md` per the 07.2 (`c7dea4c`) / 06.2 (`dc00750`) cadence.
> Each subsequent task appends a `## Task N — <subject>` section with
> work summary, tests landed (names + LoC tally), per-task deviations
> from PLAN.md (D-3.5 append-only discipline), LoC delta, and the
> 5-gate test-bucket attestation (`cargo build` / `cargo clippy` /
> `cargo fmt` / `cargo test --workspace` / `cargo deny check`). Per
> 07.1-REVIEW doctrine + 07.2 ratification, `cargo deny check` output
> MUST be explicitly quoted in every per-task attestation — do not
> write "assumed no-op."

---

## Task 1 — preamble (PLAN-write SPEC corrections + architecture-decision lock-ins)

Recorded at PLAN-write time (state-2). Lands BEFORE Tasks 1-13's execution.

### PLAN-write SPEC corrections

The 08.1 SPEC committed at `56dee82` reflects the parent-08 state-2 split decision; it was authored at split time with the 07.2-landed tree in view but six details drifted against HEAD `56dee82` between SPEC write and PLAN write. Per the user's standing preference `feedback_pick_recommendation`, each correction picks the working option; all six are folded into the PLAN's task steps. Verbatim:

1. **`DRAIN_BUDGET` hoist target site is a *module-level* `pub const`, not a `pub const` at the existing local-fn site.** Current `crates/envoy-listener/src/lib.rs:165` site is `const DRAIN_BUDGET: Duration = Duration::from_secs(5);` declared *inside* the body of `Listener::serve` (a local-fn const). And `crates/envoy-admin/src/handler.rs:28` declares a separate module-level `const DRAIN_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);`. **Correction:** the hoist deletes both existing declarations and introduces a single new `pub const DRAIN_BUDGET: Duration = Duration::from_secs(5);` at module level in `crates/envoy-listener/src/lib.rs`. `envoy-admin` already depends on `envoy-listener` (verified at `crates/envoy-admin/Cargo.toml:20`); the import is one-line: `use envoy_listener::DRAIN_BUDGET;`. Lands at PLAN Task 2.

2. **`format_iso8601` reuse for `/config_dump` `last_updated` requires visibility promotion + crate dep addition.** Verified at `crates/envoy-accesslog/src/default_format.rs:83`: the function signature is `pub(crate) fn format_iso8601(s: &mut String, t: SystemTime)` — `pub(crate)` (not reachable from `envoy-admin`) AND the `default_format` module is not declared `pub` in `crates/envoy-accesslog/src/lib.rs` (private module). Additionally, `crates/envoy-admin/Cargo.toml` does not currently depend on `envoy-accesslog`. **Correction:** add a `pub fn format_iso8601(t: SystemTime) -> String` wrapper to `crates/envoy-accesslog/src/lib.rs` that calls the existing `pub(crate)` internal function; add `envoy-accesslog = { path = "../envoy-accesslog" }` to `crates/envoy-admin/Cargo.toml`. Lands at PLAN Task 5.

3. **`ClusterManager` does NOT currently expose a `.clusters()` accessor.** Verified at `crates/envoy-cluster/src/cluster.rs:201-228`: `ClusterManager` has `.get(&name) -> Option<ClusterHandle>` + `.empty() -> Self` only — no iterator/slice accessor over the cluster set. **Correction:** Task 8 (D7) lands `pub fn clusters(&self) -> impl Iterator<Item = ClusterHandle> + '_` as the D7 prerequisite. ~5-10 LoC + 1 unit test.

4. **`AdminHandler::new` current signature is the 2-arg shape the SPEC describes; D13a widens.** Confirmed at `crates/envoy-admin/src/handler.rs:36`: `pub fn new(config: Arc<AdminConfig>, registry: Arc<StatsRegistry>) -> Self`. Seven in-file test call sites at lines 291, 318, 341, 363, 386, 416, 461 must update at Task 5; the production call site in `crates/envoy-bin/src/admin.rs` also updates. **No SPEC correction; recording for pre-task drift verification.** Additionally: this PLAN actually widens to 6-arg (not 5-arg) because `command_line_options` is built once at construction time per architecture-decision lock-in #7. Recording as PLAN-time deviation #1 from SPEC §3 D13a's "5-arg" framing — the underlying intent matches; the SPEC text was imprecise.

5. **`Bootstrap` and transitively-owned types: current derive convention is `#[derive(Debug, Deserialize)]` or `#[derive(Debug, Deserialize, PartialEq)]` (some with `Default`); NO `Serialize` derives anywhere in `crates/envoy-config/src/bootstrap.rs`.** Verified at `crates/envoy-config/src/bootstrap.rs:8-380+`. The Serialize cascade in Task 4 is mechanical (~25-32 derive-line edits). One known caveat per SPEC §5.3: YAML allows certain casings/syntax that JSON does not — the Task 4 roundtrip sanity check on fixture 0008's `envoy-rust.yaml` catches surprises here. **The cascade also recommends using `&Bootstrap` (borrowed) instead of `Bootstrap` (owned) in `ConfigDumpEntry::Bootstrap` to avoid needing a `Clone` cascade** — the executor picks the cleaner shape.

6. **`AdminEndpoint::from_path` returns `Option<Self>` and takes only `path: &str`** — confirmed at `crates/envoy-admin/src/endpoint.rs:27`. D4 widens to `dispatch(method: &str, path: &str) -> Dispatch`. The existing `handle_inner` GET-only 405-method-allowlist path at `crates/envoy-admin/src/handler.rs:146` calls `render_405()` without a per-endpoint `Allow:` header — D4's `Dispatch::MethodNotAllowed { allow: &'static str }` plumbs the value through; `render_405` is extended to take `allow: &'static str`. The 06.1 `from_path` method is retained alongside the new `dispatch` method (the dispatch refactor adds a new method + a new enum; `from_path` stays reachable for backward compatibility within the crate).

### Architecture-decision lock-ins

Per 08.1 SPEC §6's implementation signposts + §7 ADR posture, 20 decisions are locked at PLAN-write time per the user's standing preference `feedback_pick_recommendation`. The full table is in PLAN.md's "Architecture decisions locked at PLAN-write time" section. Key non-obvious choices:

- **D6 (`/config_dump`) before D5 (`/server_info`)** — D6 leading is recommended per SPEC §6.3 (the `Bootstrap` Serialize cascade is the known mechanical risk surface; landing it first reveals YAML-vs-JSON-roundtrip surprises before they compound).
- **`Bootstrap` Serialize cascade lands as its own dedicated task (Task 4)** before D6 — isolates the mechanical-risk surface.
- **`/server_info.state` is hardcoded `"LIVE"` as a constant** at 08.1; 08.2's D5e patches the value-binding source from the constant to `match drain.current() { ... }`; the struct shape (the field, its type, its position) does NOT change at the boundary.
- **`/server_info.command_line_options` is built once at construction time** as a `BTreeMap<String, serde_yaml::Value>` field on `AdminHandler` (widens the constructor to 6-arg, NOT 5-arg as SPEC §3 D13a phrased it; recording as PLAN-time deviation #1).
- **`ConfigDumpBody` uses borrowed-reference shape** (`bootstrap: &'a envoy_config::Bootstrap`) — avoids needing a `Clone` cascade on `Bootstrap` and its subtypes. Closes the SPEC §5.3 implicit Clone concern.
- **No new ADRs expected.** Ledger head stays ADR-0032. ADR-0033 reserved-available for execution-time landing only if reality forces it (per SPEC §7 conditional foundations grant for `/config_dump` proto-JSON shape — recommended posture is no grant).
- **PROGRESS.md skeleton + Task 1 preamble land alongside PLAN.md at state-2** (07.2 / 06.2 cadence; divergence from 07.1's "PROGRESS created at Task 1" pattern).

### Carryforward inventory engaged in 08.1

Per STATE.md "Phase-06.1 rollovers" + 08.1 SPEC §3 D1/D2/D3:

- **06.1 REVIEW I2** — `serialize_response` case-insensitive header dedupe. Closes at PLAN Task 1.
- **06.1 REVIEW M1** — `resp.reason.unwrap_or("OK")` → `reason_for_status(u16)` helper. Closes at PLAN Task 1. Closes structurally again as a side effect at Task 3 (D4 — every endpoint declares its 405-method-allowlist surface).
- **06.1 REVIEW M4** — `DRAIN_BUDGET` constant consolidation. Closes at PLAN Task 2.

Standing inventory carryforwards inherited but NOT named to 08.1 (carry forward unchanged unless coincidentally engaged):
- **06.3 REVIEW I1** (verification-discipline gap) — structurally closed at 07.1 via two-layer attestation pattern; 08.1 continues per-task PROGRESS test-bucket attestation + state-4 Docker-gated CI anchor.
- **06.3 REVIEW I2** (synthetic 5xx backend + 4-class `pre_requests`) — upstream-robustness family is the named owner; not engaged by 08.1.
- **06.2 REVIEW M1/M2/M4/M5**, **06.1 REVIEW M2/M3/M5/M6**, **05.3 REVIEW I2**, **05.2 REVIEW I1/I2/I3**, **04.1 REVIEW M5/M9/M-claim/M1/M2/M4/M7**, **02.2 REVIEW M1** — all carry forward indefinitely; not engaged by 08.1 unless coincidentally.

### Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 cadence-ratification ADR carries forward unchanged through 08.1 IF no new top-level Cargo deps land (the recommended posture per 08.1 SPEC §7). Task 5 adds `envoy-accesslog` and `envoy-cluster` as path-deps of `envoy-admin` (workspace-internal; not new top-level deps); Task 6 may add `serde_json` to `envoy-admin/Cargo.toml` directly if not transitively reachable — `serde_json` is already on D-3.2's permitted-foundations list (not a foundations grant). `Cargo.lock` diff at the 08.1 reviewed range is expected to be minimal.

### Phase-00 I3 deferral continues

The SIGKILL→SIGTERM `nix` deferral stays in place — 08.1 is a docs-and-endpoint surface; the drain mechanism (08.2) exercises drain via `POST /drain_listeners`, NOT via signal. 08.1's fixture 0014 does NOT need graceful subprocess termination of the harness subject.

---

## Task 2 onward — execution-arc append-only narrative

Each substantive task (Tasks 1-13 in PLAN.md numbering) appends its own `## Task N — <subject>` section here at the task's commit. Task 14 (state-4 verification + STATE-advance) appends the 6-gate evidence anchor.

## Task 1 — D1+D2: serialize_response dedupe + reason_for_status

**Commit:** `d43d97a` — `phase 08.1: task 1 — serialize_response dedupe + reason_for_status (closes 06.1 I2, M1)`
**LoC delta:** +43 production, +112 tests, +7 doc. Net +162.

### Work summary

Added a module-level `reason_for_status(u16) -> &'static str` helper at `crates/envoy-admin/src/handler.rs` covering 200/400/404/405/500/503, and rewired `AdminHandler::serialize_response`'s status-line construction to use it as the fallback when `resp.reason` is `None`. Rewrote the default-header emission block with case-insensitive dedupe (via a `has_header` closure that calls `eq_ignore_ascii_case`) over the 4 standard admin headers (`cache-control`, `x-content-type-options`, `server`, `date`); the always-emitted `connection: close` line is intentionally outside the dedupe set per the 06.1 no-keep-alive posture. Closes 06.1 REVIEW I2 (case-insensitive dedupe) and M1 (reason-phrase helper); appended a Phase 08.1 D1 dedupe note to the BEHAVIOR_CONTRACT.md header allow-list section.

### Tests landed

- `serialize_response_dedupe_and_reason_tests::dedupe_preserves_caller_provided_cache_control`
- `serialize_response_dedupe_and_reason_tests::dedupe_preserves_caller_provided_server`
- `serialize_response_dedupe_and_reason_tests::dedupe_is_case_insensitive`
- `serialize_response_dedupe_and_reason_tests::default_headers_present_when_caller_omits`
- `serialize_response_dedupe_and_reason_tests::reason_503_renders_service_unavailable_without_explicit_reason`
- `serialize_response_dedupe_and_reason_tests::reason_for_status_covers_listed_codes`
- `serialize_response_dedupe_and_reason_tests::explicit_reason_overrides_helper`

7 new tests in `crates/envoy-admin/src/handler.rs` (sibling `#[cfg(test)] mod` after the existing `mod tests`).

### Deviations from PLAN

1. **Test helper signature adapted to `envoy_http1::Response`'s actual shape.** PLAN's snippet typed the test helper as `reason: Option<&str>` and `body: Vec<u8>`, then assembled `Response { reason: reason.map(|s| s.to_string()), body, .. }`. The actual `Response` at HEAD `7dbd984` defines `reason: Option<&'static str>` and `body: bytes::Bytes` (see `crates/envoy-http1/src/response.rs:15-18`). Adapted: helper takes `reason: Option<&'static str>` (every call site passes a `'static` literal so this is sound) and `body: Vec<u8>`, then constructs the `Response` with `reason` passed through verbatim and `body: bytes::Bytes::from(body)`. The 7 test assertions are unchanged — only the helper plumbing adapts to disk reality (per PLAN's "helper shapes are tooling-only" guidance).
2. **`unwrap_or_else` not `as_deref().unwrap_or_else`.** Because `Response.reason` is `Option<&'static str>` (not `Option<String>`), `as_deref()` is unnecessary; the fallback is written as `resp.reason.unwrap_or_else(|| reason_for_status(resp.status))`.
3. **Connection-header note added.** The PLAN's snippet describes "4 standard defaults"; the existing 06.1 code emits 5 headers including `connection: close`. The dedupe applies to the 4 named in the PLAN; `connection: close` continues to emit unconditionally with a clarifying comment ("06.1 has no keep-alive; not in the D1 dedupe set"). Matches the PLAN-time guidance to "keep emitting it (do not remove)".

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
   Compiling envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
   Compiling envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 21.83s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
    Checking envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.82s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
test handler::serialize_response_dedupe_and_reason_tests::default_headers_present_when_caller_omits ... ok
test handler::serialize_response_dedupe_and_reason_tests::explicit_reason_overrides_helper ... ok
test handler::serialize_response_dedupe_and_reason_tests::reason_503_renders_service_unavailable_without_explicit_reason ... ok
test handler::serialize_response_dedupe_and_reason_tests::reason_for_status_covers_listed_codes ... ok
test handler::tests::handler_response_carries_server_header ... ok
test handler::tests::handler_returns_404_for_unknown_path ... ok
test handler::tests::handler_returns_405_for_post_method ... ok
test handler::tests::handler_serves_ready_in_process ... ok
test handler::tests::handler_serves_stats_prometheus_in_process ... ok
test handler::tests::handler_response_carries_admin_headers ... ok
test handler::tests::admin_handler_idle_read_times_out_at_5s ... ok

test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s
```

(envoy-admin tail above: 27 = 20 pre-existing + 7 new. Full `cargo test --workspace` is green across all crates; the workspace-wide aggregate ends with several `Doc-tests` blocks reporting `0 passed`, which is the no-doctests-in-this-crate norm. No `FAILED` lines on a clean run.)

`cargo deny check`:
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues introduced by 08.1 Task 1, which adds no new top-level deps.)

---

## Task 2 — D3: DRAIN_BUDGET module-level hoist

**Commit:** `c6368f4` — `phase 08.1: task 2 — DRAIN_BUDGET module-level hoist (closes 06.1 M4)`
**LoC delta:** +6 production, +20 tests, 0 doc. Net +26.

### Work summary

Hoisted the duplicated `DRAIN_BUDGET` constant. Introduced a single `pub const DRAIN_BUDGET: Duration = Duration::from_secs(5);` at module level in `crates/envoy-listener/src/lib.rs` (was local-fn-scoped inside `Listener::serve`); deleted the parallel module-level declaration in `crates/envoy-admin/src/handler.rs` and replaced it with an import via the existing `envoy-listener` crate dep. Three downstream use sites in `envoy-listener` and two in `envoy-admin` need no change — the identifier still resolves. Closes 06.1 REVIEW M4.

### Tests landed

- `envoy_listener::drain_budget_constant_tests::drain_budget_is_pub_const_at_module_level`
- `envoy_listener::drain_budget_constant_tests::drain_budget_value_is_5_seconds`
- `envoy_admin::handler::drain_budget_lockstep_tests::admin_uses_listener_drain_budget`

3 new tests across the two crates. Test modules placed as **sibling** `#[cfg(test)] mod` blocks at file end, matching the Task 1 placement choice (per Task 1 narrative's deviation discipline).

### Deviations from PLAN

1. **PLAN said handler.rs line 28; disk reality was line 42 post-Task-1.** The `const DRAIN_BUDGET` and its doc comment shifted down after Task 1 inserted `reason_for_status` and `MAX_REQUEST_HEAD`. The const declaration matched verbatim; only the line number changed. Deleted correctly.
2. **Used unqualified `Duration` (not `std::time::Duration`) in the hoisted pub const.** `use std::time::Duration;` is already at module level in `crates/envoy-listener/src/lib.rs` (line 13); using the unqualified form matches the file's prevailing style (all other `Duration` references in the file are unqualified). The PLAN noted this as a "pick whichever is more idiomatic" choice.
3. **Sibling vs. nested test-module placement.** Both test modules are placed as standalone sibling `#[cfg(test)] mod` blocks at file end (not nested inside the existing `#[cfg(test)] mod tests`), matching the Task 1 placement discipline recorded in that task's deviation narrative.

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
   Compiling envoy-listener v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-listener)
   Compiling envoy-http1 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http1)
   Compiling envoy-tcp v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-tcp)
   Compiling envoy-http2 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http2)
   Compiling envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
   Compiling http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
   Compiling envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
   Compiling http2-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 22.50s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking envoy-listener v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-listener)
    Checking envoy-http1 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http1)
    Checking envoy-tcp v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-tcp)
    Checking envoy-http2 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http2)
    Checking envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
    Checking http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
    Checking envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Checking http2-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.48s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
test handler::drain_budget_lockstep_tests::admin_uses_listener_drain_budget ... ok
test handler::serialize_response_dedupe_and_reason_tests::default_headers_present_when_caller_omits ... ok
test handler::serialize_response_dedupe_and_reason_tests::dedupe_is_case_insensitive ... ok
test handler::serialize_response_dedupe_and_reason_tests::dedupe_preserves_caller_provided_server ... ok
test handler::serialize_response_dedupe_and_reason_tests::dedupe_preserves_caller_provided_cache_control ... ok
test handler::serialize_response_dedupe_and_reason_tests::explicit_reason_overrides_helper ... ok
test handler::serialize_response_dedupe_and_reason_tests::reason_503_renders_service_unavailable_without_explicit_reason ... ok
test handler::serialize_response_dedupe_and_reason_tests::reason_for_status_covers_listed_codes ... ok
test handler::tests::handler_serves_ready_in_process ... ok
test handler::tests::handler_response_carries_server_header ... ok
test handler::tests::handler_returns_404_for_unknown_path ... ok
test handler::tests::handler_response_carries_admin_headers ... ok
test handler::tests::handler_serves_stats_prometheus_in_process ... ok
test handler::tests::handler_returns_405_for_post_method ... ok
test handler::tests::admin_handler_idle_read_times_out_at_5s ... ok
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s
test drain_budget_constant_tests::drain_budget_is_pub_const_at_module_level ... ok
test drain_budget_constant_tests::drain_budget_value_is_5_seconds ... ok
test tests::serves_aborts_stragglers_past_drain_budget ... ok
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.06s
```

(envoy-admin tail: 28 = 27 pre-existing + 1 new. envoy-listener tail: 12 = 10 pre-existing + 2 new. Full `cargo test --workspace` is green across all crates.)

`cargo deny check`:
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues introduced by 08.1 Task 2, which adds no new top-level deps.)

---

## Task 3 — D4: Dispatch enum + AdminEndpoint::dispatch refactor

**Commit:** `7188e7f` — `phase 08.1: task 3 — Dispatch enum + AdminEndpoint::dispatch refactor`
**LoC delta:** +45 production, +89 tests, 0 doc. Net +129 insertions, 17 deletions (per `git diff --stat`).

### Work summary

Introduced the `pub enum Dispatch { Endpoint(AdminEndpoint), NotFound, MethodNotAllowed { allow: &'static str } }` enum and two new `AdminEndpoint` methods (`allowed_method(&self) -> &'static str`, `dispatch(method: &str, path: &str) -> Dispatch`) in `crates/envoy-admin/src/endpoint.rs`. Widened `render_405()` to take an `allow: &'static str` parameter and emit a dynamic body (`"Method not allowed. Allow: {allow}\n"`) plus a per-call `Allow:` header value; the body shape change is PLAN-permitted (Step 4 note). Migrated `AdminHandler::handle_inner` at `crates/envoy-admin/src/handler.rs` from a hand-rolled `if method != "GET" { render_405() } else { match from_path(...) }` shape to a single `match AdminEndpoint::dispatch(&method, &path) { ... }` covering all three arms. Closes 06.1 REVIEW M1 structurally: every endpoint variant now declares its 405 allow-list surface via `allowed_method`; 08.2's POST endpoints plug in additively without touching `Dispatch`.

### Tests landed

- `endpoint::dispatch_tests::get_known_path_returns_endpoint`
- `endpoint::dispatch_tests::unknown_path_returns_not_found_regardless_of_method`
- `endpoint::dispatch_tests::known_path_wrong_method_returns_method_not_allowed_with_get_in_allow`
- `endpoint::dispatch_tests::method_match_is_case_sensitive_exact`
- `endpoint::dispatch_tests::each_endpoint_declares_its_allowed_method`
- `endpoint::dispatch_tests::dispatch_is_disjoint_from_from_path`

6 new tests in `crates/envoy-admin/src/endpoint.rs` (sibling `#[cfg(test)] mod dispatch_tests` after the existing `mod tests`). Also updated 1 pre-existing test in-place: `endpoint::tests::render_405_carries_allow_get_header` now calls `render_405("GET")` to match the widened signature.

### Deviations from PLAN

1. **Sibling vs. nested test-module placement.** PLAN Step 1 snippet says "Add this module inside `#[cfg(test)] mod tests`". Placed `dispatch_tests` as a **sibling** `#[cfg(test)] mod` block at file end instead, matching the Task 1+2 placement discipline already recorded in those tasks' deviation narratives. Behavior-equivalent; the `use super::{AdminEndpoint, Dispatch};` line at module head resolves identically from the sibling site.
2. **`render_405` body shape — adopted PLAN's new dynamic shape.** PLAN Step 4 was explicit that the executor adapts body shape; the previous body (`"admin endpoints are GET-only\n"`, `content-type: text/plain`, `content-length`, `allow: GET` static) becomes dynamic (`"Method not allowed. Allow: {allow}\n"`, `content-type: text/plain`, `content-length` matching `body.len()`, `allow: {allow}`). Kept `content-length` to match `render_404`'s sibling pattern (PLAN-permitted; cleaner with the file's prevailing style than dropping it). Set `reason: None` to let Task 1's `reason_for_status` supply the canonical `"Method Not Allowed"`.
3. **PLAN line-number references drift.** PLAN said `pub enum AdminEndpoint` "lines 8-22"; disk reality at HEAD `5904cf9` is lines 7-22. PLAN said `handle_inner` "around line 139"; disk reality is line 169 (Tasks 1+2 shifted offsets). No behavior change.
4. **`render_405` body string adapted to `Bytes::from(format!(...))`.** PLAN's snippet showed `body: format!(...).into_bytes()` returning `Vec<u8>`. The actual `envoy_http1::Response.body` is `bytes::Bytes` (verified at `crates/envoy-http1/src/response.rs:18`); used `Bytes::from(format!(...))` which is the idiomatic conversion path. Same wire output.

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
   Compiling envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
   Compiling http2-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http2-echo-server)
   Compiling http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
   Compiling envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.84s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking envoy-listener v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-listener)
    Checking envoy-http1 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http1)
    Checking envoy-tcp v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-tcp)
    Checking envoy-http2 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http2)
    Checking envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
    Checking http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
    Checking http2-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Checking envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 18.21s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
test endpoint::dispatch_tests::each_endpoint_declares_its_allowed_method ... ok
test endpoint::dispatch_tests::dispatch_is_disjoint_from_from_path ... ok
test endpoint::dispatch_tests::get_known_path_returns_endpoint ... ok
test endpoint::dispatch_tests::known_path_wrong_method_returns_method_not_allowed_with_get_in_allow ... ok
test endpoint::dispatch_tests::method_match_is_case_sensitive_exact ... ok
test endpoint::dispatch_tests::unknown_path_returns_not_found_regardless_of_method ... ok
test endpoint::tests::render_405_carries_allow_get_header ... ok
test handler::drain_budget_lockstep_tests::admin_uses_listener_drain_budget ... ok
test handler::tests::handler_returns_405_for_post_method ... ok
test handler::tests::handler_returns_404_for_unknown_path ... ok
test handler::tests::handler_serves_ready_in_process ... ok
test handler::tests::handler_serves_stats_prometheus_in_process ... ok
test handler::tests::admin_handler_idle_read_times_out_at_5s ... ok
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s
```

(envoy-admin tail: 34 = 28 pre-existing + 6 new dispatch tests. Full `cargo test --workspace` green across all crates; clean run produces no `FAILED` lines. Two transient port-binding flakes were observed in `differential::backend::tests` on parallel-load passes — `tcp_proxy_backend_spawns_and_echoes` / `tcp_proxy_backend_drop_terminates_child` on pass 1, `http1_echo_backend_spawns_and_echoes` on pass 2 — all rerun cleanly in isolation; pre-existing harness race, unrelated to dispatch refactor. Carry-forward; not engaged by Task 3.)

`cargo deny check`:
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues introduced by 08.1 Task 3, which adds no new top-level deps.)

---

## Task 4 — Bootstrap Serialize derive cascade + roundtrip sanity check

**Commit:** `ec12450` — `phase 08.1: task 4 — Bootstrap Serialize derive cascade + roundtrip sanity check`
**LoC delta:** +158 production (cascade + hand-rolled Serialize impls + Cargo.toml), +44 tests, 0 doc. Net +212 insertions, 54 deletions (per `git diff --stat`).

### Work summary

Added `Serialize` to every `#[derive(...)]` line reachable from `Bootstrap` in `crates/envoy-config/src/bootstrap.rs` (49 derive-line edits) and widened the `use serde::Deserialize;` import to `use serde::{Deserialize, Serialize};`. Added `serde_json = "1"` to `crates/envoy-config/Cargo.toml` (required for the roundtrip test and for Task 6's `/config_dump` endpoint). Added five hand-rolled `impl serde::Serialize` blocks for types that have hand-rolled `Deserialize` impls and no `Deserialize` derive: `Route`, `RouteAction`, `SafeRegex`, `StringMatcher`, and `HeaderMatcher`. Added a sibling `#[cfg(test)] mod serialize_roundtrip_tests` block with two tests: a fixture-0008 roundtrip sanity check and a minimal-bootstrap smoke test. Post-review amend corrected conditional bool emission in `StringMatcher::serialize` and `HeaderMatcher::serialize` to emit `ignore_case` and `invert_match` unconditionally (PLAN lock-in #8); also renamed the roundtrip test and added a template-values comment (two cosmetic Minor findings absorbed).

### Tests landed

- `bootstrap::serialize_roundtrip_tests::fixture_0008_bootstrap_roundtrips_yaml_to_json`
- `bootstrap::serialize_roundtrip_tests::minimal_bootstrap_serializes_to_json`

2 new tests in `crates/envoy-config/src/bootstrap.rs` (sibling `#[cfg(test)] mod serialize_roundtrip_tests` at file end, matching Tasks 1/2/3 placement cadence).

### Deviations from PLAN

1. **Full derive-line inventory (auditable record of the mechanical cascade).** The grep produced 49 derive lines with `Deserialize` (not 25-32 as estimated in the PLAN — the file grew considerably since the PLAN estimate was written). Each was transformed by inserting `Serialize` immediately before `Deserialize`. Before→after for each line number (post-edit):

   | Line | Before | After |
   |------|--------|-------|
   | 8 | `#[derive(Debug, Deserialize)]` | `#[derive(Debug, Serialize, Deserialize)]` |
   | 25 | `#[derive(Debug, Deserialize)]` | `#[derive(Debug, Serialize, Deserialize)]` |
   | 31 | `#[derive(Debug, Deserialize)]` | `#[derive(Debug, Serialize, Deserialize)]` |
   | 45 | `#[derive(Debug, Default, Deserialize)]` | `#[derive(Debug, Default, Serialize, Deserialize)]` |
   | 54 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 85 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 108 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 116 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 122 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 134 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 143 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 156 | `#[derive(Debug, Deserialize, PartialEq, Default)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]` |
   | 168 | `#[derive(Debug, Default, Deserialize, PartialEq)]` | `#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]` |
   | 172 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 178 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 184 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 190 | `#[derive(Debug, Deserialize)]` | `#[derive(Debug, Serialize, Deserialize)]` |
   | 208 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 214 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 221 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 235 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 245 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 253 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 273 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 286 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 298 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 304 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 314 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 323 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 336 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 342 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 352 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 361 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 368 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 382 | `#[derive(Debug, Clone, Deserialize, PartialEq)]` | `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]` |
   | 396 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 426 | `#[derive(Debug, Deserialize, PartialEq, Clone, Copy)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]` |
   | 435 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 442 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 456 | `#[derive(Debug, Deserialize, PartialEq, Default)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]` |
   | 463 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 471 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 483 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 490 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 498 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 510 | `#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]` | `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]` |
   | 527 | `#[derive(Debug, Default, Deserialize, PartialEq, Clone)]` | `#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]` |
   | 551 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 558 | `#[derive(Debug, Deserialize, PartialEq)]` | `#[derive(Debug, Serialize, Deserialize, PartialEq)]` |
   | 594 | `#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]` | `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]` |

2. **Five types required hand-rolled `impl serde::Serialize` blocks (not in PLAN).** The PLAN's cascade description assumed all types used `#[derive(Deserialize)]`. In reality, five types use hand-rolled `Deserialize` impls to handle field-name oneof discrimination, which means they also needed hand-rolled `Serialize` impls. These are:
   - `SafeRegex` — serializes only `regex: String`; `compiled: Option<Arc<regex::Regex>>` is skip-serialized (the compiled form is transient; `regex` is the sole serializable field).
   - `StringMatcher` — serializes the mode key (`exact`/`prefix`/`suffix`/`safe_regex`/`contains`) and `ignore_case` (always emitted per lock-in #8; see deviation 8 below).
   - `Route` — serializes `match` + either `direct_response` or `route` depending on the `RouteAction` variant.
   - `RouteAction` — serializes as a single-key map matching the `Deserialize` field-name oneof shape.
   - `HeaderMatcher` — serializes `name`, the mode key, and `invert_match` (always emitted per lock-in #8; see deviation 8 below).

3. **Fixture 0008 uses `{{PORT}}`, `{{BACKEND_HOST}}`, and `{{HTTP1_BACKEND_PORT}}` template variables.** The PLAN said "parse via serde_yaml" on the fixture file, but all `envoy-rust.yaml` fixtures use template placeholders that serde_yaml interprets as YAML mappings (not scalars), causing parse failures. The test substitutes `{{PORT}}` → `10000`, `{{BACKEND_HOST}}` → `127.0.0.1`, and `{{HTTP1_BACKEND_PORT}}` → `10001` before passing to serde_yaml. This is not a serde roundtrip issue per SPEC §5.3 — it's a test setup concern. The substitution matches how the existing integration harness resolves templates.

4. **Sibling vs. nested test-module placement.** Placed `serialize_roundtrip_tests` as a standalone sibling `#[cfg(test)] mod` block at file end, consistent with Tasks 1/2/3 cadence.

5. **`serde_json` added to `crates/envoy-config/Cargo.toml`.** The PLAN noted it should be added if not present (it was not present). Added as `serde_json = "1"` matching the workspace's existing serde_json 1.0.149 in `Cargo.lock`.

6. **PLAN derive-line count estimate off.** PLAN estimated ~25-32 derive lines; actual count is 49. The file grew considerably from Task 1–3 landings and prior phases. No behavior impact.

7. **clippy `doc_lazy_continuation` lint on test doc comment.** Initial doc comment used `+ http_filters + multi-route` phrasing which clippy's `doc_lazy_continuation` lint misread as a list continuation item without indentation. Rewrote the comment to use em-dash and parenthetical grouping to avoid the lint.

8. **Post-review amend: conditional bool emission fixed (Important finding, PLAN lock-in #8).** A code-quality review of the substantive commit caught that `StringMatcher::serialize` and `HeaderMatcher::serialize` emitted `ignore_case` and `invert_match` conditionally (only when `true`), citing a `// matches serde's #[serde(skip_serializing_if = "is_false")] convention` comment. This violated PLAN lock-in #8 ("Serialize emits the literal value, default or not"). The substantive commit was amended to emit both fields unconditionally — fixed-count map lengths (`Some(2)` for `StringMatcher`, `Some(3)` for `HeaderMatcher`), `map.serialize_entry(field, &self.field)?` without guards, and the `skip_serializing_if`-citing comments removed. The roundtrip test remains green because the struct-equality assertion is unaffected by JSON verbosity. Two Minor findings were also absorbed cosmetically: the test `fixture_0008_bootstrap_roundtrips_yaml_to_json_to_yaml` was renamed to `fixture_0008_bootstrap_roundtrips_yaml_to_json` (the `_to_yaml` suffix was misleading — the test goes YAML→struct→JSON→struct→JSON and asserts JSON equality), and a one-line comment was added above the `.replace()` calls noting that template substitution values are arbitrary. All 5 gates re-run and green post-amend.

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
   Compiling envoy-config v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-config)
   Compiling envoy-listener v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-listener)
   Compiling envoy-cluster v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-cluster)
   Compiling envoy-filter v0.1.0 (/Users/esa/git/envoy-rust/crates/envoy-filter)
   Compiling envoy-tls v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-tls)
   Compiling envoy-tcp v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-tcp)
   Compiling envoy-http1 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http1)
   Compiling envoy-http2 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http2)
   Compiling envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
   Compiling http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
   Compiling envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
   Compiling http2-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 24.92s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking envoy-config v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-config)
    Checking envoy-cluster v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-cluster)
    Checking envoy-listener v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-listener)
    Checking envoy-filter v0.1.0 (/Users/esa/git/envoy-rust/crates/envoy-filter)
    Checking envoy-tls v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-tls)
    Checking envoy-http1 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http1)
    Checking envoy-tcp v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-tcp)
    Checking envoy-http2 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http2)
    Checking envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
    Checking http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
    Checking envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Checking http2-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 23.40s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
test bootstrap::serialize_roundtrip_tests::fixture_0008_bootstrap_roundtrips_yaml_to_json ... ok
test bootstrap::serialize_roundtrip_tests::minimal_bootstrap_serializes_to_json ... ok
test bootstrap::tests::fuzz_corpus_seeds_parse_or_reject_cleanly ... ok

test result: ok. 209 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

(envoy-config tail: 209 = 207 pre-existing + 2 new. Full `cargo test --workspace` green across all crates; no `FAILED` lines on a clean run. The pre-existing `differential` port-binding transient flakes may appear on busy systems; they rerun cleanly in isolation and are unrelated to Task 4 — same pattern noted in Task 3 deviation narrative.)

`cargo deny check`:
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues. `serde_json` is already a transitive dep with MIT license — adding it as a direct dep to `envoy-config` adds no new license categories.)

---

## Task 5 — D13a: AdminHandler::new widening + envoy-bin wiring + format_iso8601 wrapper

**Commit:** `1a05eaa` — `phase 08.1: task 5 — AdminHandler::new widen + envoy-bin wiring + format_iso8601 pub wrapper`
**LoC delta:** +189 production+test handler.rs, +35 production+test accesslog/lib.rs, +1 envoy-admin/Cargo.toml dep block (3 new entries), +1 envoy-bin/Cargo.toml dep, +30 envoy-bin/src/main.rs, +4 Cargo.lock. Net +276 insertions, 10 deletions (per `git diff --stat`). Of the handler.rs delta: ~64 lines production (struct field cascade + widened constructor + new imports + 6-arg call-site reshape in 7 tests), ~85 lines new test block (`admin_handler_new_6arg_tests`), ~40 lines test-helper functions (`dummy_bootstrap`/`dummy_cluster_manager` in the `tests` module). Of the accesslog/lib.rs delta: ~15 lines production (`pub fn format_iso8601` + doc), ~20 lines new test block (`public_format_iso8601_tests`).

### Work summary

Widened `AdminHandler::new` at `crates/envoy-admin/src/handler.rs` from the 2-arg `(config, registry)` shape to a 6-arg shape: added four new struct fields (`bootstrap: Arc<Bootstrap>`, `cluster_manager: Arc<ClusterManager>`, `start_instant: Instant`, `command_line_options: BTreeMap<String, serde_yaml::Value>`) carrying the handles Tasks 6–9 (`/server_info`, `/config_dump`, `/clusters`, `/stats` JSON) need at render time. Added `envoy-accesslog`, `envoy-cluster`, and `serde_yaml = "0.9"` to `crates/envoy-admin/Cargo.toml` per PLAN-write SPEC correction 2 (visibility promotion + dep) and lock-in #7 (`command_line_options` built once at construction). Added `pub fn format_iso8601(t: SystemTime) -> String` at `crates/envoy-accesslog/src/lib.rs` as an allocating wrapper around the internal `pub(crate) default_format::format_iso8601` `&mut String` writer. Threaded `Arc<Bootstrap>`, `Arc<ClusterManager>`, `Instant::now()`, and a `BTreeMap` populated with `{"config_path": Value::String(<-c value>)}` through `crates/envoy-bin/src/main.rs` (the production call site lives in `main.rs`, not `admin.rs` — there is no `crates/envoy-bin/src/admin.rs`; recorded as deviation #3). Updated the 7 in-file `AdminHandler::new(...)` test call sites in `handler.rs` to use new `dummy_bootstrap()` / `dummy_cluster_manager()` test helpers + `Instant::now()` + `BTreeMap::new()` per the PLAN Step 6 pattern.

### Tests landed

- `public_format_iso8601_tests::epoch_zero_renders_canonical_shape` (production crate `envoy-accesslog`)
- `public_format_iso8601_tests::known_date_renders_correctly` (production crate `envoy-accesslog`)
- `handler::admin_handler_new_6arg_tests::admin_handler_new_accepts_six_args_and_constructs` (production crate `envoy-admin`)

3 new tests total. `envoy-accesslog` test bucket: 16 passed (was 14 pre-task; +2 new). `envoy-admin` test bucket: 35 passed (was 34 pre-task; +1 new). All existing tests continue to pass.

TDD discipline: tests written first, watched fail with the expected error shapes (`E0432 cannot find function format_iso8601 in crate envoy_accesslog`; `E0061 unexpected argument #5/#6` + `E0433 cannot find module serde_yaml`), then implementation added; re-ran tests to confirm green.

### Deviations from PLAN

1. **PLAN line-number drift (carry-forward from Tasks 2–4).** The PLAN listed the 7 in-file `AdminHandler::new(...)` test call sites at lines 291, 318, 341, 363, 386, 416, 461 in handler.rs. Post-Task 4, the actual line numbers were 362, 389, 412, 434, 457, 487, 532 — a ~71-line forward drift caused by Task 1's `serialize_response_dedupe_and_reason_tests` (~110 lines) and Task 4's untouched-here `Serialize` cascade (no handler.rs edits, but shared crate context). Re-anchored against `grep -n "AdminHandler::new("`; declaration matches verbatim, only line offsets shifted. No behavioral impact.

2. **PLAN-write SPEC correction lock-in: `_5arg_tests` → `_6arg_tests` test-module rename.** The PLAN's Step 1 listed `admin_handler_new_5arg_tests`, but PLAN lock-in #7 + PLAN-write SPEC correction #4 require the constructor be 6-arg (not 5). The test module name was promoted to `admin_handler_new_6arg_tests` to match the actual arity, and the inner test `admin_handler_new_accepts_six_args_and_constructs` was named in plural form to match. This is a cosmetic naming alignment, not a scope change.

3. **Production call site lives in `main.rs`, not a separate `admin.rs`.** The PLAN listed both `crates/envoy-bin/src/admin.rs` (twice) and `crates/envoy-bin/src/main.rs` as possible production sites. The current disk state has no `crates/envoy-bin/src/admin.rs` — admin construction is inlined in `main.rs::run` at the post-listener-walk block (the PLAN even noted: "or `main.rs` — verify against disk"). All envoy-bin edits land in `main.rs`. The historical `admin.rs` was the pre-08.1-Task-1 location of `AdminHandler` and `MAX_REQUEST_HEAD`, both since moved to `envoy-admin`.

4. **`bootstrap` widened from `Bootstrap` to `Arc<Bootstrap>` in `main.rs::run`.** The PLAN said "construct `Arc<Bootstrap>` from the parsed config". I implemented this as `let bootstrap = Arc::new(envoy_config::parse_bootstrap(&yaml)?);` (changing the local from `Bootstrap` to `Arc<Bootstrap>` once at parse time, then cloning into the admin handler) rather than constructing a separate `Arc::new(bootstrap.clone())` at the admin call site. The Arc wrap is cleaner because (a) all subsequent field accesses go through Deref auto-coercion (verified compile-clean), (b) the production code already treats `bootstrap` as a read-only borrow target, (c) it avoids an extra clone of the (potentially large) bootstrap struct. The one call-site that takes a `&Bootstrap` parameter (`envoy_cluster::from_bootstrap`) coerces `&Arc<Bootstrap>` → `&Bootstrap` via Arc's `Deref` impl in function-arg position. Documented inline.

5. **Sibling test-module placement.** `admin_handler_new_6arg_tests` and `public_format_iso8601_tests` were placed as top-level sibling `#[cfg(test)] mod` blocks at the end of `handler.rs` and `lib.rs` respectively, NOT nested inside the existing `tests` modules. Consistent with Tasks 1/2/3/4 cadence. Recorded per the Task 1 review's standing reminder ("record this in Deviations even if unsurprising").

6. **Four `#[allow(dead_code)]` annotations on the new `AdminHandler` fields.** `bootstrap`, `cluster_manager`, `start_instant`, and `command_line_options` are wired in Task 5 but only read by consumers landing in Tasks 6/7/8/9. Without `#[allow(dead_code)]`, clippy's default-on `dead_code` warning (escalated to deny by `-D warnings`) would block the build. Each annotation carries an inline comment naming the consumer task. The annotations come off naturally as each consumer task lands (an accessor is added, the field is read).

7. **`envoy-bin/Cargo.toml` also gained `serde_yaml = "0.9"`** because `main.rs` now constructs `serde_yaml::Value::String(...)` directly to populate `command_line_options`. The PLAN listed `envoy-admin/Cargo.toml`'s dep additions but did not call out the envoy-bin dep; it falls out of the requirement to construct the map at the call site rather than inside `AdminHandler::new`. `serde_yaml` is already on the D-3.2 permitted-foundations list (used by `envoy-config` and `envoy-listener` at the same `0.9` pin); no new license category.

8. **`fmt --check` initially flagged one long-line in the new helper.** The `dummy_bootstrap()` YAML literal exceeded the 100-column width on the `let yaml = "..."` line; `cargo fmt --all` split it across two lines (assignment on one, string on the next). Mechanical reformatting; recorded for completeness.

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.10s
```

(Clean incremental rebuild after all gates; the initial full-rebuild post-Cargo.toml edits compiled all four crates touched — `envoy-accesslog`, `envoy-admin`, `envoy-bin`, plus the transitive `envoy-http1`/`envoy-http2`/`http1-echo-server`/`http2-echo-server` — in ~27s on a cold target.)

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
    Checking envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.85s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
test handler::admin_handler_new_6arg_tests::admin_handler_new_accepts_six_args_and_constructs ... ok
test public_format_iso8601_tests::epoch_zero_renders_canonical_shape ... ok
test public_format_iso8601_tests::known_date_renders_correctly ... ok

test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

(envoy-admin tail: 35 = 34 pre-existing + 1 new. envoy-accesslog tail: 16 = 14 pre-existing + 2 new. Full `cargo test --workspace` green; no `FAILED` lines across any of the workspace's ~55 binary/test buckets. Same pre-existing `differential` port-binding transient flake posture as Tasks 3/4; rerun cleanly in isolation.)

`cargo deny check`:
```
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues. `serde_yaml` and `envoy-accesslog` were already transitive deps with Apache-2.0 / MIT licenses; promoting them to direct deps of `envoy-admin` adds no new license categories. The `envoy-bin/Cargo.toml`'s new `serde_yaml = "0.9"` likewise re-uses an existing transitive.)

---

## Task 6 — D6: /config_dump endpoint + BEHAVIOR_CONTRACT row

**Commit:** `24f8382` — `phase 08.1: task 6 — /config_dump endpoint + BEHAVIOR_CONTRACT row`
**LoC delta:** +204 endpoint.rs (~50 production: variant + render_with + render_config_dump + body types + doc updates; ~120 test for the new `config_dump_tests` module + 1-line update to `each_endpoint_declares_its_allowed_method`), +76/-13 handler.rs (~50 production: 5 new accessors + widened `handle_inner` signature to `(Arc<Self>, stream)` + reshaped `ConnectionHandler::handle` to clone Arc fields + reconstruct `Arc<Self>` mirroring envoy-tcp's pattern, removed `#[allow(dead_code)]` on `bootstrap`), +2 envoy-admin/Cargo.toml (`serde = { version = "1", features = ["derive"] }` + `serde_json = "1"`), +2 Cargo.lock, +20 docs/envoy-rust/BEHAVIOR_CONTRACT.md (new "Admin endpoint body shapes" subsection with `/config_dump` row), +PROGRESS.md narrative. Net +273 insertions, 11 deletions.

### Work summary

Landed `/config_dump` GET endpoint at `crates/envoy-admin/src/endpoint.rs`: added `AdminEndpoint::ConfigDump` variant, extended `from_path` and `allowed_method` (collapsed via `|`-pattern to 4 GET variants), added `pub fn render_with(&self, &AdminHandler)` as the new dispatch method (the existing `render(&StatsRegistry)` carries forward for the 06.1 endpoints via the catch-all `_` arm), and added `render_config_dump` + lifetime-parameterized `ConfigDumpBody<'a>` / `ConfigDumpEntry<'a>` body types per PLAN lock-in #1 (borrowed-reference shape — avoids the `Clone` cascade on `Bootstrap`). At `crates/envoy-admin/src/handler.rs`: added five `pub(crate)` accessors (`bootstrap()`, `registry()`, `cluster_manager()`, `start_instant()`, `command_line_options()`) per PLAN lock-in #2, removed `#[allow(dead_code)]` on the `bootstrap` field (per lock-in #3 — leaves the other three in place for Tasks 7/8/9), widened `handle_inner` from `(Arc<StatsRegistry>, stream)` to `(Arc<Self>, stream)` and rerouted dispatch through `render_with(&handler)`, and reshaped the `ConnectionHandler::handle(&self, ...)` impl to clone the internal Arcs + reconstruct an `Arc<Self>` per accept (mirroring envoy-tcp's existing pattern for the same `&self` → `'static future` lifetime mismatch). Added `serde = { version = "1", features = ["derive"] }` and `serde_json = "1"` to `crates/envoy-admin/Cargo.toml` (both already transitive deps; matches envoy-config's pin per Task 4). Created the new "Admin endpoint body shapes" subsection in `docs/envoy-rust/BEHAVIOR_CONTRACT.md` between "Stat-name mapping" and "Access log field mapping", with the first row covering `/config_dump`.

### Tests landed

- `endpoint::config_dump_tests::config_dump_path_dispatches_on_get` (production crate `envoy-admin`)
- `endpoint::config_dump_tests::config_dump_405_on_post` (production crate `envoy-admin`)
- `endpoint::config_dump_tests::config_dump_renders_200_with_application_json` (production crate `envoy-admin`)
- `endpoint::config_dump_tests::config_dump_body_is_valid_json_with_configs_array` (production crate `envoy-admin`)
- `endpoint::config_dump_tests::config_dump_body_has_bootstrap_config_dump_entry` (production crate `envoy-admin`)
- `endpoint::config_dump_tests::config_dump_bootstrap_subtree_carries_node_id` (production crate `envoy-admin`)

6 new tests total, all in a new sibling `#[cfg(test)] mod config_dump_tests` block at the end of `endpoint.rs` (placed before the existing `dispatch_tests` block). `envoy-admin` test bucket: 41 passed (was 35 pre-task; +6 new). All existing tests stay green. The `each_endpoint_declares_its_allowed_method` test in `dispatch_tests` was extended with a 4th `assert_eq!` for `ConfigDump` (one-line additive change).

TDD discipline: tests written first, watched fail with the expected error shapes (`E0433 cannot find module or crate serde_json`; `cannot find variant ConfigDump`; `no method named render_with`), then implementation added; re-ran tests to confirm 6/6 green.

### Deviations from PLAN

1. **PLAN test stub used out-of-date `Address::SocketAddress(...)` enum-variant shape.** The PLAN's Step 1 test stub constructed `envoy_config::Address::SocketAddress(envoy_config::SocketAddress { protocol: Default::default(), address: "127.0.0.1".to_string(), port_value: 0, })`. On disk, `envoy_config::Address` is a struct (`Address { socket_address: SocketAddress { ... } }`) and `SocketAddress` has no `protocol` field — only `address` and `port_value`. Re-anchored the test helper to use the actual on-disk struct shape (matches the existing `handler.rs::tests::admin_config` helper). No semantic divergence; the `AdminConfig::from_envoy_config` call returns the same value either way. This drift mirrors the project's general pattern of PLAN stubs being written against a hypothesized API and re-anchored at execution time.

2. **`render` arm for `ConfigDump` is `unreachable!()` not a real implementation.** The PLAN suggested `ConfigDump` go through `render_with` exclusively. To keep the existing `render(&StatsRegistry)` API exhaustive (and prevent silently dispatching `ConfigDump` through a state-less path), the new variant's arm in `render` panics with a clear message. The `render_with` catch-all only delegates to `render` for the 06.1 endpoints (`Ready`/`Stats`/`StatsPrometheus`); `ConfigDump` is matched explicitly first. This makes the contract enforceable at runtime with a clear panic message rather than silently producing a wrong response.

3. **`handle_inner` signature widened to `(Arc<Self>, TcpStream)` instead of just changing the dispatch arm.** The PLAN's Step 4 said simply: "Change the dispatch arm to call `render_with(&self)`". The signature change is the structural prerequisite — the existing `handle_inner` only carried `Arc<StatsRegistry>`; calling `render_with(&handler)` requires `&AdminHandler` in scope. Threaded `Arc<Self>` through both `handle_inner` and the `ConnectionHandler::handle` shim. The shim now clones each internal Arc field + the `BTreeMap` (small in 08.1: typically a single `config_path` entry per `envoy-bin/src/main.rs`) and reconstructs a fresh `AdminHandler` wrapped in `Arc::new`, mirroring `envoy-tcp::TcpProxy`'s identical workaround for the trait's `&self` → `'static future` lifetime gap. The pattern is documented inline in both `handle()` and the `handle_inner` doc-comment.

4. **`config_dump_tests` placed BEFORE `dispatch_tests` (sibling-module placement).** Per the standing reminder from Task 1's review, sibling-vs-nested placement is recorded as a Deviation even when unsurprising. The new `config_dump_tests` block is a top-level sibling `#[cfg(test)] mod` placed after the existing `tests` block and before `dispatch_tests`. Three sibling test modules now coexist in `endpoint.rs`: `tests` (06.1 endpoint coverage), `config_dump_tests` (08.1 D6 coverage), `dispatch_tests` (08.1 D4 coverage). Sibling placement is consistent with Tasks 1/2/3/4/5 cadence.

5. **`serde` declared as a direct dep alongside `serde_json` in `envoy-admin/Cargo.toml`.** The PLAN's Step 5 only mentioned `serde_json = "1"`. The new `ConfigDumpBody` / `ConfigDumpEntry` types use `#[derive(Serialize)]`, which requires the `serde` crate be in scope with the `derive` feature — `serde_json` alone does not pull this in transitively for derive purposes. Added `serde = { version = "1", features = ["derive"] }` matching `envoy-config/Cargo.toml`'s pin. Both crates were already transitive deps; Cargo.lock just gains them in envoy-admin's deps list (no new license categories).

6. **PLAN line-number drift carry-forward.** The PLAN referenced "current size ~390 lines" for endpoint.rs and "current size ~835 lines" for handler.rs. On-disk pre-task: endpoint.rs was 393 lines, handler.rs was 836 lines. Re-anchored against the symbols (`from_path`, `allowed_method`, `dispatch`, `handle_inner`, `ConnectionHandler::handle`); declarations match verbatim, only the line offsets shifted ≤1.

7. **Five new `pub(crate)` accessors added on `AdminHandler` (not just `bootstrap()`).** Per PLAN lock-in #2, accessors are the discipline. Added all five up-front (`bootstrap`, `registry`, `cluster_manager`, `start_instant`, `command_line_options`) so Tasks 7/8/9 don't need to revisit `handler.rs` for new accessors. The four not-yet-consumed accessors (everything except `bootstrap` and `registry`) carry `#[allow(dead_code)]` with inline comments naming the consumer task. The `registry()` accessor is the first time the field is exposed via a method (was previously cloned via `Arc::clone(&self.registry)` directly inside `handle()`); the new shape is consistent with the rest.

8. **One `cargo fmt` reformatting pass applied.** The first version of `render_config_dump`'s `headers: vec![( ... )]` literal violated rustfmt's single-line collapse rule for short tuple-vec literals. `cargo fmt --all` collapsed it to a single-line form. Mechanical reformatting; no behavioral impact.

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
   Compiling envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
   Compiling envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.37s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
    Checking envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.76s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
test endpoint::config_dump_tests::config_dump_405_on_post ... ok
test endpoint::config_dump_tests::config_dump_path_dispatches_on_get ... ok
test endpoint::config_dump_tests::config_dump_body_is_valid_json_with_configs_array ... ok
test endpoint::config_dump_tests::config_dump_renders_200_with_application_json ... ok
test endpoint::config_dump_tests::config_dump_bootstrap_subtree_carries_node_id ... ok
test endpoint::config_dump_tests::config_dump_body_has_bootstrap_config_dump_entry ... ok

test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s
```

(envoy-admin tail: 41 = 35 pre-existing + 6 new. Full `cargo test --workspace` green except for the same pre-existing port-binding transient flake in `differential::backend::tests::http1_echo_backend_*` observed at Tasks 3/4/5 — passes cleanly when re-run in isolation: `cargo test -p differential --lib backend::tests` → `10 passed; 0 failed`. No new regressions across the workspace's ~55 binary/test buckets.)

`cargo deny check`:
```
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues. `serde` and `serde_json` were already transitive deps with MIT/Apache-2.0 dual-licenses; promoting them to direct deps of `envoy-admin` adds no new license categories.)

---

## Task 7 — D5: /server_info endpoint + BEHAVIOR_CONTRACT row

**Commit:** `f463a3c` — `phase 08.1: task 7 — /server_info endpoint + BEHAVIOR_CONTRACT row`
**LoC delta:** +187/-3 endpoint.rs (~55 production: `ServerInfo` variant + `/server_info` `from_path` arm + extended `allowed_method` `|`-chain + `render` `unreachable!` arm + `render_with` arm + `ServerInfoBody<'a>` body type + `render_server_info` fn + doc updates; ~120 test for the new `server_info_tests` module + 1-line `each_endpoint_declares_its_allowed_method` extension + 1 visibility bump on `handler_with_bootstrap` to `pub(super)` so Task 7 can reuse it), +7/-11 handler.rs (removed 2 field-level + 2 accessor-level `#[allow(dead_code)]` annotations on `start_instant` and `command_line_options` now that Task 7 consumes them; refreshed `(Task 6)` doc-comment references to `(Task 7)` on those two fields; tightened `cluster_manager`'s field-level comment from `// wired for Tasks 6-9` to `// wired for Task 8`), +1 docs/envoy-rust/BEHAVIOR_CONTRACT.md (new `/server_info` row in the "Admin endpoint body shapes" table), +PROGRESS.md narrative. Net +195 insertions, 14 deletions.

### Work summary

Landed `/server_info` GET endpoint at `crates/envoy-admin/src/endpoint.rs`: added `AdminEndpoint::ServerInfo` variant (5th total), extended `from_path` and `allowed_method` (collapsed `|`-pattern now covers all 5 GET variants), added explicit `AdminEndpoint::ServerInfo => render_server_info(handler)` arm to `render_with` ahead of the catch-all, added an `unreachable!` arm in the registry-only `render` path mirroring the Task 6 `ConfigDump` precedent, and added lifetime-parameterized `ServerInfoBody<'a>` body type + `render_server_info` fn per PLAN lock-in #1 (borrowed-reference shape: `Option<&'a envoy_config::Node>` and `&'a BTreeMap<String, serde_yaml::Value>` — avoids any `Clone` cascade). The `node` field is `Option<&'a envoy_config::Node>` rather than `&'a envoy_config::Node` because `Bootstrap.node` is `Option<Node>` on disk (PLAN stub assumed `Node` directly — this is a re-anchor against disk reality). At `crates/envoy-admin/src/handler.rs`: removed the field-level `#[allow(dead_code)] // wired for Tasks 6-9` annotations on `start_instant` (line 62) and `command_line_options` (line 69) AND the accessor-level `#[allow(dead_code)] // wired for Task 7` on `start_instant()` and `command_line_options()` (per Task 7's consumer obligations); refreshed the `(Task 6)` doc-comment references on those fields to `(Task 7)` (Task 6 code-quality review M1 close); tightened `cluster_manager`'s field-level comment to `// wired for Task 8` (Task 6 review M2 close); the `cluster_manager` annotations themselves are left in place per the PLAN's explicit "Leave both" instruction. Added the new `/server_info` row to BEHAVIOR_CONTRACT.md's "Admin endpoint body shapes" table.

### Tests landed

- `endpoint::server_info_tests::server_info_path_dispatches_on_get` (production crate `envoy-admin`)
- `endpoint::server_info_tests::server_info_405_on_post` (production crate `envoy-admin`)
- `endpoint::server_info_tests::server_info_renders_200_with_application_json` (production crate `envoy-admin`)
- `endpoint::server_info_tests::server_info_body_has_required_keys` (production crate `envoy-admin`)
- `endpoint::server_info_tests::server_info_state_is_live_at_phase_08_1` (production crate `envoy-admin`)
- `endpoint::server_info_tests::server_info_node_subtree_carries_id` (production crate `envoy-admin`)
- `endpoint::server_info_tests::server_info_uptime_is_non_negative` (production crate `envoy-admin`)

7 new tests total, all in a new sibling `#[cfg(test)] mod server_info_tests` block placed AFTER `config_dump_tests` and BEFORE `dispatch_tests` in `endpoint.rs` (maintaining file's increasing-task-number order). `envoy-admin` test bucket: 48 passed (was 41 post-Task-6; +7 new). All existing tests stay green. The `each_endpoint_declares_its_allowed_method` test in `dispatch_tests` was extended with a 5th `assert_eq!` for `ServerInfo` (one-line additive change).

TDD discipline: tests written first, watched fail with the expected error shape (`E0599: no variant or associated item named ServerInfo found for enum AdminEndpoint`), then implementation added; iterated once on a real disk-vs-PLAN drift (`Bootstrap.node` is `Option<Node>`, surfacing `E0308: expected &Node, found &Option<Node>`) which I resolved by widening `ServerInfoBody.node` to `Option<&'a envoy_config::Node>` and switching the renderer to `handler.bootstrap().node.as_ref()`; re-ran tests to confirm 7/7 green.

### Deviations from PLAN

1. **`ServerInfoBody.node` widened to `Option<&'a envoy_config::Node>` (PLAN stub used `&'a envoy_config::Node`).** On-disk, `envoy_config::Bootstrap.node` is `Option<Node>` (see `crates/envoy-config/src/bootstrap.rs:12`) — the PLAN's Step 3 code stub assumed unconditional `Node`. The minimal-impact correction is to carry `Option<&Node>` in the body envelope (serde_json renders `None` → JSON `null`); the renderer wires `node: handler.bootstrap().node.as_ref()`. Test `server_info_node_subtree_carries_id` exercises the present-`node` path (passes); absent-`node` would serialize as `"node": null` which is consistent with the SPEC's "value-exact from the parsed bootstrap" disposition. No struct-shape divergence visible to consumers when `node` is present.

2. **`render` arm for `ServerInfo` is `unreachable!()` mirroring Task 6's `ConfigDump` arm.** PLAN-implicit but not stated: the registry-only `render(&StatsRegistry)` API must stay exhaustive. Adding a parallel `ServerInfo => unreachable!("ServerInfo requires handler-scoped state; dispatch via AdminEndpoint::render_with")` arm preserves the structural invariant that `render_with` is the only legitimate path for state-bearing endpoints. This is the same deviation Task 6 explicitly ratified (Task 6 deviation #2).

3. **`handler_with_bootstrap` visibility promoted from private to `pub(super)`.** PLAN Step 1 stub said `use super::config_dump_tests::handler_with_bootstrap;` — but on disk the helper was `fn handler_with_bootstrap(yaml: &str)` (private). Promoted to `pub(super)` so both `config_dump_tests` and the new `server_info_tests` sibling can share one helper without duplication. Task 8's `/clusters` and Task 9's `/listeners` tests will benefit too. Minimal, justified deviation per the prompt's explicit recommendation.

4. **Field-level + accessor-level `#[allow(dead_code)]` cleanup landed in this commit.** Per the prompt's explicit obligations: removed the field-level `#[allow(dead_code)] // wired for Tasks 6-9` annotations on `start_instant` (line 62) and `command_line_options` (line 69), AND the accessor-level `#[allow(dead_code)] // wired for Task 7` annotations on `start_instant()` and `command_line_options()`. `cluster_manager`'s field and accessor annotations are left in place (Task 8 removes those). All four annotations were `// wired for Tasks 6-9` / `// wired for Task 7` — Task 7 is the consumer.

5. **Doc-comment refresh on `start_instant` + `command_line_options` field-level comments (Task 6 review M1 close).** Changed `(Task 6)` references in the field-level doc comments to `(Task 7)` since Task 7 is the actual consumer that lights up the dead accessors. Optional cleanup per prompt recommendation; landed.

6. **`cluster_manager` field-level comment tightened to `// wired for Task 8` (Task 6 review M2 close).** Changed `// wired for Tasks 6-9` to `// wired for Task 8` on the `cluster_manager` field. Optional cleanup per prompt recommendation; landed.

7. **`server_info_tests` placed AFTER `config_dump_tests` and BEFORE `dispatch_tests` (sibling-module placement).** Per the standing reminder from Task 1's review, sibling-vs-nested placement is recorded as a Deviation even when expected. Four sibling test modules now coexist in `endpoint.rs`: `tests` (06.1 endpoint coverage), `config_dump_tests` (08.1 D6), `server_info_tests` (08.1 D5), `dispatch_tests` (08.1 D4). Sibling placement is consistent with Tasks 1-6 cadence.

8. **`render_server_info` headers literal collapsed by `cargo fmt` (mechanical reformatting).** The first version of the tests module used a multi-line YAML literal; `cargo fmt --all` collapsed the trailing `let yaml = "node:\n..."` literal to its single-line form across several tests. Mechanical reformatting; no behavioral impact. Mirrors the same Task 6 fmt-pass deviation.

9. **PLAN body-stub omitted `Bytes::from(body_bytes)` wrap on the response.** The PLAN's Step 3 stub wrote `body: body_bytes` (raw `Vec<u8>`). On disk, `envoy_http1::Response.body` is `bytes::Bytes` (see `render_config_dump` at endpoint.rs:232). Wrapped with `Bytes::from(body_bytes)` to match the existing precedent. Explicitly anticipated by the prompt.

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
   Compiling envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
   Compiling envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.51s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
    Checking envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.50s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
test endpoint::server_info_tests::server_info_path_dispatches_on_get ... ok
test endpoint::server_info_tests::server_info_405_on_post ... ok
test endpoint::server_info_tests::server_info_node_subtree_carries_id ... ok
test endpoint::server_info_tests::server_info_uptime_is_non_negative ... ok
test endpoint::server_info_tests::server_info_renders_200_with_application_json ... ok
test endpoint::server_info_tests::server_info_state_is_live_at_phase_08_1 ... ok
test endpoint::server_info_tests::server_info_body_has_required_keys ... ok

test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s
```

(envoy-admin tail: 48 = 41 pre-task + 7 new. Full `cargo test --workspace` green across all buckets — no port-binding flakes resurfaced this run; the differential `http1_echo_backend_*` flake noted at Tasks 3/4/5/6 did NOT recur.)

`cargo deny check`:
```
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues. Task 7 introduces no new direct deps — `serde`, `serde_json`, and `serde_yaml` are already direct deps of `envoy-admin` from Tasks 4/6.)

---

## Task 8 — D7: /clusters endpoint + ClusterManager::clusters() accessor + BEHAVIOR_CONTRACT row

**Commit:** `1a9a3ed` — `phase 08.1: task 8 — /clusters endpoint + ClusterManager::clusters() + BEHAVIOR_CONTRACT row`
**LoC delta:** +26/-0 crates/envoy-cluster/src/cluster.rs (~10 production: `ClusterManager::clusters()` accessor returning `impl Iterator<Item = ClusterHandle> + '_` with deterministic by-name sort over the internal `HashMap`; ~7 test for the new `clusters_accessor_tests` sibling module). +123/-3 crates/envoy-admin/src/endpoint.rs (~65 production: `Clusters` variant on `AdminEndpoint` + `/clusters` `from_path` arm + extended `allowed_method` `|`-chain to include `Clusters` + `render` `unreachable!` arm + `render_with` explicit `Clusters` arm + `render_clusters` free fn emitting two lines per cluster in `<name>::observability_name::<name>` + `<name>::default_priority::endpoints` shape + doc updates; ~58 test for the new `clusters_tests` sibling module + 1-line `each_endpoint_declares_its_allowed_method` extension + 6-endpoint expansion of `get_known_path_returns_endpoint` covering all dispatchable endpoints; plus 1 small adjustment to the legacy `from_path_unknown_returns_none` test which previously asserted `/clusters` → `None` and was updated to use `/listeners` as the still-unknown probe). +4/-4 crates/envoy-admin/src/handler.rs (removed the field-level + accessor-level `#[allow(dead_code)] // wired for Task 8` annotations on `cluster_manager` — the last `#[allow(dead_code)]` pair on the AdminHandler accessor set is now gone — and refreshed the doc-comments on both surfaces to past-tense / consumer-named wording). +1/-0 docs/envoy-rust/BEHAVIOR_CONTRACT.md (new `/clusters` row in the "Admin endpoint body shapes" table). +PROGRESS.md narrative. Net +154 insertions, 7 deletions.

### Work summary

Landed `/clusters` GET endpoint at `crates/envoy-admin/src/endpoint.rs`: added `AdminEndpoint::Clusters` variant (6th total), extended `from_path` and `allowed_method` (the `|`-pattern in `allowed_method` now covers all 6 GET variants; trailing `// Task 9 adds: Listeners => "GET",` comment retained to telegraph the upcoming task), added explicit `AdminEndpoint::Clusters => render_clusters(handler)` arm to `render_with` ahead of the catch-all, added an `unreachable!` arm in the registry-only `render` path mirroring the Task 6 `ConfigDump` + Task 7 `ServerInfo` precedent, and added `render_clusters` free fn placed between `render_server_info` and `render_404` (PLAN-recommended placement). The renderer borrows `&AdminHandler`, walks `handler.cluster_manager().clusters()` (which sorts by-name internally per the accessor's contract), and emits two lines per cluster via `writeln!`. At `crates/envoy-cluster/src/cluster.rs`: added `pub fn clusters(&self) -> impl Iterator<Item = ClusterHandle> + '_` that collects the `HashMap` entries into a `Vec`, sorts by name, and yields `ClusterHandle`s on the `into_iter().map(..)` chain — explicit sort because the internal repr is `HashMap` (PLAN guessed `BTreeMap`), and the architecture lock-in #10 demands deterministic by-name order. At `crates/envoy-admin/src/handler.rs`: removed the field-level and accessor-level `#[allow(dead_code)] // wired for Task 8` annotations on `cluster_manager` (the last `#[allow(dead_code)]` pair on AdminHandler — Task 7 already removed the start_instant + command_line_options pairs); refreshed the doc-comments to consumer-named wording ("Read by `render_clusters` via the `cluster_manager()` accessor (Task 8)" / "Consumed by Task 8's `/clusters` renderer ..."). Added the new `/clusters` row to BEHAVIOR_CONTRACT.md's "Admin endpoint body shapes" table.

### Tests landed

- `cluster::clusters_accessor_tests::empty_cluster_manager_yields_no_clusters` (production crate `envoy-cluster`)
- `endpoint::clusters_tests::clusters_path_dispatches_on_get` (production crate `envoy-admin`)
- `endpoint::clusters_tests::clusters_405_on_post` (production crate `envoy-admin`)
- `endpoint::clusters_tests::clusters_renders_200_with_text_plain` (production crate `envoy-admin`)
- `endpoint::clusters_tests::clusters_body_is_empty_for_zero_clusters` (production crate `envoy-admin`)

5 new tests total: 1 in `envoy-cluster` (sibling `clusters_accessor_tests`) + 4 in `envoy-admin` (sibling `clusters_tests` placed AFTER `server_info_tests` and BEFORE `dispatch_tests`). `envoy-admin` test bucket: 52 passed (was 48 post-Task-7; +4 new). `envoy-cluster` test bucket: 22 passed (was 21 pre-task; +1 new). All existing tests stay green.

The `each_endpoint_declares_its_allowed_method` test in `dispatch_tests` was extended with a 6th `assert_eq!` for `Clusters` (one-line additive change). The `get_known_path_returns_endpoint` test in `dispatch_tests` was expanded from 3 endpoint arms (Ready/Stats/StatsPrometheus) to 6 endpoint arms (added ConfigDump/ServerInfo/Clusters) — opportunistic close of Task 7 code-quality review M1 per the prompt.

TDD discipline: tests written first, watched fail with the expected error shapes (`E0599: method clusters not found for ClusterManager`, `E0599: no variant Clusters found for AdminEndpoint`), then implementation added; iterated once on a `from_path_unknown_returns_none` regression (existing legacy test asserted `/clusters` → `None`, which became false once we added the `Clusters` variant) — fixed by switching that probe path to `/listeners` (still unknown until Task 9); re-ran tests to confirm 5/5 new tests green plus all legacy buckets green.

### Deviations from PLAN

1. **`ClusterManager`'s internal repr is `HashMap<String, Arc<Cluster>>`, not `BTreeMap<String, Cluster>` as the PLAN stub guessed.** Anticipated by the prompt ("disk is `HashMap<String, Arc<Cluster>>`"). The accessor sorts explicitly via `Vec::sort_by` after collecting `iter()` entries so the deterministic by-name order required by architecture-decision lock-in #10 is maintained at the accessor layer rather than at the consumer. The `ClusterHandle` is constructed by `Arc::clone`-ing the cached `Arc<Cluster>` — same shape as `ClusterManager::get`. No `BTreeMap` migration is needed at this phase; the consumer (Task 8 `/clusters` renderer) only iterates and never lookups by ordering.

2. **`render` arm for `Clusters` is `unreachable!()` mirroring Task 6's `ConfigDump` + Task 7's `ServerInfo` arms.** PLAN-explicit ("mirroring Task 6/7's `unreachable!()` precedent"). Documents the structural invariant that `render_with` is the only legitimate path for state-bearing endpoints.

3. **Sibling-module test placement.** `clusters_accessor_tests` placed AFTER the existing `mod tests` block in `cluster.rs` (matching the file's existing single-module convention; minimal disturbance). `clusters_tests` placed AFTER `server_info_tests` and BEFORE `dispatch_tests` in `endpoint.rs` (matching the increasing-task-number cadence; consistent with Tasks 6 and 7 placements). Both are sibling rather than nested modules per the standing 08.1 convention. Recorded as a Deviation per Task 1's review reminder even though it's the expected placement.

4. **Opportunistic close of Task 7 code-quality review M1: `get_known_path_returns_endpoint` extended from 3 endpoint arms to 6.** Per the prompt's explicit naming as an opportunistic close. The expanded test covers Ready/Stats/StatsPrometheus + the 08.1-added ConfigDump/ServerInfo/Clusters (Task 9 will add Listeners). Future task-9 implementer adds a 7th `assert!(matches!(...))` arm; no further refactor needed.

5. **`from_path_unknown_returns_none` test updated: `/clusters` probe replaced with `/listeners`.** Pre-task this test asserted `/clusters` → `None` (which was true when Task 5 last touched the file). Once Task 8 adds the `Clusters` variant, `/clusters` resolves to `Some(AdminEndpoint::Clusters)`. Switched to `/listeners` (still unknown until Task 9) as the unknown-path probe; the empty-path and `/` cases are retained. Minimal, surgical correction.

6. **`#[allow(dead_code)]` pair on `cluster_manager` removed in this commit (last pair on AdminHandler accessor set).** Per the prompt's explicit obligation. The doc-comment on the field was also refreshed from "Phase 08.1 D13a: cluster manager handle for the `/clusters` renderer (Task 8)..." to add a sentence "Read by `render_clusters` via the `cluster_manager()` accessor (Task 8).", and the accessor's doc-comment was rewritten from "Reserved for Task 8's `/clusters` renderer. Currently unused." to "Consumed by Task 8's `/clusters` renderer; borrowed into the renderer to walk all clusters in deterministic by-name order." — past-tense / consumer-named per the prompt's "judgment" guidance.

7. **`render_clusters` doc-comment carries the architecture lock-in #10 note inline.** The PLAN stub's body comment naming "lock-in #10" was preserved verbatim so future readers tracing why per-endpoint counter lines are omitted at 08.1 find the architectural context at the renderer (closes the question without needing to follow a SPEC link).

8. **`TINY_BOOTSTRAP` was NOT hoisted to `pub(super)` in this task (opportunistic close M2 of Task 7 review left in place).** The prompt explicitly marked this as "not required". The PLAN-stub test bodies inline the YAML literal twice and that was the path of least drift; revisiting the hoist is a future-task cleanup if Task 9's `/listeners` tests would benefit from sharing.

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
   Compiling envoy-cluster v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-cluster)
   Compiling envoy-http1 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http1)
   Compiling envoy-tcp v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-tcp)
   Compiling envoy-http2 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http2)
   Compiling envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
   Compiling http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
   Compiling http2-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http2-echo-server)
   Compiling envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 24.69s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking envoy-cluster v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-cluster)
    Checking envoy-http1 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http1)
    Checking envoy-tcp v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-tcp)
    Checking envoy-http2 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http2)
    Checking envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
    Checking http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
    Checking http2-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Checking envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.91s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
     Running unittests src/lib.rs (target/debug/deps/envoy_admin-4c2e520e2a6223dc)
test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s
     Running unittests src/lib.rs (target/debug/deps/envoy_cluster-97dbad6faa16a2eb)
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

(envoy-admin tail: 52 = 48 pre-task + 4 new. envoy-cluster tail: 22 = 21 pre-task + 1 new. Full `cargo test --workspace` green across all buckets — no port-binding flakes resurfaced this run; the differential `http1_echo_backend_*` flake noted at prior tasks did NOT recur.)

`cargo deny check`:
```
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues. Task 8 introduces no new direct deps — `bytes`, `envoy-cluster`, and `envoy-http1` are already direct deps of `envoy-admin` from earlier tasks.)

---

## Task 9 — D8: /listeners endpoint + BEHAVIOR_CONTRACT row + opportunistic closes

**Commit:** `2bb3545` — `phase 08.1: task 9 — /listeners endpoint + BEHAVIOR_CONTRACT row`
**LoC delta:** +205/-27 crates/envoy-admin/src/endpoint.rs (~55 production: `Listeners` variant on `AdminEndpoint` + `/listeners` `from_path` arm + extended `allowed_method` `|`-chain to include `Listeners` and removed the trailing `// Task 9 adds: Listeners => "GET",` placeholder + `render` `unreachable!` arm for `Listeners` + `render_with` explicit `Listeners` arm + `render_listeners` free fn that walks `handler.bootstrap().static_resources.listeners.iter()`, maps to `(name, "<addr>:<port>")` tuples via direct field access through the `Address` struct's `socket_address` field, sorts by name, and emits one `"<name>::<addr>:<port>"` line per listener; ~123 test for the new `listeners_tests` sibling module — 6 tests including a `TWO_LISTENERS_BOOTSTRAP` literal with `zebra` declared BEFORE `alpha` exercising both populated-body emission and sorted-by-name determinism; ~10 test for the 7th-arm extensions to `get_known_path_returns_endpoint` (Listeners) and `each_endpoint_declares_its_allowed_method` (Listeners); the legacy `from_path_unknown_returns_none` test's `/listeners` probe re-targeted to `/nope` since `/listeners` is now a known endpoint; the previously-inlined empty-bootstrap YAML literal in `server_info_tests` and `clusters_tests` replaced with `super::config_dump_tests::TINY_BOOTSTRAP` after hoisting the `const TINY_BOOTSTRAP` to `pub(super)` — closes Task 7 review M2 carryforward). +1/-0 docs/envoy-rust/BEHAVIOR_CONTRACT.md (new `/listeners` row in the "Admin endpoint body shapes" table). +PROGRESS.md narrative. Net +206 insertions, 27 deletions.

### Work summary

Landed `/listeners` GET endpoint at `crates/envoy-admin/src/endpoint.rs`: added `AdminEndpoint::Listeners` variant (7th and final 08.1 GET variant — the enum now carries all 7 GET-only endpoints `Ready / Stats / StatsPrometheus / ConfigDump / ServerInfo / Clusters / Listeners`), extended `from_path` and `allowed_method` (the `|`-pattern in `allowed_method` now exhaustively covers all 7 GET variants and the placeholder comment is gone), added explicit `AdminEndpoint::Listeners => render_listeners(handler)` arm to `render_with` ahead of the catch-all, added an `unreachable!` arm in the registry-only `render` path mirroring the Task 6/7/8 precedent (4-of-7 `render` arms are now `unreachable!` — refactor deferred per the prompt), and added `render_listeners` free fn placed between `render_clusters` and `render_404`. The renderer borrows `&AdminHandler`, walks `handler.bootstrap().static_resources.listeners.iter()`, builds `(name, "<addr>:<port>")` tuples via direct field access on the `Address` struct (single `socket_address: SocketAddress` field — `SocketAddress` carries `address: String` and `port_value: u16`), sorts the tuple `Vec` by name with `Vec::sort_by`, and `writeln!`s into the body string. Hoisted `config_dump_tests::TINY_BOOTSTRAP` to `pub(super) const` so `server_info_tests`, `clusters_tests`, and the new `listeners_tests` share one source for the minimal empty-listener/empty-cluster bootstrap YAML — closes Task 7 review M2 carryforward (Task 8 explicitly deferred to Task 9). Re-targeted the legacy `from_path_unknown_returns_none` test's `/listeners` probe to `/nope` (and refreshed its comment) since `/listeners` is now a known endpoint. Extended `get_known_path_returns_endpoint` from 6 → 7 arms and `each_endpoint_declares_its_allowed_method` from 6 → 7 arms (one new `assert!`/`assert_eq!` each). Added the new `/listeners` row to BEHAVIOR_CONTRACT.md's "Admin endpoint body shapes" table.

### Tests landed

- `endpoint::listeners_tests::listeners_path_dispatches_on_get` (production crate `envoy-admin`)
- `endpoint::listeners_tests::listeners_405_on_post` (production crate `envoy-admin`)
- `endpoint::listeners_tests::listeners_renders_200_with_text_plain` (production crate `envoy-admin`)
- `endpoint::listeners_tests::listeners_body_is_empty_for_zero_listeners` (production crate `envoy-admin`)
- `endpoint::listeners_tests::listeners_body_emits_name_address_port_per_listener` (production crate `envoy-admin`)
- `endpoint::listeners_tests::listeners_body_is_sorted_by_name` (production crate `envoy-admin`)

6 new tests total, all in `envoy-admin` (sibling `listeners_tests` placed AFTER `clusters_tests` and BEFORE `dispatch_tests`, matching the increasing-task-number cadence consistent with Tasks 6/7/8 placements). `envoy-admin` test bucket: 58 passed (was 52 post-Task-8; +6 new). All existing tests stay green; the re-targeted `from_path_unknown_returns_none` probe (`/listeners` → `/nope`) continues to pass, and the 7th-arm extensions to `get_known_path_returns_endpoint` + `each_endpoint_declares_its_allowed_method` also pass.

TDD discipline: tests written first (sibling `listeners_tests` + dispatch-tests extensions), watched fail with the expected `E0599: no variant or associated item named Listeners found for enum AdminEndpoint` (3 callsites in `listeners_tests` + 1 in `dispatch_tests` `get_known_path_returns_endpoint` + 1 in `dispatch_tests` `each_endpoint_declares_its_allowed_method` — exactly the variant-not-found shape predicted by the Task 8 precedent), then implementation added (variant + `from_path` arm + `allowed_method` extension + `render` `unreachable!` arm + `render_with` arm + `render_listeners` fn), re-ran tests to confirm 6/6 new tests green plus all 52 legacy tests still green (58 total).

### Deviations from PLAN

1. **`envoy_config::Address` is a STRUCT, not an enum.** Anticipated by the prompt (the PLAN-stub guessed `Address::SocketAddress(sa)` enum-variant shape — disk-truth at `crates/envoy-config/src/bootstrap.rs:208-212` is `struct Address { socket_address: SocketAddress }`). Renderer uses direct field access `l.address.socket_address.address` (String) and `l.address.socket_address.port_value` (u16); no `match` on `Address`. This is the load-bearing deviation the prompt flagged.

2. **`TINY_BOOTSTRAP` hoisted to `pub(super)` and consumed by both pre-existing sibling test modules.** Per the prompt's explicit Task 9 obligation. After the hoist, the inlined `"node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n"` literal in `server_info_tests` (4 sites: `server_info_renders_200_with_application_json`, `server_info_body_has_required_keys`, `server_info_state_is_live_at_phase_08_1`, `server_info_uptime_is_non_negative`) and `clusters_tests` (2 sites: `clusters_renders_200_with_text_plain`, `clusters_body_is_empty_for_zero_clusters`) was replaced with `super::config_dump_tests::TINY_BOOTSTRAP`. The new `listeners_tests` consumes it too for the 200 + empty-body cases. The `server_info_node_subtree_carries_id` test retains its own inlined YAML because it uses a different `node.id` value (`my-id` vs `t`). Closes Task 7 review M2 carryforward; the Task 8 narrative explicitly noted this hoist was deferred to Task 9.

3. **`from_path_unknown_returns_none` probe re-targeted from `/listeners` → `/nope`.** Per the prompt's explicit Task 9 obligation (test would regress once `/listeners` becomes known). The probe path was changed to `/nope` (genuinely unknown across both 08.1 and 08.2 endpoint surfaces) and the test's comment was refreshed to note Task 9 closed the 08.1 endpoint surface — all 7 GET-only variants are now known. The empty-path and `/` cases stay unknown and are preserved.

4. **`get_known_path_returns_endpoint` extended from 6 → 7 arms (added `/listeners` → `Listeners`).** Per the prompt's mandatory Task 9 inclusion. One-line additive `assert!(matches!(...))` extension; consistent with Task 8's expansion from 3 → 6 arms.

5. **`each_endpoint_declares_its_allowed_method` extended from 6 → 7 arms (added `Listeners.allowed_method() == "GET"`).** Per the prompt's mandatory Task 9 inclusion. One-line additive `assert_eq!` extension.

6. **`render` arm for `Listeners` is `unreachable!()` mirroring Task 6/7/8 arms.** PLAN-explicit. Documents the structural invariant that `render_with` is the only legitimate path for state-bearing endpoints. The Task 9 commit leaves the registry-only `render` path with 4-of-7 `unreachable!` arms — refactor (e.g. splitting `render` and `render_with` into separate types, or making `Listeners`/`Clusters`/`ServerInfo`/`ConfigDump` not implement `render` at all) is explicitly deferred per the prompt's "DEFERRED" guidance. Carryforward to REVIEW or a future task.

7. **`render_listeners` placement.** Placed AFTER `render_clusters` and BEFORE `render_404`, matching the PLAN-recommended placement and the Tasks 6/7/8 cadence (each new state-bearing renderer was appended in increasing-task-number order ahead of the 404/405 helpers). Recorded as a Deviation per Task 1's review reminder even though it's the expected placement.

8. **`render_listeners` body construction uses a `Vec<(String, String)>` + `Vec::sort_by` approach** rather than the `BTreeMap` insertion-order approach the `/clusters` renderer relies on (`/clusters` reads from `ClusterManager::clusters()` which sorts at the accessor layer). `static_resources.listeners` is a `Vec<Listener>` on the parsed `Bootstrap` — declaration order is preserved by `serde_yaml` so the renderer is the natural place to enforce the deterministic by-name order. The two-listener `zebra` / `alpha` test (`listeners_body_is_sorted_by_name`) exercises this directly; a future task that introduces an `xDS`-derived listener set would either add a similar accessor-level sort or extend the renderer.

9. **`Bytes::from(body_string)` wrap pattern** matches Tasks 6/7/8's `render_config_dump` / `render_server_info` / `render_clusters` precedent (build the body as a Rust `String` via `writeln!`, then wrap into `bytes::Bytes` for the `Response.body` field). No `BytesMut` needed for plain text; PLAN-explicit.

10. **`TWO_LISTENERS_BOOTSTRAP` is a private `const` inside `listeners_tests`** (not hoisted to `pub(super)` or moved to `config_dump_tests`). Only consumed by two tests within `listeners_tests` itself; no cross-module reuse anticipated at this phase. Mirrors Task 7's posture of keeping test-specific YAML literals local to their consuming module.

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
   Compiling envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
   Compiling envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.64s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking envoy-cluster v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-cluster)
    Checking envoy-http1 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http1)
    Checking envoy-tcp v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-tcp)
    Checking envoy-http2 v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-http2)
    Checking envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
    Checking http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
    Checking http2-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Checking envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 18.00s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
     Running unittests src/lib.rs (target/debug/deps/envoy_admin-36014a9343649dbc)
test result: ok. 58 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s
     Running unittests src/lib.rs (target/debug/deps/envoy_cluster-97dbad6faa16a2eb)
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

(envoy-admin tail: 58 = 52 pre-task + 6 new. envoy-cluster tail: 22, unchanged from Task 8. Full `cargo test --workspace` green across all buckets — no port-binding flakes resurfaced this run; the differential `http1_echo_backend_*` flake noted at prior tasks did NOT recur.)

`cargo deny check`:
```
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues. Task 9 introduces no new direct deps — `bytes`, `envoy-config`, and `envoy-http1` are already direct deps of `envoy-admin` from earlier tasks; the renderer composes plain `String` + `bytes::Bytes` exclusively.)

---

## Task 10 — D15: BodyRule::JsonShape + BodyRule::TextLines harness extensions

**Commit:** `9cf8831` — `phase 08.1: task 10 — BodyRule::JsonShape + BodyRule::TextLines harness extensions`
**LoC delta:** +1 dep (`tests/differential/Cargo.toml`), +302 in `tests/differential/src/lib.rs` (~180 production: two new struct-form `BodyRule` variants `JsonShape` + `TextLines` with `#[serde(default)]`-per-field bodies, the helper struct `JsonSubtreeRule` carrying `path: String` + `expected: serde_yaml::Value`, the `walk_pointer` dotted-path free fn, and the two new match arms on `assert_body_rule` for `BodyRule::JsonShape` and `BodyRule::TextLines`; ~120 tests: new sibling `#[cfg(test)] mod body_rule_extension_tests` block at the file's end with 7 unit tests). Net +303 insertions, 0 deletions.

### Work summary

Landed the two new `BodyRule` struct-form variants for the `/config_dump` (JSON-shape) and `/clusters` + `/listeners` (line-oriented text) diff territory, plus the helper struct + the dotted-path walker fn + the two new dispatch arms on `assert_body_rule`. Both variants reuse the established `tag = "kind"`-internally-tagged serde shape that 06.1 Task 12's `BodyRule::PrometheusExposition` established (per architecture-decision lock-in #12). `BodyRule::JsonShape` parses both bodies as JSON via `serde_json::from_slice`, asserts they are JSON objects, fail-strict-checks `required_keys` on BOTH sides, and (optionally) walks `required_subtree.path` on both sides via `walk_pointer` then JSON-string-compares the addressed sub-values. `BodyRule::TextLines` decodes both bodies as UTF-8 via `std::str::from_utf8`, builds a `BTreeSet<&str>` per side via `.lines().collect()`, fail-strict-checks `required_lines` (exact match) and `required_line_prefixes` (at-least-one-line-prefixes) on BOTH sides. The allowlist-* and `value_may_differ_keys` fields are accepted at the schema level so fixture YAML can declare them now, but DO NOT participate in fail logic at Task 10 — strictness is intentionally deferred to Task 11 per PLAN line 2231/2301 ("the executor adapts"). Sibling `mod body_rule_extension_tests` placed AFTER the existing `mod tests` block — same per-task end-of-file placement convention Tasks 6/7/8/9 used for their new sibling test modules (e.g. `listeners_tests` placed after `clusters_tests`).

### Tests landed

- `body_rule_extension_tests::json_shape_required_keys_pass_when_all_present` (crate `differential`)
- `body_rule_extension_tests::json_shape_required_keys_fail_when_missing` (crate `differential`)
- `body_rule_extension_tests::json_shape_envoy_only_key_allowed` (crate `differential`)
- `body_rule_extension_tests::json_shape_required_subtree_value_exact` (crate `differential`)
- `body_rule_extension_tests::text_lines_required_lines_pass_when_present` (crate `differential`)
- `body_rule_extension_tests::text_lines_envoy_only_lines_allowed` (crate `differential`)
- `body_rule_extension_tests::text_lines_required_prefix_matches` (crate `differential`)

7 new tests total, all in `differential` crate. Crate test bucket: 84 (was 77 post-Task-9-no-op-on-this-crate; +7 new).

TDD discipline: 7 tests written FIRST, watched fail with the expected compile errors `error[E0432]: unresolved import super::JsonSubtreeRule` + `error[E0599]: no variant named JsonShape found for enum BodyRule` + `error[E0599]: no variant named TextLines found for enum BodyRule` (the exact variant-not-found shape predicted by the controller's TDD step). Implementation landed (BodyRule extension + JsonSubtreeRule struct + walk_pointer fn + two new dispatch arms + serde_json dep + JsonSubtreeRule PartialEq/Eq derive), re-ran tests to confirm 7/7 new tests green plus all 77 legacy `differential` tests still green (84 total). Full `cargo test --workspace` green.

### Deviations from PLAN

1. **Package name: PLAN-stub used `-p envoy-rust-differential`; actual is `-p differential`** per `tests/differential/Cargo.toml:2`. Used `cargo test -p differential body_rule_extension_tests`. Pre-flagged by the controller.

2. **Dispatch shape: PLAN-stub assumed `rule.assert_equivalent(envoy, rust)` method; actual is free fn `assert_body_rule(rule, envoy, rust)`**. Extended the existing free fn at `tests/differential/src/lib.rs:2111` with two new match arms instead of adding a method wrapper — minimal surface, matches the existing 06.1 / 06.3 `PrometheusExposition` precedent. `assert_body_rule` kept private (`fn`, not `pub fn`); sibling-`mod`-in-same-file scope reaches it via `use super::{BodyRule, JsonSubtreeRule, assert_body_rule};`. Pre-flagged by the controller.

3. **Tests adapted to free-fn shape: changed all 7 tests from `rule.assert_equivalent(...)` to `assert_body_rule(&rule, ...)`**. Pre-flagged by the controller.

4. **`JsonSubtreeRule` is a struct (not a tuple): test 4 instantiates `Some(JsonSubtreeRule { path: ..., expected: ... })`** not the tuple shape the PLAN-stub used at lines 2086-2098. Adapted to the architecture-decision lock-in #12 struct form. Pre-flagged by the controller.

5. **`Eq` derive RETAINED on `BodyRule`** — the controller's drift-verified-ground-truth-#4 predicted the `Eq` drop, but verification against on-disk `/Users/esa/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde_yaml-0.9.34+deprecated/src/value/mod.rs:673` shows `impl Eq for Value {}` is explicitly provided (manually, alongside the auto-derived `PartialEq`). So `BodyRule::JsonShape { required_subtree: Option<JsonSubtreeRule>, ... }` propagates `Eq` cleanly when `JsonSubtreeRule` derives `Eq` too. Picked the simpler / cleaner option per `feedback_pick_recommendation`: kept the original `#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]` on `BodyRule` and added `#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]` to `JsonSubtreeRule`. No cascade drops on `Driver` / `Equivalence` / `Expectations` were needed. The doc-comment on `BodyRule` was updated to document this finding so a future Task-11 author doesn't re-litigate it.

6. **`serde_json = "1"` added to `tests/differential/Cargo.toml` direct deps**. D-3.2 permitted-foundations (not a foundations grant); no ADR needed. Pre-flagged by the controller.

7. **Strictness pick: `required_keys` / `required_subtree` / `required_lines` / `required_line_prefixes` are fail-strict assertions; the `allowlist_envoy_only_keys` / `allowlist_envoy_rust_only_keys` / `value_may_differ_keys` / `allowlist_envoy_only_lines` / `allowlist_envoy_rust_only_lines` fields are accepted at the schema level (so fixture 0014 YAML can declare them now) but do NOT participate in fail logic at Task 10.** Defers to Task 11 / phase-end REVIEW per PLAN line 2231/2301 ("the executor adapts to the desired strictness level"). The five no-op fields are pattern-bound via `_` in the match arms with comments naming the deferral. Pre-flagged by the controller.

8. **Test module placement at line ~3389 AFTER `mod tests`'s closing `}` at line 3381.** Mirrors the per-task end-of-file placement convention Tasks 6/7/8/9 used (e.g. Task 9's `listeners_tests` placed after `clusters_tests`). Pre-flagged by the controller.

9. **Doc-comments added on `BodyRule` (existing, extended), `JsonShape` variant, `TextLines` variant, `JsonSubtreeRule` struct, and `walk_pointer` fn** narrating the SPEC-tie + the Task 11 strictness-deferral disposition. Mirrors the doc-comment density established by `PrometheusExposition` at 06.1 / 06.3.

10. **`walk_pointer` uses `.with_context(|| format!(..))` (lazy)** instead of the PLAN-stub-pseudocode's `.context(format!(..))` (eager) — clippy-clean per `or_fun_call` lint posture; matches the workspace convention.

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
   Compiling differential v0.0.0 (/Users/esa/git/envoy-rust/tests/differential)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.98s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
    Checking differential v0.0.0 (/Users/esa/git/envoy-rust/tests/differential)
    Checking envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 24.73s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
     Running unittests src/lib.rs (target/debug/deps/differential-c77ccb82d4505bf6)
test result: ok. 84 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.82s
     Running unittests src/lib.rs (target/debug/deps/envoy_admin-36014a9343649dbc)
test result: ok. 58 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s
     Running unittests src/lib.rs (target/debug/deps/envoy_cluster-97dbad6faa16a2eb)
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

(`differential` bucket: 84 = 77 pre-task + 7 new. `envoy-admin` bucket: 58, unchanged from Task 9 — Task 10 is a `tests/differential/`-only change. Full `cargo test --workspace` green across all buckets. One transient `backend::tests::tcp_proxy_backend_spawns_and_echoes` port-binding flake observed on the first workspace-wide run; passed on isolated re-run and on the second workspace-wide run. The same flake was noted at Tasks 7/8/9 — pre-existing, unrelated to this task's diff.)

`cargo deny check`:
```
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues. Task 10 adds `serde_json = "1"` to `tests/differential/Cargo.toml`'s `[dependencies]` section — `serde_json` is D-3.2 permitted-foundations and is already transitively present in the workspace dep tree from earlier tasks; `cargo deny check` remains clean. No new top-level Cargo deps land per architecture-decision lock-in #17.)

---

## Task 11 — D17.1 — Fixture `0014-admin-config-dump-server-info` + Docker-gated wrapper

**Commit:** `6ee1ad7` — `phase 08.1: task 11 — fixture 0014 + Driver::AdminScrape Vec<sub-case> widening`
**LoC delta:** +~330 production (`tests/differential/src/lib.rs`: `AdminScrapeCase` struct + `Driver::AdminScrape` widening to `Vec<AdminScrapeCase>`; multi-scrape dispatch loop; `JsonShape` strictness wiring — `value_may_differ_keys` + per-side `allowlist_*_keys` + `required_subtree.expected` + shared-key value-equality; `TextLines` strictness wiring — per-side `allowlist_*_lines` + Task 11 NEW per-side `allowlist_*_line_prefixes` family; `Box<BodyRule>` on `Http1WithAccessLog` to land clippy's `large_enum_variant` after the `Driver::AdminScrape` shrink; `check_content_type` parameter-tolerance for the bare-vs-charset divergence; diagnostic `DIFFERENTIAL_DUMP_ADMIN` env-var dump; doc-comment refresh on `BodyRule::{JsonShape,TextLines}` + `JsonSubtreeRule`), +~150 tests (1 Docker-gated wrapper + 1 new `driver_admin_scrape_parses_with_multiple_scrapes` + 7 new strictness-wiring unit tests in `body_rule_extension_tests` + 2 pre-existing test adaptations + Task 11 NEW `text_lines_envoy_only_line_prefix_*` × 2), +~430 fixture/doc (fixture 0014's 4 files at 419 LoC + fixture 0011's expectations.yaml re-indent migration). Net +~910 insertions, ~440 deletions (the bulk of fixture 0011's "deletions" are line-level re-indentation under the new `scrapes:` parent — no semantic delta).

### Work summary

Landed fixture `0014-admin-config-dump-server-info` with its 4 paired files (envoy.yaml + envoy-rust.yaml + expectations.yaml + README.md) and the Docker-gated wrapper at `tests/differential/tests/admin_config_dump_server_info.rs`. The fixture drives the new `Driver::AdminScrape { pre_requests, scrapes: Vec<AdminScrapeCase> }` multi-case shape (Task 11 widening — architecture-decision lock-in #13 forbids a new Driver variant; the per-sub-case `path` / `expected_*` tuple moved into a dedicated `AdminScrapeCase` struct) against 4 admin endpoints (`/config_dump`, `/server_info`, `/clusters`, `/listeners`) in a single bilateral invocation against upstream Envoy v1.33. Fixture 0011 migrated in lockstep to a single-element `scrapes:` list with no semantic change.

Same commit wires the Task 10 strictness deferrals to fail-strict: `JsonSubtreeRule.expected` now asserts envoy_sub AND rust_sub equal the expected value; `JsonShape` enforces top-level key-set equality modulo per-side `allowlist_envoy_only_keys` / `allowlist_envoy_rust_only_keys` AND `value_may_differ_keys`; `TextLines` enforces line-set equality modulo per-side `allowlist_envoy_only_lines` / `allowlist_envoy_rust_only_lines` plus a Task 11 NEW per-side `allowlist_envoy_only_line_prefixes` / `allowlist_envoy_rust_only_line_prefixes` family for address-bearing varying-suffix lines (fixture 0014's `/clusters` per-endpoint counter lines + `/listeners` per-side address+port shapes).

Empirical allow-list seeding converged to GREEN in one iteration after the strictness model + the per-side line-prefix family landed: the first Docker-gated run captured both proxies' bodies via the new `DIFFERENTIAL_DUMP_ADMIN=1` diagnostic env-var; the second run with the seeded expectations went green. The 4 sub-cases assert on the BEHAVIOR_CONTRACT dispositions: `/config_dump` — required_keys `["configs"]` + required_subtree `configs.0.@type == BootstrapConfigDump` + `configs` ∈ value_may_differ; `/server_info` — required_keys minus the uptime-field-naming split (envoy emits Duration-string `uptime_current_epoch`/`uptime_all_epochs`; envoy-rust emits seconds-u64 `uptime_*_seconds`) absorbed via per-side `allowlist_*_keys`, with `version` / `hot_restart_version` / `command_line_options` / `node` ∈ value_may_differ; `/clusters` — `observability_name::backend` required-bilateral, 9 envoy-only address-INVARIANT lines + 1 envoy-only address-BEARING prefix (`backend::192.168.65.254:`) absorbed, `backend::default_priority::endpoints` envoy-rust-only allow-listed; `/listeners` — `ingress_http::` required prefix bilateral, per-side address+port shapes (`ingress_http::0.0.0.0:` envoy / `ingress_http::127.0.0.1:` envoy-rust) absorbed via prefix-allow-list.

### Tests landed

- `admin_config_dump_server_info` (`tests/differential/tests/admin_config_dump_server_info.rs`) — Docker-gated bilateral test; 4 admin-scrape sub-cases against `tests/fixtures/0014-admin-config-dump-server-info/`.
- `body_rule_extension_tests::json_shape_required_subtree_fails_when_expected_value_mismatches` — `JsonSubtreeRule.expected` wiring.
- `body_rule_extension_tests::json_shape_fails_on_envoy_only_key_outside_allowlist` — envoy-only-keys diff strictness.
- `body_rule_extension_tests::json_shape_fails_on_rust_only_key_outside_allowlist` — rust-only-keys diff strictness.
- `body_rule_extension_tests::json_shape_fails_when_shared_key_values_differ_outside_may_differ` — shared-key value-equality.
- `body_rule_extension_tests::json_shape_passes_when_value_diff_inside_may_differ` — `value_may_differ_keys` allowance.
- `body_rule_extension_tests::text_lines_fails_on_envoy_only_line_outside_allowlist` — envoy-only-lines diff strictness.
- `body_rule_extension_tests::text_lines_fails_on_rust_only_line_outside_allowlist` — rust-only-lines diff strictness.
- `body_rule_extension_tests::text_lines_envoy_only_line_prefix_absorbs_varying_suffix` — per-side line-prefix allow-list (Task 11 NEW family).
- `body_rule_extension_tests::text_lines_envoy_only_line_prefix_does_not_shadow_other_lines` — per-side prefix family non-shadowing semantics.
- `driver_admin_scrape_parses_with_multiple_scrapes` — multi-sub-case YAML parse coverage for `Driver::AdminScrape { scrapes: [...] }`.
- Pre-existing 2 test adaptations to the new strictness model: `json_shape_required_subtree_value_exact` (drop the `"other":1` vs `"other":99` shared-key asymmetry — now diff-strict unless in `value_may_differ_keys`); `text_lines_required_prefix_matches` (per-side address-bearing lines now require explicit allow-list seeding).
- Pre-existing 2 test re-shapings to the new `Driver::AdminScrape { scrapes: [...] }` shape: `driver_admin_scrape_parses_with_default_pre_requests`, `driver_admin_scrape_parses_with_pre_requests`.

11 brand-new tests + 1 new Docker-gated wrapper + 4 pre-existing adaptations. `differential` lib bucket: 84 pre-task + 10 new = 94 (+10). `differential` integration: 14 fixtures simultaneously (was 13; +1 = fixture 0014 wrapper).

TDD discipline: 3 RED tests written FIRST against the new `Driver::AdminScrape { scrapes: [...] }` shape (the variant-doesn't-have-field compile error confirmed RED); 7 RED strictness-wiring tests written FIRST for `JsonShape` + `TextLines` (one failing at compile time for variant-field changes, the rest failing at runtime for the schema-level no-op fields); 2 RED tests for the Task 11 NEW per-side line-prefix family (failing at compile time for missing fields). After GREEN landed across all 10 + 3 new + 2 adapted = 15 differential-bucket changes, ran the Docker-gated fixture 0014 wrapper LOCALLY against Docker Desktop 4.40.0; first run with the relaxed `kind: json_shape`/`text_lines` expectations captured all 4 bodies via `DIFFERENTIAL_DUMP_ADMIN=1`; second run with empirically-seeded allow-lists went GREEN.

### Deviations from PLAN

1. **`Driver::AdminScrape` widened to `Vec<AdminScrapeCase>` sub-cases; fixture 0011 migrated in lockstep.** (Rationale: PLAN's expectations.yaml sketch uses an `admin_scrapes: [...]` shape AND architecture-decision lock-in #13 forbids a new Driver variant. The widening + 0011-migration is the only coherent move that honors both constraints. Fixture 0011's single-path form becomes a single-element `scrapes:` list with no semantic change. Pre-flagged by the controller as Deviation #1.)

   **Note on future 08.2 re-widening:** `pre_requests` placement stays on `Driver::AdminScrape` itself (not per-`AdminScrapeCase`) at 08.1 because the two extant call sites (fixture 0011: single pre-request then scrape; fixture 0014: no pre-requests, 4 scrapes) are both well-served by the shared shape. 08.2's fixture 0015 (drain action interleaved between scrapes) will likely force re-widening `pre_requests` into `AdminScrapeCase` (with `#[serde(default)]` so existing fixtures stay clean) AND restructuring the dispatch loop to interleave per-case pre-actions. Forecasted re-widening is intentionally deferred to 08.2's PLAN; calling it out here per D-3.5 append-only discipline so the next implementer rediscovers the constraint upfront.

2. **Strictness wiring at Task 11 (Task 10 minor-findings #1, #2, #3 closed).** Wired `JsonSubtreeRule.expected` into the dispatch arm (asserts envoy_sub == expected AND rust_sub == expected); wired `value_may_differ_keys` + per-side `allowlist_*_keys` into `BodyRule::JsonShape` (top-level key-set diff modulo allow-lists, shared-key value-equality); wired per-side `allowlist_*_lines` into `BodyRule::TextLines` (line-set diff). Doc-comments updated on `BodyRule::{JsonShape,TextLines}` + `JsonSubtreeRule` to document the wired strictness. Pre-flagged by the controller.

3. **Task 11 NEW: per-side line-prefix allow-list family added to `BodyRule::TextLines`.** Two new fields `allowlist_envoy_only_line_prefixes: Vec<String>` + `allowlist_envoy_rust_only_line_prefixes: Vec<String>` (both `#[serde(default)]`). Fixture 0014 surfaces two empirical use cases the per-side exact-line allow-list cannot cover cleanly: (a) `/clusters` per-endpoint counter lines like `backend::192.168.65.254:<ephemeral-port>::cx_active::0` (~17 lines per endpoint with a kernel-ephemeral port that shifts per fixture run); (b) `/listeners` per-side address+port lines `ingress_http::0.0.0.0:<container-port>` (envoy) vs `ingress_http::127.0.0.1:<ephemeral-port>` (envoy-rust). Adding 2 fields to the existing struct-form variant is a 2-line schema delta + ~16-line dispatch delta; the alternative (per-side post-prefix-allow-list dispatch logic, or hard-coded numeric-port wildcards) is strictly more complex. 2 new unit tests cover the family's pass + non-shadow semantics.

4. **`check_content_type` widened: bare-expected matches actual-with-parameters.** Upstream Envoy emits `text/plain; charset=UTF-8` for `/clusters` + `/listeners`; envoy-rust emits the bare `text/plain` (per the renderers in `crates/envoy-admin/src/endpoint.rs`, Tasks 8 + 9 — content-type pin is intentional, BEHAVIOR_CONTRACT will absorb the charset-parameter variance in a follow-on phase). Strict-match would require either changing envoy-rust's renderers (out of Task 11 scope) or splitting `expected_content_type` per-side (more invasive). Widening `check_content_type` to accept the parameter-bearing form when the expected value is the bare media-type form preserves fixture 0011's strict semantics (its expected value `text/plain; charset=UTF-8` carries a parameter, so it still strict-matches) AND unblocks fixture 0014. Doc-comment narrates the disposition.

5. **`Http1WithAccessLog.expected_body: Box<BodyRule>` to land clippy's `large_enum_variant` lint.** Pre-Task-11, `Driver::AdminScrape`'s inline `path: String, expected_status: u16, expected_content_type: String, expected_body_rule: BodyRule` made it the second-largest `Driver` variant (~150 bytes); after Task 11's widening, `Driver::AdminScrape { pre_requests, scrapes }` is ~48 bytes (two `Vec<T>`s), so the largest variant `Http1WithAccessLog` (~362 bytes, contains a `BodyRule` direct) is now ~285 bytes larger than the new second-largest variant `Http1` (77 bytes) — tipping past clippy's 200-byte threshold. The clippy hint itself suggests boxing the `BodyRule`. Auto-deref handles the single dispatch call site (`assert_body_rule(expected_body, ...)` works with `&Box<BodyRule>` via deref-coercion). No other call sites needed updating; no fixture YAML changes (Box<T> deserializes identically to T under serde).

6. **`DIFFERENTIAL_DUMP_ADMIN` env-var diagnostic added to the AdminScrape dispatch arm.** Dumps both sides' bodies + content-types for ALL sub-cases BEFORE any assertion fires (lets the empirical-iteration loop capture both proxies in a single failing run, rather than iterating assertion-by-assertion). Matches the dispatch-level RUST_LOG-controlled tracing precedent at the 04.x family; doc-comment reframes from "temporary" to "leave-on diagnostic" because the future Task 14 + follow-on phases will want the same pattern. Mention of the env var is added to fixture 0014's README "Empirical allow-list seeding" section in a follow-on revision (not blocking — the env var is self-documenting at the lib.rs source).

7. **PLAN package name `-p envoy-rust-differential` is stub drift; used `-p differential`** per `tests/differential/Cargo.toml:2`. Same disposition as Task 10. Pre-flagged by the controller.

8. **PLAN's expectations.yaml sketch had `required_keys: [..., uptime_current_epoch_seconds, uptime_all_epochs_seconds, ...]`; that form would fire a `required_keys` assertion on the envoy side** (envoy emits the protobuf-canonical `uptime_current_epoch` / `uptime_all_epochs` Duration-string form, not envoy-rust's seconds-u64 names). This is NOT a Task 6/7 regression (which the PLAN's "STOP and report" guidance addresses) — it's a real envoy ↔ envoy-rust field-naming divergence the PLAN's sketch did not anticipate. Resolved via the symmetric per-side `allowlist_*_keys` + the 5 bilateral required keys (`state`, `version`, `node`, `hot_restart_version`, `command_line_options`) — the divergence is documented in fixture 0014's expectations.yaml + README.md `/server_info` sections. A follow-on phase can either (a) add Duration-string projection on envoy-rust's side, or (b) widen the BEHAVIOR_CONTRACT `/server_info` row to acknowledge the dual naming.

9. **PLAN's expectations.yaml sketch had `required_lines: ["<cluster_name>::observability_name::<cluster_name>", "<cluster_name>::default_priority::endpoints"]` for `/clusters`; envoy does NOT emit the second line** (envoy emits per-priority circuit-breaker counters like `backend::default_priority::max_connections::1024`, NOT the literal `endpoints` token). `backend::default_priority::endpoints` is envoy-rust-only (lock-in #10 — envoy-rust emits only the 2-line minimum). Moved to `allowlist_envoy_rust_only_lines`; only `backend::observability_name::backend` is required-bilateral.

10. **PLAN's expectations.yaml sketch had `required_lines: ["<listener_name>::0.0.0.0:{{PORT}}"]` for `/listeners`; expectations.yaml does NOT participate in `render_yaml`'s `{{PORT}}` substitution.** A literal `{{PORT}}` token would never match either side. Used `required_line_prefixes: ["ingress_http::"]` (matches both sides' shapes verbatim) + per-side `allowlist_*_line_prefixes` for the address+port suffix divergence. Same disposition would apply to any future expectation-shape that needs port template substitution; expectations.yaml stays declarative.

11. **PROGRESS narrative + Per-task append template line-number drift:** Task 10's narrative spans lines ~1026-1115 instead of the brief's "lines ~960-1115" estimate (Task 10 came in larger than expected). Task 11's narrative inserts at line ~1114 BEFORE the Per-task append template block at line ~1116 per the documented convention.

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
   Compiling differential v0.0.0 (/Users/esa/git/envoy-rust/tests/differential)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.73s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking differential v0.0.0 (/Users/esa/git/envoy-rust/tests/differential)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.45s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
test result: ok. 94 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.77s   # differential lib
test result: ok. 1 passed; 0 failed; ...   # 14 differential integration buckets (admin_config_dump_server_info, admin_stats_prometheus, admin_ready, access_log_file_sink, echo, http1_direct_response, http1_router_upstream, http2_direct_response, http2_router_upstream, http_filter_header_mutation, tcp_proxy, tls_downstream, tls_sni, tls_upstream — all 14 green simultaneously)
test result: ok. 58 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s   # envoy-admin lib
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s   # envoy-accesslog lib
... (all other workspace buckets green; full log at /tmp/workspace-final.log)
```

(`differential` lib bucket: 94 = 84 pre-task + 10 new. `differential` integration buckets: 14 simultaneously (was 13; +1 = fixture 0014 wrapper). `envoy-admin` bucket: 58, unchanged from Task 10 — Task 11 is a `tests/differential/`-only change at the production side. Full `cargo test --workspace` green across all buckets.)

`cargo deny check`:
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues. Task 11 introduces NO new top-level Cargo deps. `serde_json = "1"` (added by Task 10) is reused in the new strictness wiring. Cargo deny check quoted explicitly per 07.1-REVIEW doctrine reminder + project precedent.)

### Docker-gated bilateral run

Fixture 0014 wrapper (first iteration with empirically-seeded expectations.yaml):

```
running 1 test
[2026-05-16T12:50:01.863Z] INFO node registered node.id=envoy-rust-phase-08.1-fixture-0014 node.cluster=envoy-rust-phase-08.1
[2026-05-16T12:50:01.863Z] INFO envoy-rust listening (http_connection_manager) addr=127.0.0.1:63824 stat_prefix=ingress_http codec_type=HTTP1
[2026-05-16T12:50:01.863Z] INFO envoy-rust listening (admin) addr=127.0.0.1:63825
test admin_config_dump_server_info ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.05s
```

All 14 Docker-gated fixtures simultaneously green (`cargo test -p differential`, ~90 seconds end-to-end on local Docker Desktop 4.40.0). Empirical-iteration summary: 1 capture run (relaxed expectations + `DIFFERENTIAL_DUMP_ADMIN=1`) → seed all 4 sub-cases' allow-lists → 1 validation run (seeded expectations) → GREEN. Total empirical iterations: 2 (well within the brief's 5-iteration budget). Convergence was fast because the Task 10 + Task 11 strictness model + the new per-side line-prefix family + the bare-content-type allowance covered all empirical divergence categories without further harness churn.

---

## Task 12 — D17.3a — Fuzz corpus seed admin_multi_endpoint_bootstrap.yaml

**Commit:** `<sha-pending>` — `phase 08.1: task 12 — fuzz corpus seed (admin_multi_endpoint_bootstrap.yaml)`
**LoC delta:** +41 fixture (the new seed YAML), +1 fixture (`.gitignore` allow-line), +1 test (`bootstrap.rs` SUCCESS-array entry), +~95 doc (this PROGRESS narrative). Net +~138 insertions, 0 deletions. No production code change.

### Work summary

Landed a new fuzz-corpus seed at `crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_multi_endpoint_bootstrap.yaml` exercising the admin + multi-cluster shape (single listener with empty `filter_chains`, two STRICT_DNS clusters each with one DNS-named endpoint, plus the admin listener). Added the matching `.gitignore` allow-line and appended the seed's path to the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array. No new test function landed — Task 12 EXTENDS test data, not test logic. The seed broadens libFuzzer's structural coverage of the parse_bootstrap target into bootstrap shapes that pair the admin listener with multiple clusters (the existing `admin_with_stats_route.yaml` covers admin + one listener + zero clusters; the new seed covers admin + one listener + two clusters).

### Tests landed

- None. Task 12 extends the existing `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array (was 13 entries; now 14). The new seed becomes test data, not a new test function. The `envoy-config` lib bucket count is unchanged at 209.

### Deviations from PLAN

1. **PLAN sketch's `connect_timeout: 1s` removed.** The `Cluster` struct (`crates/envoy-config/src/bootstrap.rs:54-83`) uses `#[serde(deny_unknown_fields)]` and has no `connect_timeout` field — the PLAN's sketch as-written would yield an "unknown field" serde error. The field is omitted from the seed; the parser does not model connect timeouts at the phase-01 surface.

2. **PLAN sketch's `endpoints: []` replaced with one populated locality + lb_endpoint per cluster.** The validator at `bootstrap.rs:1215-1225` returns `ConfigError::EmptyClusterEndpoints` when `total_endpoints == 0`. The PLAN's empty-endpoints form would fail parse-time validation. Each cluster now carries a single `lb_endpoints` entry with a DNS-named address (`backend-a.local:7001` / `backend-b.local:7002`) to satisfy the non-empty-endpoints invariant while preserving the STRICT_DNS intent. Matches the schema convention seen in `strict_dns_cluster.yaml`.

3. **PLAN sketch's `lb_policy` was missing — added `ROUND_ROBIN` to each cluster.** The `Cluster` struct's `lb_policy` field is non-optional (`bootstrap.rs:60`, no `#[serde(default)]`). Without it, the parser yields a "missing field" serde error. `ROUND_ROBIN` is the only `LbPolicy` variant currently defined (`bootstrap.rs:118-120`) and matches every other existing seed.

4. **PLAN sketch's second listener (`listener_1`) removed.** The validator at `bootstrap.rs:1198-1202` caps listeners at 1 (`ConfigError::TooManyListeners`) per phase 01. The PLAN's "multi-listener" framing is not satisfiable as a SUCCESS-array seed. Reinterpreted "multi-endpoint" in the seed name as referring to the admin-endpoint surface (the same axis Task 11's fixture 0014 scrapes — `/config_dump` + `/server_info` + `/clusters` + `/listeners`) and to multi-cluster, both of which the parser accepts. The remaining single listener `listener_0` retains `filter_chains: []` (allowed — no validator rejects empty filter chains at this layer; the corresponding cluster-side endpoint validation already covers reachability invariants).

5. **PLAN's `cd crates/envoy-config && cargo fuzz run ...` snippet adjusted to `cd crates/envoy-config/fuzz && cargo +nightly fuzz run ...`.** Per ADR-0010 the fuzz subcrate is workspace-excluded and `cargo-fuzz` requires its own `Cargo.toml` directory; the invocation must execute from `crates/envoy-config/fuzz/`. Nightly toolchain prefix `+nightly` is required (cargo-fuzz instrumentation depends on nightly-only flags).

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
   Compiling envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
   Compiling http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
   Compiling envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
   Compiling http2-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 25.75s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
    Checking envoy-admin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-admin)
    Checking http1-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http1-echo-server)
    Checking envoy-bin v0.0.0 (/Users/esa/git/envoy-rust/crates/envoy-bin)
    Checking http2-echo-server v0.0.0 (/Users/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 24.95s
```

`cargo fmt --all -- --check`:
```
(no output; exit 0)
```

`cargo test --workspace`:
```
test result: ok. 209 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s   # envoy-config lib (unchanged count; Task 12 extends test data, not test logic)
test result: ok. 94 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.77s   # differential lib (unchanged from Task 11)
test result: ok. 58 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s   # envoy-admin lib (unchanged)
test result: ok. 1 passed; 0 failed; 0 ignored; ... (14 differential integration buckets — all green simultaneously, unchanged from Task 11)
... (all other workspace buckets green; full log via `cargo test --workspace`)
```

Bucket-level: SUCCESS-array test (`bootstrap::tests::fuzz_corpus_seeds_parse_or_reject_cleanly`) now exercises 14 SUCCESS seeds (was 13). Test bucket count is unchanged (test extends data, not logic).

`cargo deny check`:
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing unmatched license allowances per ADR-0005; no new advisories or license issues. Task 12 introduces NO new top-level Cargo deps — it is a fixture-only + 1-test-data-line + 1-gitignore-line change. Cargo deny check quoted explicitly per 07.1-REVIEW doctrine reminder + project precedent.)

### Short-budget fuzz run

`cd crates/envoy-config/fuzz && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`:
```
#368049	REDUCE cov: 12210 ft: 33665 corp: 3277/1780Kb lim: 4096 exec/s: 12268 rss: 578Mb L: 95/4089 MS: 2 InsertByte-EraseBytes-
#368115	REDUCE cov: 12210 ft: 33665 corp: 3277/1780Kb lim: 4096 exec/s: 12270 rss: 578Mb L: 47/4089 MS: 1 EraseBytes-
#368186	REDUCE cov: 12210 ft: 33665 corp: 3277/1780Kb lim: 4096 exec/s: 12272 rss: 578Mb L: 988/4089 MS: 1 EraseBytes-
#368607	NEW    cov: 12211 ft: 33673 corp: 3278/1782Kb lim: 4096 exec/s: 12286 rss: 578Mb L: 2015/4089 MS: 1 InsertByte-
#368898	NEW    cov: 12211 ft: 33678 corp: 3279/1786Kb lim: 4096 exec/s: 12296 rss: 578Mb L: 3877/4089 MS: 1 ChangeBit-
#370923	DONE   cov: 12211 ft: 33678 corp: 3279/1786Kb lim: 4096 exec/s: 11965 rss: 578Mb
###### Recommended dictionary. ######
"\013\000\000\000\000\000\000\000" # Uses: 26509
"\001\200" # Uses: 2116
"\377b" # Uses: 419
###### End of recommended dictionary. ######
Done 370923 runs in 31 second(s)
```

0 crashes in 31 seconds (370 923 runs). Coverage ended at 12 211 PCs / 33 678 features / corp 3 279 / 1786 KiB. The new seed contributed structural variation in the admin+multi-cluster region of the input space; libFuzzer's persistent corpus (`crates/envoy-config/fuzz/corpus/parse_bootstrap/`, untracked beyond the curated allow-list) absorbed mutations off the new seed plus the existing 13 seeds without incident.

---

## Per-task append template

For each task commit, append the following block:

```markdown
## Task N — <subject from PLAN>

**Commit:** `<sha>` — `<commit message subject line>`
**LoC delta:** +X production, +Y tests, +Z fixture/doc. Net +W.

### Work summary

<1-3 sentences narrating what landed.>

### Tests landed

- `<test_name_1>` (production crate `<name>`)
- `<test_name_2>` ...

### Deviations from PLAN

<None | enumerated deviations with rationale.>

### 5-gate test-bucket attestation

`cargo build --workspace --all-targets`:
```
<quoted tail>
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
<quoted tail>
```

`cargo fmt --all -- --check`:
```
<quoted tail>
```

`cargo test --workspace`:
```
<quoted tail>
```

`cargo deny check`:
```
<quoted tail>
```
```

The state-4 PROGRESS entry (Task 14) additionally includes:

- CI run URL + HEAD SHA + conclusion + completion timestamp.
- Per-fixture green status (14 fixtures: 0001-0014).
- h2spec pass-rate at the ≥95% gate (carry-forward; 08.1 engages no H2-framing surfaces).
- Short-budget fuzz target output (`parse_bootstrap`).
