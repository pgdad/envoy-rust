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

## Task 3 — envoy-config bootstrap type tree (2026-04-24)

- Commit: 639075e
- Change: populated crates/envoy-config/src/bootstrap.rs with the 10-struct Bootstrap type tree (SPEC §D1) + 2 serde shape tests (parses_phase00_minimal_into_bootstrap, parses_admin_only_bootstrap). No parse_bootstrap/ConfigError yet (Task 4).
- Verification: cargo test -p envoy-config → 2 passed, 0 failed; cargo clippy -p envoy-config --all-targets --all-features -- -D warnings → 0; cargo fmt --all -- --check → 0.
- TDD evidence: Step-2 red run failed with: `error[E0425]: cannot find type 'Bootstrap' in this scope` at crates/envoy-config/src/bootstrap.rs:22:16; Step-4 post-implement → 1 passed; Step-6 full gate → 2 passed, 0 failed.
