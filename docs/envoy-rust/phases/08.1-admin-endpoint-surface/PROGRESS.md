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
