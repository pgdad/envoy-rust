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
