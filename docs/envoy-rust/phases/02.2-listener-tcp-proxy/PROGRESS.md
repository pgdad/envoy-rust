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
