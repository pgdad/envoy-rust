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

## Task 3 — `HttpFilterInstance` enum (Router-only) + `RouterTerminus`

### Work summary

Replaces Task 2's placeholder `crates/envoy-filter/src/instance.rs`
with the real Router-payload variant
`HttpFilterInstance::Router(RouterTerminus)`. Creates new module
`crates/envoy-filter/src/router.rs` with `RouterTerminus` struct
(zero-state; derives `Debug + Clone + Default`; `pub(crate) fn new()`
constructor; `decode_headers` + `encode_headers` both return
`Decision::Continue` without mutating req/resp).

Router is the terminus of every filter chain per parent-07 SPEC §6
Rule 3. The validator at envoy-config (Task 4) enforces Router-at-last
at config-load time. The iteration semantic — decode walks Router LAST;
reverse-encode walks Router FIRST — models Envoy's "Router produces the
response, other filters mutate on encode" shape.

### Tests landed

3 unit tests at `crates/envoy-filter/src/router.rs::tests`:
- `decode_headers_returns_continue_and_does_not_mutate_request` — full
  6-field snapshot before/after; verifies no mutation.
- `encode_headers_returns_continue_and_does_not_mutate_response` — full
  4-field snapshot before/after; verifies no mutation.
- `router_terminus_is_clone_and_default` — Default + Clone symmetry.

1 unit test at `crates/envoy-filter/src/instance.rs::tests`:
- `build_router_succeeds` — verifies the Router arm of
  `HttpFilterInstance::build` produces the right variant.

Total envoy-filter test count: 5 (Task 1) + 4 (Task 2) + 1 + 3 = 13.

### LoC delta

| File | LoC |
|---|---|
| `crates/envoy-filter/src/router.rs` (new) | ~95 (incl. 3 tests) |
| `crates/envoy-filter/src/instance.rs` (rewrite from placeholder) | ~65 (incl. 1 test) |
| `crates/envoy-filter/src/lib.rs` (extension) | +2 |
| PROGRESS.md (Task 3 entry) | ~55 |
| **Total** | **~217** |

### Deviations from PLAN

**Deviation 1 (PLAN.md:909, 999 — Response import path):** PLAN
prescribed `use envoy_http1::codec::{Request, Response};` in both
`router.rs` and `instance.rs`. Disk has `Response` in
`crates/envoy-http1/src/response.rs`, not `codec.rs`. Used
`use envoy_http1::{Request, Response};` (crate-root re-exports).

**Deviation 2 (PLAN.md:939-944 — `decode_headers` test Request shape):**
PLAN's Request literal listed 4 fields; disk has 6
(`crates/envoy-http1/src/codec.rs:20-46`). Added `version`,
`bytes_consumed`, and used `Some(Bytes::from_static(...))` for `body`.
Expanded the 4-tuple snapshot to a 6-tuple and the 4 post-call
assertions to 6.

**Deviation 3 (PLAN.md:962-966 — `encode_headers` test Response shape):**
PLAN's Response literal omitted the `reason` field; disk has 4
(`crates/envoy-http1/src/response.rs:13-19`). Added `reason: None` and
extended the 3-tuple snapshot to 4 plus the matching extra assertion.

**Deviation 4 (rustfmt — `encode_headers` test `before` tuple):** PLAN
wrote the `before` 4-tuple on one line. `rustfmt` expanded it to
multi-line (line-length limit). Applied before final commit to keep
`cargo fmt --check` clean.

### Test-bucket attestation

- `cargo test -p envoy-filter`: PASS (13 tests).
  ```
  test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- `cargo build --workspace --all-targets`: clean.
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s
  ```
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
  ```
  Checking envoy-filter v0.1.0 (/Users/esa/git/envoy-rust/crates/envoy-filter)
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.10s
  ```
