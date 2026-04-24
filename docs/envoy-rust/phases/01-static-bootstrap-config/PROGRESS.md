# Phase 01 Progress

## Task 1 — ADRs 0008 / 0009 / 0010 (2026-04-24)

- Commit: 497bde5
- Change: appended ADR-0008 (envoy-config crate extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys as dev tooling), ADR-0010 (nightly toolchain for fuzz-only invocation) to DECISIONS.md.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 10 (ADR-0001 through ADR-0010).
- Deviation: PLAN Task 1 Step 6's sed-then-amend idiom captures the pre-amend SHA (orphaned after amend). Switched to the follow-up-commit convention that PLAN Task 2 Step 6 explicitly permits, and will apply it for every remaining Phase-01 task. SHA above is now the on-branch Task-1 main commit (497bde5); the SHA-patch follow-up commit lands separately.

## Task 2 — scaffold envoy-config crate (2026-04-24)

- Commit: 16581b8
- Change: created crates/envoy-config/{Cargo.toml,src/lib.rs,src/bootstrap.rs}; added envoy-config to root workspace members.
- Verification: cargo build --workspace --all-targets → 0; cargo clippy --workspace --all-targets --all-features -- -D warnings → 0; cargo fmt --all -- --check → 0; cargo test --workspace → test result: ok. 0 passed; 0 failed (envoy_config: 0 passed, 0 failed).
