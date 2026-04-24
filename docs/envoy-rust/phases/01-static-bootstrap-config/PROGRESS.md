# Phase 01 Progress

## Task 1 — ADRs 0008 / 0009 / 0010 (2026-04-24)

- Commit: 5c016bf
- Change: appended ADR-0008 (envoy-config crate extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys as dev tooling), ADR-0010 (nightly toolchain for fuzz-only invocation) to DECISIONS.md.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 10 (ADR-0001 through ADR-0010).
