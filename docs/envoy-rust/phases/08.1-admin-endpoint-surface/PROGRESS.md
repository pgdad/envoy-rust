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

_(empty; populated by the state-3 execution arc)_

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
