# Phase 03.2 Progress

## Task 1 — envoy-config: FilterChainMatch struct + server_names + 5 parse-shape tests (2026-04-26)

- Commit: 1e1ea64
- Change: Inserted `FilterChainMatch` struct (single `server_names: Vec<String>` field, `#[serde(deny_unknown_fields)]`, `#[derive(Debug, Deserialize, PartialEq)]`) in `crates/envoy-config/src/bootstrap.rs`. Added `filter_chain_match: Option<FilterChainMatch>` as the first field on `FilterChain` (mirrors Envoy's bootstrap proto field ordering) and extended its derive set with `PartialEq` (Task 2's validator tests will rely on it). Re-exported `FilterChainMatch` from `crates/envoy-config/src/lib.rs`. Appended 5 parse-shape tests covering present / missing / empty-map / empty-list / unknown-field cases.
- Verification: `cargo test -p envoy-config --lib` reported 55 passed (50 pre-existing from 03.1 + 5 new). Workspace gate clean: `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check` all exit 0. Cargo.lock unchanged.
- Deviation from PLAN: none.
