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
