# Phase 02.2 Progress

## Task 1 — ADRs 0015 + 0016 (2026-04-25)

- Commit: 435c6fa
- Change: appended ADR-0015 (cross-container host reachability via host.docker.internal + host-gateway) and ADR-0016 (phase 02 TCP proxy runs with Envoy's default enable_half_close: false) to DECISIONS.md.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 16 (ADR-0001 through ADR-0016).

## Task 2 — phase-01 rollover M1: retarget stale TODO(phase-01) (2026-04-25)

- Commit: 8aab844
- Change: replaced the open-ended `nix`-deferral TODO comment in `tests/differential/src/subject.rs:25-32`. No functional change.
- Verification: `cargo build -p differential --all-targets`, `cargo clippy -p differential --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check` — all clean.
