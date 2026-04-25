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
