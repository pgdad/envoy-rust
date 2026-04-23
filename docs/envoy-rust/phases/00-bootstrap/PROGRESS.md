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
