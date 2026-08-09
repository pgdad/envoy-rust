# Sub-phase 108.2 — implementation progress (§5 state 3)

> One entry per PLAN.md task, appended by the state-3 implementation session
> as each task lands (TDD-first, one commit per task). PLAN.md §0's pre-flight
> numbers were treated as CLAIMS and re-established here.

## Task 1 — `AdminEndpoint::Runtime` (envoy-admin)

- **RED (verified):** with the two test-support consts
  (`RUNTIME_TWO_LAYER_BOOTSTRAP`, `RUNTIME_SCALARS_BOOTSTRAP`) and the 6-test
  `mod runtime_tests` appended, `cargo test -p envoy-admin runtime_tests`
  failed to compile with exactly the predicted forcing error, 4×:
  `error[E0599]: no variant or associated item named 'Runtime' found for enum
  'endpoint::AdminEndpoint'`.
- **GREEN:** after the variant + the three dispatch arms (`from_path`,
  `allowed_method` — first compile-forcing site, `render_with` — second) +
  `RuntimeBody`/`RuntimeEntryBody`/`render_runtime` (via `json_pretty_200`,
  no new response plumbing) + the two convention-test rows
  (`get_known_path_returns_endpoint`, `each_endpoint_declares_its_allowed_method`):
  `cargo test -p envoy-admin` → `test result: ok. 103 passed; 0 failed`
  (97 baseline + 6 new — matches the plan's stated count).
- Both new consumers call `RuntimeSnapshot::from_bootstrap` only (DD-2 / M-5);
  the M-6 positive `/config_dump` pin
  (`config_dump_serializes_layered_runtime_positively`) landed in this task.
- **Boundary gate:** `cargo fmt --all -- --check` clean *after* one
  `cargo fmt --all` pass — the PLAN's literal Rust was NOT fully fmt-canonical
  as transcribed: rustfmt reflowed 3 long lines in the new tests
  (`!resp.headers.iter().any(...)`, the `/runtime_modify` `assert_eq!`, the
  `.get("layered_runtime").is_none()` chain). Formatting-only deviation,
  behavior identical. `cargo clippy --workspace --all-targets --all-features
  -- -D warnings` exit 0 with 2 `Checking` lines (non-zero → not a cached
  no-op).
- **Anchors:** re-derived by text pre-edit; `endpoint.rs` was 3091 lines at
  task start (identical to the plan's measurement — no drift since `ced6802`).
- **Commit:** `phase 108.2 task 1: admin GET /runtime — the eleventh endpoint`

## Task 2 — the nine `runtime.*` stats (envoy-bin)

- **RED (verified, with a shape correction):** with
  `crates/envoy-bin/src/runtime_stats.rs` created but NOT declared in
  `main.rs`, `cargo test -p envoy-bin --bin envoy-bin runtime_stats` printed
  `test result: ok. 0 passed; 0 failed; ... 37 filtered out` — NOT the plan's
  predicted `E0583`: an undeclared `.rs` file is simply never compiled, so
  there is no error, only the absence of the tests. Per the standing
  `0 passed; N filtered out` trap this is a false GREEN by exit code and the
  true RED by count: the two tests did not exist for the build.
- **GREEN:** after `mod runtime_stats;` (following `mod network_rbac;`) and
  the `register_runtime_stats(&bootstrap, &registry)` call directly after
  `register_rds_stats` in `main.rs`:
  `cargo test -p envoy-bin --bin envoy-bin runtime_stats` →
  `test result: ok. 2 passed; 0 failed` (the plan's stated count).
- Kinds pinned per DD-6 (4 gauges / 5 counters); both value tables asserted;
  `num_keys` flattened-leaf semantics witnessed via `nested.deep` (4, not 3);
  registration calls `RuntimeSnapshot::from_bootstrap` only (DD-2).
- **Boundary gate:** fmt needed one reflow pass again (the `num_layers`
  assert line in the loop — same formatting-only deviation class as Task 1);
  then `cargo fmt --all -- --check` clean; clippy exit 0, 1 `Checking` line
  (non-zero).
- **Commit:** `phase 108.2 task 2: the nine runtime.* stats`

## Task 3 — `expected_stats` harness extension + fixture `0087` data files

- **RED (verified):** with the parse test
  (`fixture_0087_expectations_parses_as_admin_scrape_with_expected_stats`)
  inserted and the three fixture data files written,
  `cargo test -p differential --lib fixture_0087` failed with exactly the
  predicted `error[E0026]: variant 'Driver::AdminScrape' does not have a
  field named 'expected_stats'`.
- **GREEN:** after the four lib.rs edits — (a) the `#[serde(default)]`
  `expected_stats: Vec<KeepAliveExpectedStat>` field, (b) the dispatch
  destructure + pass-through, (c) the widened 8-arg `run_admin_scrape_arm`
  with `#[allow(clippy::too_many_arguments)]` + justification (DD-7), (d)
  the STEP 3.5 `assert_expected_stats_bilaterally` call between the scrape
  loop and STEP 4 — `cargo test -p differential --lib` →
  `test result: ok. 162 passed; 0 failed; 2 ignored`. **Count-mismatch
  investigated and resolved:** the plan's "164" is the TOTAL (the 2 ignored
  are pre-existing Docker-gated tests); measured baseline at the task-2
  commit is `161 passed; 2 ignored` = 163 total, so the delta is exactly the
  +1 new parse test (163 → 164 total). The new test passes
  (`1 passed; 163 filtered out` on the name filter).
- Fixture `0087` data files carry the plan's measured transcript verbatim:
  14 entries (every scalar a quoted YAML string), `layers`
  `["base_layer","override_layer"]`, nine `expected_stats` with exactly the
  four non-zero witnesses first, subtree anchors at single-segment `entries`
  / `layers` only (DD-8). `envoy-rust.yaml` uses the NAME-ONLY echo spelling
  (DD-3); zero clusters, zero backends.
- **Boundary gate:** fmt clean on first check (no reflow this time); clippy
  exit 0, 1 `Checking` line (non-zero).
- **Commit:** `phase 108.2 task 3: AdminScrape expected_stats + fixture 0087 data`

## Task 4 — differential test binary + the LOCAL fixture run + mutation checks

- `tests/differential/tests/runtime_static_layer.rs` created (the 87th
  differential test file / 164th workspace test binary).
- `cargo build -p envoy-bin` FIRST (fresh debug binary — the stale-binary trap).
- **The SPEC §5 LOCAL RUN, RECORDED:**
  `cargo test -p differential --test runtime_static_layer -- --nocapture` →
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered
  out; finished in 7.88s` (first run; container start included). Subsequent
  runs 1.15-1.22 s — inside the plan's ~1-3 s window. Backend-free and
  cluster-free, so fully verifiable on this host; Docker daemon was up
  (`docker ps` clean). Transcript at scratchpad `fixture-0087-run1.log`.
- **Mutation checks (in-place data mutations, serial, no parallel subagents):**
  - (a) `shared.key.final_value` `"from_override"` → `"from_base"`: RED with
    a real `test result: FAILED. 0 passed; 1 failed` line and the failure
    text naming the witness — `required_subtree "entries" envoy != expected`
    (the UPSTREAM side rejected the mutated expectation; the assertion
    reaches a real assertion, not a compile/startup failure).
  - (b) revert (a); `runtime.num_keys` `value: 14` → `13`: RED with
    `upstream stat runtime.num_keys expected 13 got 14` — the wrong
    flattened-leaf count fails loudly (bilateral: upstream checked first).
  - (c) revert (b); `git diff --stat` EMPTY (byte-exact restore of the
    tracked expectations.yaml); rerun → GREEN `1 passed` (the unmutated
    control from the same tree).
- **Boundary gate:** rustfmt reflowed the `run_fixture(...).await.expect(...)`
  chain in the new test file (same formatting-only deviation class as Tasks
  1-2); after `cargo fmt --all`: check clean, fixture still green
  (`1 passed`, 1.15s). Clippy exit 0, 1 `Checking` line.
- **Commit:** `phase 108.2 task 4: fixture 0087 differential test + local green`

## Task 5 — `BEHAVIOR_CONTRACT.md` `## Runtime` + admin row + stat mapping + fixture README

- Three contract edits, all located BY TEXT: the `## Runtime` section
  inserted immediately before `## xDS wire state machine`; the `/runtime`
  row appended to `## Admin endpoint body shapes` after the
  `/healthcheck/ok` row; the `**108.2 entries:**` three-row block appended
  at the end of `## Stat-name mapping` (before its closing `---`). All three
  texts transcribed verbatim from PLAN.md Task 5.
- `tests/fixtures/0087-runtime-static-layer/README.md` created (the witness
  ledger, the exclusion list, and the measurement provenance).
- The fixture `0011` prose correction is RECORDED in the contract section
  (fixture NOT edited — D-3.5).
- **Insurance run:** `cargo test -p differential --test runtime_static_layer`
  still `1 passed` after the docs-only edits.
- **Commit:** `phase 108.2 task 5: BEHAVIOR_CONTRACT ## Runtime + fixture 0087 README`
