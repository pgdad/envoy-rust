# Phase 00 Progress

## Task 1 — ADR-0002 (2026-04-23)
- Commit: 3fd0a97
- Change: appended ADR-0002 (GitHub Actions as CI provider) to DECISIONS.md
- Verification: `grep -q '^## ADR-0002' DECISIONS.md` → exit 0

## Task 2 — ADR-0003 (2026-04-23)
- Commit: 95839ba
- Change: appended ADR-0003 (Rust edition 2024) to DECISIONS.md
- Verification: `grep -q '^## ADR-0003' DECISIONS.md` → exit 0

## Task 3 — ADR-0004 + ENVOY_TARGET.md (2026-04-23)
- Commit: 9f5d1d2
- Change: ENVOY_TARGET.md populated with v1.33.0 pin (multi-arch index digest sha256:56da5a…70c2, proto tree commit b0f43d6); ADR-0004 appended
- Verification: grep checks for ADR-0004, sha256:, Proto tree commit: all exit 0; no `TBD` in either file
- Deviation: local Docker daemon has an IPv6 routing bug; digest resolved via Docker Hub public API (https://hub.docker.com/v2/repositories/envoyproxy/envoy/tags/v1.33.0) instead of `docker inspect`. Value is the canonical multi-arch manifest-index digest — equivalent to what `docker inspect` would report against a freshly-pulled manifest.

## Task 4 — Workspace scaffolding (2026-04-23)
- Commits: 171376d (ADR-0005), d455515 (skeleton)
- Change: created `crates/envoy-bin/{Cargo.toml,src/main.rs}` and `tests/differential/{Cargo.toml,src/lib.rs}` skeletons; populated root workspace `members`; fixed `deny.toml` per ADR-0005 (wrappers on hyper/hyper-util/tower-service, advisory ignores for RUSTSEC-2025-0111 and RUSTSEC-2025-0134)
- Verification: `cargo build --workspace --all-targets` → 0; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → 0; `cargo fmt --all -- --check` → 0; `cargo test --workspace` → 0 (0 tests in both crates); `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`
- Deviation: PLAN Task 4 Step 6 prescribed `skip-tree = [{ name = "testcontainers" }]` as the mechanism for exempting the bollard→hyper/tower transitive chain. Empirical testing against cargo-deny 0.19.4 shows `skip-tree` only affects the `multiple-versions` check, not `[bans] deny`. Landed ADR-0005 to document the correct mechanism (`wrappers` per deny entry) and the two RustSec advisory ignores on the dev-only testcontainers chain.

## Task 5 — envoy-bin config parser (2026-04-23)
- Commit: a7461bf
- Change: `crates/envoy-bin/src/config.rs` with `Bootstrap`/`StaticResources`/`Listener`/`Address`/`SocketAddress`/`FilterChain`/`NetworkFilter` types + `parse_bootstrap()` + `validate()` + `ECHO_FILTER` const + 5 unit tests. `mod config;` registered in `main.rs` with a scoped `#[allow(dead_code)]` (removed in Task 8 when `run()` consumes the module).
- Verification: `cargo test -p envoy-bin --bin envoy-bin config` → `5 passed`; `cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings` → 0; `cargo fmt -p envoy-bin -- --check` → 0 (rustfmt autofixed the plan's verbatim line-breaks on `parse_bootstrap` and the first test; behavior unchanged).
- Deviation: PLAN Task 5 Step 3 uses `cargo test -p envoy-bin --lib config`, but `envoy-bin` is a binary-only crate (no `[lib]` target); the tests were run via `--bin envoy-bin` with the same selector. Also, PLAN's clippy step `-D warnings` fails on dead_code for items defined in Task 5 but not consumed until Task 8 — resolved with a single `#[allow(dead_code)]` on the `mod config;` declaration in `main.rs`, scoped and annotated for removal in Task 8. Same pattern will apply to Tasks 6 and 7 (argv + echo modules).

## Task 6 — envoy-bin argv parser (2026-04-23)
- Commit: 4c270b9
- Change: hand-rolled argv parser (`-c`/`--config-path`) in main.rs with `ArgvError` + 6 unit tests; scoped `#![allow(dead_code)]` widened to the crate root to cover the argv items alongside config.
- Verification: `cargo test -p envoy-bin --bin envoy-bin argv_tests` → 6 passed; clippy + fmt --check → 0.
- Deviation: replaced the per-`mod config;` `#[allow(dead_code)]` with a single crate-root `#![allow(dead_code)]` on `main.rs` (carries forward Tasks 5–7 hygiene, all removed in Task 8).

## Task 7 — envoy-bin TCP echo + drain (2026-04-23)
- Commit: a41db88
- Change: `crates/envoy-bin/src/echo.rs` with async `serve(listener, shutdown)`, per-connection `echo_once`, JoinSet-based 5s drain + timeout-abort; 2 `#[tokio::test]`s (single-payload + concurrent). `mod echo;` registered in main.rs.
- Verification: `cargo test -p envoy-bin --bin envoy-bin echo::tests` → 2 passed; clippy + fmt --check → 0.
- Deviation: the brief's echo module requires `tokio::time::timeout` and `tokio::sync::oneshot`, but the envoy-bin Cargo.toml from Task 4 did not enable those tokio features. Added `"time"` and `"sync"` to the tokio feature list to make the code (and its tests) compile. This also lands in the same commit.

## Task 8 — envoy-bin main wiring (2026-04-23)
- Commit: 19afe3d
- Change: removed crate-root `#![allow(dead_code)]` + comment; wired `main() -> ExitCode` with argv parse → explicit multi-thread tokio runtime → `run()` (config load + TcpListener::bind + `echo::serve`) → `shutdown_signal()` (SIGTERM/SIGINT via `tokio::signal::unix`); added `install_tracing()` reading `ENVOY_RUST_LOG`.
- Verification: `cargo build -p envoy-bin --release` → 0; clippy + fmt --check → 0; `cargo test -p envoy-bin` → 13 passed (5 config + 6 argv + 2 echo); binary smoke test with no args → stderr matches `envoy-bin: expected exactly one of …`, exit 2.

## Task 9 — differential harness helpers (2026-04-23)
- Commit: 6c909c0
- Change: `tests/differential/src/lib.rs` with `Expectations`/`Equivalence`/`BodyRule`, `load_expectations`, `reserve_port` (TOCTOU-accepted), `render_yaml`, `write_temp`, `wait_accept_ready` (50ms→500ms exp. backoff) + 6 tests. Placeholders `subject.rs` and `upstream.rs` (filled in Tasks 11 and 10).
- Verification: `cargo test -p differential --lib` → 6 passed; clippy + fmt --check → 0.

## Task 10 — differential upstream launcher (2026-04-23)
- Commit: df8a0d1
- Change: `tests/differential/src/upstream.rs` with `IMAGE_NAME`/`IMAGE_TAG`/`CONTAINER_PORT` constants (matching ADR-0004), `UpstreamProxy` drop-to-stop guard, `start()` that bind-mounts the fixture's `envoy.yaml` and waits on the "starting main dispatch loop" stderr message + 500ms grace. One `#[ignore]`d tokio integration test. Added `tempfile = "3"` to dev-dependencies.
- Verification: `cargo test -p differential --lib` → 0 failed, 1 ignored (the container-launching test); clippy + fmt --check → 0; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (ADR-0005 exemptions hold).
- Note: local Docker daemon has an IPv6 routing bug; the `#[ignore]`d test was NOT run locally and will be validated by CI per the phase-done gate.

## Task 11 — differential subject launcher (2026-04-23)
- Commit: 143bbf4
- Change: `tests/differential/src/subject.rs` with `Subject` (port + optional Child; Drop aborts), async `shutdown(budget)`, `locate_envoy_bin` (honoring `CARGO_TARGET_DIR`, picking debug vs release by `cfg!(debug_assertions)`), `start(path, port)` (kill_on_drop, stdio inherited, `ENVOY_RUST_LOG=info`) + 2 tokio tests.
- Verification: `cargo test --workspace subject::tests` → 2 passed; clippy + fmt --check → 0; cargo deny check → all ok.