- `cargo fmt --all -- --check`: clean (no output).
- `cargo test --workspace`: PASS. All suites passing; no failures.
  ```
  test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- `cargo deny check`: no-op (no new top-level deps).

## Task 4 — `envoy-config` terminal-router validator + 3 new `ConfigError` variants

### Work summary

Replaces the pre-07.1 cardinality gate at `crates/envoy-config/src/bootstrap.rs:1335-1347` (the 13-line `match hcm.http_filters.len()` block) with a single call `validate_http_filters(&hcm.http_filters, listener_name)?`. Adds new free function `validate_http_filters(filters: &[HttpFilter], listener_name: &str) -> Result<(), ConfigError>` enforcing: (a) at least one filter, (b) exactly one Router, (c) Router at terminus, (d) name/typed_config consistency.

Adds 3 new `ConfigError` variants to `crates/envoy-config/src/lib.rs`:
`EmptyHttpFilters { listener }`, `RouterNotTerminal { listener, position }`, `DuplicateRouterFilter { listener }`. Retains the existing `MultipleHttpFilters` variant per signpost 13 (no longer constructed; doc-comment supersession note added). The pre-existing `UnsupportedHttpFilter` variant continues firing on name/typed_config mismatch.

`validate_hcm` signature unchanged — `listener_name: &str` is already a parameter (since 06.3's Http2ClusterFromHttp1Listener listener-name-threading); no caller updates needed.

### Tests landed

7 new unit tests at `crates/envoy-config/src/bootstrap.rs::tests`:
- `validate_http_filters_accepts_single_router`
- `validate_http_filters_rejects_empty_list`
- `validate_http_filters_rejects_duplicate_router`
- `validate_http_filters_rejects_name_typed_config_mismatch`
- `validate_http_filters_listener_name_propagates`
- `validate_http_filters_duplicate_router_takes_precedence_over_router_not_terminal`
- `validate_http_filters_accepts_existing_fixture_shape`

Step 8 (amend existing tests) confirmed no-op: `grep -rn MultipleHttpFilters crates/envoy-config/src/` returns only the variant definition in lib.rs. The existing `rejects_unsupported_http_filter` test (bootstrap.rs:3640) continues asserting `UnsupportedHttpFilter` unchanged.

### LoC delta

| File | LoC |
|---|---|
| `crates/envoy-config/src/lib.rs` (3 new variants + 1 doc-comment) | +35 |
| `crates/envoy-config/src/bootstrap.rs` (validate_http_filters function) | +60 |
| `crates/envoy-config/src/bootstrap.rs` (replace cardinality gate) | -12 +2 |
| `crates/envoy-config/src/bootstrap.rs` (7 new tests) | ~140 |
| `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md` | ~55 |
| **Total** | **~280** |

### Deviations from PLAN

None. `validate_hcm`'s signature already had `listener_name: &str` (confirmed at Step 1, verified at bootstrap.rs:1299-1304). No caller updates were needed. PLAN Step 6 worry was moot as predicted.

### Test-bucket attestation

- `cargo test -p envoy-config validate_http_filters`: PASS (7 tests).
  ```
  test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 180 filtered out; finished in 0.00s
  ```
- `cargo test -p envoy-config`: PASS.
  ```
  test result: ok. 187 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  ```
- `cargo test --workspace`: PASS.
  ```
  Finished `test` profile [unoptimized + debuginfo] target(s) in 0.00s (all suites clean)
  ```
- `cargo build --workspace --all-targets`: clean.
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.53s
  ```
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.53s
  ```
- `cargo fmt --all -- --check`: clean (no output after applying fmt).
- `cargo deny check`: no-op (no new top-level deps).

## Task 5 — H1 HCM 5-writer-arm refactor (factor wire-write to unified site)

### Work summary

Pure refactor — NO filter invocation yet (Task 6 layers that on top).

`crates/envoy-http1/src/hcm.rs::serve_connection` now declares `let outgoing: Response;` above the writer-arm match. Each of the 5 wire-write sites (1 outer Synth arm + 4 nested paths inside the Proxy arm — see Deviation 4) is converted from inline wire-write to `outgoing = <constructed_response>;`. Below the match, a unified factored site derives the access-log/counter locals (`response_status_for_log`, `response_body_len`, `response_headers_for_log`) from `outgoing` and fires a single `Http1Response::write_to(&outgoing, &mut downstream).await?` call. The 06.3 per-class HCM counter site + 06.2 access-log dispatch site are unchanged downstream of the unified site.

The pre-Task-5 late-init-with-`mut` posture on the three log-locals (06.3 Task 4 / 06.2 REVIEW I1) is preserved in spirit but shifted: `outgoing` carries the compile-time `E0381` regression guarantee (any arm that fails to populate it produces a compile error). The three derived locals are now `let x = …;` initializers at the unified site, which satisfies clippy's `needless_late_init` lint.

`crates/envoy-http1/src/router.rs` factors `construct_proxied_response` out of `write_proxied_response` (same body minus the wire-write step; returns Response value; takes the `close: bool` flag — see Deviation 3). The cluster-side `upstream_rq_total` / `upstream_rq_5xx` increments (06.3 D15.3.c) move into `construct_proxied_response` so they fire once per construction regardless of how the response is subsequently written. `write_proxied_response` retained as a thin wrapper (existing tests and any external callers continue to work unchanged).

### Tests landed

3 new unit tests at `crates/envoy-http1/src/router.rs::tests`:
- `construct_proxied_response_returns_response_with_status_200`
- `construct_proxied_response_increments_upstream_rq_total_only_once`
- `construct_proxied_response_increments_upstream_rq_5xx_on_503`

### LoC delta

| File | LoC delta |
|---|---|
| `crates/envoy-http1/src/hcm.rs` (writer-arm refactor + unified site) | +43 / -73 |
| `crates/envoy-http1/src/router.rs` (construct_proxied_response factoring + wrapper rewrite + 3 new tests) | +111 / -7 |
| PROGRESS.md (Task 5 entry) | ~85 |
| **Total** | **~320** |

### Deviations from PLAN

**Deviation 1 (PLAN.md:1693 — `outgoing` type):** PLAN said `let mut outgoing: Http1Response;`. Disk `Http1Response` is a unit namespace struct (response.rs:21); the response value type is `Response` (response.rs:13). Used `let outgoing: Response;` (not `mut` — late-init flow analysis correctly handles 5-arm initialization without `mut`).

**Deviation 2 (PLAN.md:1706, 1793 — `write_to` arity):** PLAN listed `write_to(&resp, &mut downstream, close)`. Disk signature is `write_to(resp: &Response, w: &mut W) -> Result<...>` taking 2 args (response.rs:26). The `close` flag is baked into the `Response` value's Connection header via synth/construct helpers. Final unified call: `Http1Response::write_to(&outgoing, &mut downstream).await?`.

**Deviation 3 (PLAN.md:1632-1636 — `construct_proxied_response` signature):** PLAN omitted the `close: bool` parameter. The existing `write_proxied_response` uses close to set `Connection: close|keep-alive` (router.rs:148-152); bit-equivalent emission requires the factored function take close too. Final signature: `construct_proxied_response(cluster, upstream_response, elapsed_ms, close) -> Response`.

**Deviation 4 (PLAN.md:1561-1564 — "5 sibling writer arms" mental model):** Actual control flow is 1 outer `BuildOutcome::Synth` arm + 4 nested paths inside `BuildOutcome::Proxy` (success / send-fail-502 / connect-fail-502 / no-endpoint-503). The refactor produces 5 `outgoing = …` assignment sites distributed across the nested structure, not 5 sibling match arms.

**Deviation 5 (PLAN.md:1740 — `upstream_host_for_log`):** PLAN said `Some(cluster.upstream_host_string())`. Disk uses `Some(endpoint.to_string())` at hcm.rs (inside `if let Some(endpoint)`). Preserved verbatim — this line is OUTSIDE the writer-arm assignments to `outgoing` and feeds 3 of the 4 proxy nested paths uniformly; the no-endpoint-503 path inherits `None` from the initial declaration. No change to this line needed.

**Deviation 6 (PLAN.md:1574 — test import):** PLAN said `use envoy_http1::codec::Response;`. The new tests live INSIDE `crates/envoy-http1/src/router.rs::tests`; reused existing `use super::*;` + `mk_test_cluster()` / `upstream()` helpers. Also note: existing tests use `Counter::value()` (not `Counter::get()` as PLAN suggested) — the new tests follow the existing convention.

**Deviation 7 (clippy `needless_late_init` on the 3 log-locals):** After the refactor pulls all initialization to one site, clippy flagged the late-init posture on `response_status_for_log` / `response_body_len` / `response_headers_for_log`. Resolved by converting them to `let x: T = …;` initializers at the unified site, and replacing the late-init explanation comment block with a comment on `outgoing` itself (which now carries the 5-arm-init compile-time guarantee). The semantic posture is preserved — any arm that fails to populate `outgoing` produces an E0381.

### Test-bucket attestation (per architecture decision + 06.3 REVIEW I1 closure)

- `cargo test -p envoy-http1`: PASS — 60 tests (3 new on `construct_proxied_response` + 57 pre-existing).
  ```
  test result: ok. 60 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
  ```
- `cargo test -p envoy-bin` (in-process backstop for fixtures 0001-0012 at `crates/envoy-bin/tests/http1_*.rs` and `http2_*.rs`): PASS — all suites green (see workspace summary).
- `cargo test --workspace`: PASS — 548 passed, 0 failed, 2 ignored across all crates.
- `cargo build --workspace --all-targets`: clean.
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.49s
  ```
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.43s
  ```
- `cargo fmt --all -- --check`: clean (no output after applying fmt).
- `cargo deny check`: pre-existing license-not-encountered warnings on `Unicode-DFS-2016` / `Zlib` (unrelated to this task — same on `main`). No new top-level deps.

Docker-gated bilateral attestation deferred to Task 8 (state-4 anchor); the in-process backstop tests are the surrogate at Task 5.
