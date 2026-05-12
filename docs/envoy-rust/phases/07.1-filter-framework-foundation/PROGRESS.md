# Phase 07.1 (`07.1-filter-framework-foundation`) — PROGRESS

> Per-task narrative log appended at each substantive commit.
> Stranger-readable per D-3.4. PROGRESS.md is CREATED at Task 1's
> commit (NOT at the state-2 standalone PLAN.md commit — divergence
> from the 06.1/06.2/06.3 cadence; the 07.1 SPEC §8 cadence is "PLAN.md
> + STATE.md advance ONLY at state-2; PROGRESS lands at Task 1").

## Task 1 — `crates/envoy-filter/` scaffold + `FilterError` typed-error enum

### Work summary

Landed the new workspace member `crates/envoy-filter/` with `lib.rs` +
`error.rs` only (the strict module-per-task split per 07.1 SPEC §5
signpost 1 + PLAN architecture decision 1). `FilterError` enum with 4
variants (`EmptyChain`, `RouterNotTerminal`, `DuplicateRouter`,
`UnsupportedFilterType`) covers the framework's parse-time and
build-time invariants; the validator at envoy-config (Task 4) is the
earlier-layer catch and these are defense-in-depth at the framework
crate boundary.

Cargo.toml dependencies are existing workspace foundations only
(`bytes = "1"`, `thiserror = "2"`, `tracing = "0.1"`) plus the two
workspace path deps (`envoy-config`, `envoy-http1`). No new top-level
Cargo deps; `cargo deny check` remains a no-op for 07.1.

### Tests landed

5 unit tests at `crates/envoy-filter/src/error.rs::tests`:
- `display_empty_chain_is_human_readable`
- `display_router_not_terminal_includes_position_and_name`
- `display_duplicate_router_includes_position`
- `display_unsupported_filter_type_includes_position_and_name`
- `filter_error_is_send_sync_static`

### LoC delta

| File | LoC |
|---|---|
| `crates/envoy-filter/Cargo.toml` | ~12 |
| `crates/envoy-filter/src/lib.rs` | ~10 |
| `crates/envoy-filter/src/error.rs` | ~75 (incl. 5 tests) |
| `Cargo.toml` (workspace root) | +1 line |
| `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md` | ~40 |
| **Total** | **~138** |

### Deviations from PLAN

**Deviation 1 (PLAN.md:209 — edition pin):** PLAN.md prescribed
`edition = "2021"` for the new `crates/envoy-filter/Cargo.toml`. Every
other workspace crate (envoy-accesslog, envoy-admin, envoy-bin,
envoy-cluster, envoy-config, envoy-http1, envoy-http2, envoy-listener,
envoy-stats, envoy-tcp, envoy-tls — verified via `grep '^edition'
crates/*/Cargo.toml`) uses `edition = "2024"`. Landed the new crate at
`edition = "2024"` to match project convention. Recorded per the
PLAN's invitation at lines 42-54 + 466-471 to surface empirical
PLAN-write corrections at Task 1.

### Test-bucket attestation

- `cargo test -p envoy-filter`: PASS (5 tests).
  ```
  running 5 tests
  test error::tests::display_router_not_terminal_includes_position_and_name ... ok
  test error::tests::display_duplicate_router_includes_position ... ok
  test error::tests::display_unsupported_filter_type_includes_position_and_name ... ok
  test error::tests::display_empty_chain_is_human_readable ... ok
  test error::tests::filter_error_is_send_sync_static ... ok

  test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- `cargo build --workspace --all-targets`: clean.
  ```
  Compiling envoy-filter v0.1.0 (/Users/esa/git/envoy-rust/crates/envoy-filter)
      Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.14s
  ```
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
  ```
  Checking envoy-filter v0.1.0 (/Users/esa/git/envoy-rust/crates/envoy-filter)
      Finished `dev` profile [unoptimized + debuginfo] target(s) in 29.62s
  ```
- `cargo fmt --all -- --check`: clean (no output).
- `cargo test --workspace`: PASS. All suites passing; envoy-filter contributes 5 new tests.
  ```
  running 5 tests
  test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- `cargo deny check`: no-op (no new top-level deps).

## Task 2 — `FilterPipeline` + `Decision` enum + iteration-loop skeleton

### Work summary

Landed `crates/envoy-filter/src/pipeline.rs` with the `FilterPipeline`
struct + `Decision::{Continue, StopAndSend}` enum + `build_from_config`
constructor + `decode_headers` (declaration-order walk) + `encode_headers`
(reverse-declaration-order walk per parent-07 SPEC §6 Rule 6).

Per architecture decision 15, also landed a placeholder
`crates/envoy-filter/src/instance.rs` with a single-variant
`HttpFilterInstance::Router` (zero-state placeholder). Task 3 will
replace the placeholder with the real `Router(RouterTerminus)` variant
+ separate `router.rs` module.

### Tests landed

4 unit tests at `crates/envoy-filter/src/pipeline.rs::tests`:
- `build_from_config_rejects_empty_list`
- `build_from_config_with_single_router_succeeds`
- `decode_headers_on_single_router_returns_continue`
- `encode_headers_on_single_router_returns_continue`

Total envoy-filter test count: 5 (Task 1) + 4 (Task 2) = 9.

### LoC delta

| File | LoC |
|---|---|
| `crates/envoy-filter/src/pipeline.rs` | ~140 (incl. 4 tests + 2 test helpers) |
| `crates/envoy-filter/src/instance.rs` (placeholder) | ~37 |
| `crates/envoy-filter/src/lib.rs` (extension) | +6 |
| `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md` (Task 2 entry) | ~55 |
| **Total** | **~238** |

### Deviations from PLAN

**Deviation 1 (PLAN.md:529, 691 — Response import path):** PLAN
prescribed `use envoy_http1::codec::{Request, Response};`. On disk
`Response` lives in `crates/envoy-http1/src/response.rs`, not `codec.rs`
(per `crates/envoy-http1/src/lib.rs:24,27` crate-root re-exports). Used
`use envoy_http1::{Request, Response};` in both `pipeline.rs` and
`instance.rs`.

**Deviation 2 (PLAN.md:660-667 — `test_request` field shape):** PLAN
listed 4 fields. Disk `Request` struct
(`crates/envoy-http1/src/codec.rs:20-46`) has 6 fields:
`version: HttpVersion`, `bytes_consumed: usize`, and
`body: Option<bytes::Bytes>` (not `bytes::Bytes` directly). Adjusted
helper to construct all 6.

**Deviation 3 (PLAN.md:669-675 — `test_response` field shape):** PLAN
listed 3 fields. Disk `Response`
(`crates/envoy-http1/src/response.rs:13-19`) has 4: added
`reason: None` field.

**Deviation 4 (PLAN.md:704 — `position` parameter):** Renamed
`position: usize` → `_position: usize` in placeholder
`HttpFilterInstance::build` to avoid `unused_variables` warning under
`cargo clippy -- -D warnings`. Task 3 may rename back when the
parameter becomes load-bearing.

### Test-bucket attestation

- `cargo test -p envoy-filter`: PASS (9 tests).
  ```
  running 9 tests
  test error::tests::display_router_not_terminal_includes_position_and_name ... ok
  test error::tests::display_empty_chain_is_human_readable ... ok
  test error::tests::display_duplicate_router_includes_position ... ok
  test error::tests::display_unsupported_filter_type_includes_position_and_name ... ok
  test pipeline::tests::build_from_config_with_single_router_succeeds ... ok
  test error::tests::filter_error_is_send_sync_static ... ok
  test pipeline::tests::build_from_config_rejects_empty_list ... ok
  test pipeline::tests::decode_headers_on_single_router_returns_continue ... ok
  test pipeline::tests::encode_headers_on_single_router_returns_continue ... ok

  test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- `cargo build --workspace --all-targets`: clean.
  ```
  Compiling envoy-filter v0.1.0 (/Users/esa/git/envoy-rust/crates/envoy-filter)
      Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.22s
  ```
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
  ```
  Checking envoy-filter v0.1.0 (/Users/esa/git/envoy-rust/crates/envoy-filter)
      Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.09s
  ```
- `cargo fmt --all -- --check`: clean (no output after `cargo fmt` applied rustfmt normalizations).
- `cargo test --workspace`: PASS. All suites passing; envoy-filter contributes 9 tests.
  ```
  test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- `cargo deny check`: no-op (no new top-level deps).
