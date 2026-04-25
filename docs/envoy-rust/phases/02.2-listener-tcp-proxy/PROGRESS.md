# Phase 02.2 Progress

## Task 1 — ADRs 0015 + 0016 (2026-04-25)

- Commit: 435c6fa
- Change: appended ADR-0015 (cross-container host reachability via host.docker.internal + host-gateway) and ADR-0016 (phase 02 TCP proxy runs with Envoy's default enable_half_close: false) to DECISIONS.md.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 16 (ADR-0001 through ADR-0016).

## Task 2 — phase-01 rollover M1: retarget stale TODO(phase-01) (2026-04-25)

- Commit: 8aab844
- Change: replaced the open-ended `nix`-deferral TODO comment in `tests/differential/src/subject.rs:25-32`. No functional change.
- Verification: `cargo build -p differential --all-targets`, `cargo clippy -p differential --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check` — all clean.

## Task 3 — phase-01 rollover I4: admin 8 KiB read-slice tightening (2026-04-25)

- Commit: 4bd0e22
- Change: in `crates/envoy-bin/src/admin.rs::handle_one`, bounded `stream.read(&mut scratch)` to `stream.read(&mut scratch[..remaining])` where `remaining = (MAX_REQUEST_HEAD - buf.len()).min(scratch.len())`. Updated `rejects_oversized_request_headers` to send exactly `MAX_REQUEST_HEAD + 1` bytes; added new test `accepts_requests_exactly_at_cap` proving 8192-byte requests parse cleanly to a normal 404.
- Verification: `cargo test -p envoy-bin admin::` — all admin tests pass (11 total, up from 10). Workspace gate (`build`, `clippy -D warnings`, `fmt --check`) clean.

## Task 4 — scaffold envoy-listener crate (2026-04-25)

- Commit: 787049a
- Change: created `crates/envoy-listener/{Cargo.toml,src/lib.rs}` (compiling stub — `#![forbid(unsafe_code)]` + module-level docstring only, no public items yet); added `crates/envoy-listener` to root `Cargo.toml [workspace] members`. Tasks 5 and 6 land the real surface.
- Verification: `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test -p envoy-listener` (0 tests) — all clean.

## Task 5 — envoy-listener::Listener::bind + ConnectionHandler trait + 2 tests (2026-04-25)

- Commit: 1ccc5a3
- Change: implemented `BoxFuture` alias, `ConnectionHandler` object-safe trait, `ListenerError` enum, `Listener` struct with `bind` + `local_addr`. `serve` stubbed `unimplemented!()` (Task 6). Added 2 tests: `bind_returns_socket_address`, `bind_fails_cleanly_on_address_in_use`. Plan-time deviation: `ListenerError::AddressParse(String, u16)` added (4th variant) for malformed `cfg.address.socket_address.address` strings — `envoy-config` keeps these as `String` until bind time. Mirrors phase-02.1 `envoy-cluster::ClusterError::EndpointParse`. Also added manual `Debug` impl for `Listener` (required by `expect_err` in test; `TcpListener` doesn't derive `Debug` automatically in this context) and formatted to pass `cargo fmt --check`.
- Verification: `cargo test -p envoy-listener` → 2 passed. Workspace gates (`build`, `clippy -D warnings`, `fmt --check`) clean.

## Task 6 — envoy-listener::Listener::serve + drain + 4 tests (2026-04-25)

- Commit: f601961
- Change: replaced `Listener::serve` stub with real `tokio::select!` accept loop over `JoinSet`, shutdown via pinned future, 5s `DRAIN_BUDGET`, abort-stragglers returning `ListenerError::DrainTimeout`. Four tests: `serves_accepts_and_dispatches_to_handler`, `serves_honors_shutdown_signal`, `serves_drains_in_flight_connection_within_budget`, `serves_aborts_stragglers_past_drain_budget`. envoy-listener test count: 2 → 6.
- Verification: `cargo test -p envoy-listener` → 6 passed. Workspace gates (`build`, `clippy -D warnings`, `fmt --check`) clean.

## Task 7 — scaffold envoy-tcp crate (2026-04-25)

- Commit: 2683476
- Change: created `crates/envoy-tcp/{Cargo.toml,src/lib.rs}` (compiling stub — `#![forbid(unsafe_code)]` + module-level docstring only, no public items yet); added `crates/envoy-tcp` to root `Cargo.toml [workspace] members`. Task 8 lands `TcpProxy` + `ConnectionHandler` impl + 4 tests.
- Verification: `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test -p envoy-tcp` (0 tests) — all clean.

## Task 8 — envoy-tcp::TcpProxy + ConnectionHandler impl + 4 tests (2026-04-25)

- Commit: c9950f2
- Change: implemented `TcpProxy` struct, `TcpProxyError` enum (`NoHealthyEndpoint`, `UpstreamConnect`, `CopyFailed`), and `ConnectionHandler` impl. Bidirectional copy uses `tokio::select!` over the two `tokio::io::copy` futures (plan-time deviation from SPEC §D2 step 4's `try_join!`), so EOF on either side drops the other copy future and propagates FIN — matches ADR-0016's `enable_half_close: false` posture. Four tests: `proxies_payload_end_to_end`, `proxies_closes_downstream_on_upstream_close`, `proxies_closes_upstream_on_downstream_close`, `proxies_returns_err_on_upstream_connect_refused`.
- Verification: `cargo test -p envoy-tcp` → 4 passed. Workspace gates (`build`, `clippy -D warnings`, `fmt --check`) clean.

## Task 9 — envoy-bin wiring + tcp_proxy integration test (2026-04-25)

- Commit: e1efc82
- Change: added `envoy-cluster`, `envoy-listener`, `envoy-tcp` path deps to `crates/envoy-bin/Cargo.toml`. Modified `main::run` to construct `ClusterManager` once and dispatch the listener's single filter on `envoy.filters.network.echo` (existing `echo::serve` path) vs. `envoy.filters.network.tcp_proxy` (new: build `TcpProxy`, pass to `Listener::serve`). Added Rust-native integration test `crates/envoy-bin/tests/tcp_proxy.rs` (no Docker): spawns envoy-bin subprocess against an in-process echo backend, drives a 17-byte payload, asserts byte-exact round-trip.
- Verification: `cargo test -p envoy-bin` → all tests pass (admin + echo + integration tests). Workspace gates (`build`, `clippy -D warnings`, `fmt --check`, `deny check`) clean.
- Note: Step 2 (pre-modification test) — the test passed against the unmodified `envoy-bin` because `echo::serve` echoes bytes locally without needing the upstream backend round-trip. The test still serves as a regression gate: post-wiring, the byte-exact round-trip now goes through the tcp_proxy path via the backend.

## Task 10 — TcpProxyBackend helper + 2 tests (2026-04-25)

- Commit: 8624c41
- Change: created `tests/differential/src/backend.rs` with `TcpProxyBackend` (spawns the workspace `tcp-echo-server` binary as a host subprocess on a reserved port; SIGKILL on Drop with 2s exit polling; `container_host()` returns `host.docker.internal` per ADR-0015). `locate_tcp_echo_server` walks two parents up from `CARGO_MANIFEST_DIR` to the workspace root and joins `target/<profile>/tcp-echo-server` (cross-package `CARGO_BIN_EXE_*` is unavailable per SPEC §6 signpost 8). Two tests: `tcp_proxy_backend_spawns_and_echoes`, `tcp_proxy_backend_drop_terminates_child` (both skip if the helper binary isn't built).
- Verification: `cargo test -p differential backend::tests` → 2 passed. Workspace gates (`build`, `clippy -D warnings`, `fmt --check`) clean.

## Task 11 — differential: backend keys + run_fixture dispatch + with_host (2026-04-25)

- Commit: aa4187f
- Change: dropped dead `|| msg.contains("CRLF")` disjunct in `decode_chunked_truncated_size_line` (phase-02.1 REVIEW M3 close-out). Extended `run_fixture` to spawn `backend::TcpProxyBackend` when either template references `{{BACKEND_PORT}}`; build per-side substitution maps with `{{BACKEND_HOST}}` → `host.docker.internal` (envoy side) vs. `127.0.0.1` (envoy-rust side); flag `host_uses_host_gateway` on rendered upstream YAML containing `host.docker.internal`. Extended `upstream::start(yaml, host_gateway: bool)` to apply `with_host("host.docker.internal", Host::HostGateway)` per ADR-0015 when the flag is true. Updated `starts_upstream_envoy_and_exposes_host_port` to pass `false`. Added 3 unit tests: `render_yaml_substitutes_backend_keys_for_envoy_side`, `render_yaml_substitutes_backend_keys_for_envoy_rust_side`, `fixture_0003_expectations_parses_as_tcp_echo` (skip-if-not-yet-landed wrapper).
- Verification: `cargo test -p differential --lib` → all pass. Workspace gates (`build`, `clippy -D warnings`, `fmt --check`) clean.

## Task 12 — fixture 0003-tcp-proxy + Docker-gated test (2026-04-25)

- Commit: 8e343b7
- Change: created `tests/fixtures/0003-tcp-proxy/{envoy.yaml,envoy-rust.yaml,inputs/payload.bin,expectations.yaml,README.md}`. payload.bin is byte-copy of fixture 0001's 18-byte payload `b"hello, envoy-rust\n"` (SPEC §6 signpost 10). envoy.yaml uses `{{BACKEND_HOST}}` (templates to `host.docker.internal` per ADR-0015); envoy-rust.yaml uses literal `127.0.0.1`. `enable_half_close` is absent from both per ADR-0016. Created `tests/differential/tests/tcp_proxy.rs` (Docker-gated acceptance test calling `differential::run_fixture("0003-tcp-proxy")`). The forward-regression test `fixture_0003_expectations_parses_as_tcp_echo` (landed in Task 11) now exercises the file rather than skipping.
- Verification: `cargo test -p differential fixture_0003_expectations_parses_as_tcp_echo` → 1 passed (no longer skipping). `cargo test --workspace --lib --bins` → all pass. Workspace gates (`build`, `clippy -D warnings`, `fmt --check`) clean. Docker-gated `tcp_proxy_fixture` runs in CI; local pass-through depends on Docker availability.
