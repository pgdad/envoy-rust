# Phase 02.1 Progress

## Task 1 — ADR-0014 (2026-04-24)

- Commit: 6d1f8d6
- Change: appended ADR-0014 (YAML-native `typed_config` deserialization until the xDS/protos family lands) to DECISIONS.md. Renumbered from parent-SPEC ADR-0013 per the ADR-0013 phase-02 split decision.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 14 (ADR-0001 through ADR-0014).

## Task 2 — typed_config envelope (2026-04-24)

- Commit: ebaa712
- Change: extended `NetworkFilter` with `typed_config: Option<TypedConfig>`; introduced `TypedConfig` tagged-enum envelope (single `TcpProxy(TcpProxyConfig)` variant per ADR-0014) and `TcpProxyConfig { stat_prefix, cluster }`. Added 3 shape tests.
- Verification: `cargo test -p envoy-config` → `test result: ok. 24 passed; 0 failed` (21 phase-01 + 3 new).
- Deviation from plan: the test YAML in `parses_bootstrap_with_tcp_proxy_filter` (PLAN.md Step 1) as-written references cluster fields (`type`, `lb_policy`, `load_assignment`) that only land in Task 3; with phase-01's `Cluster { name: String }` under `deny_unknown_fields`, serde rejected the YAML before the test's filter assertions could run. Simplified the cluster block to `clusters: [{ name: backend }]` — preserves the TCP-proxy → cluster name-reference scene-setting and matches Task 2's parse-layer semantics. Task 3's `parses_bootstrap_with_single_endpoint_cluster` exercises the full cluster shape. Drift logged per D-3.5 (plan drift, not spec drift; no ADR needed).

## Task 3 — cluster topology types (2026-04-24)

- Commit: c380481
- Change: replaced `Cluster { name }` stub with full topology (`name`, `cluster_type` (serde-renamed from `type`), `lb_policy`, `load_assignment`); introduced six supporting types: `ClusterType` (enum, SCREAMING_SNAKE_CASE, `Static` variant only), `LbPolicy` (enum, SCREAMING_SNAKE_CASE, `RoundRobin` variant only), `LoadAssignment`, `LocalityLbEndpoints`, `LbEndpoint`, `Endpoint`. Added `PartialEq` to `Address` and `SocketAddress`. All new types carry `#[serde(deny_unknown_fields)]`.
- Verification: `cargo test -p envoy-config` → `test result: ok. 27 passed; 0 failed` (24 phase-01/02 + 3 new in this task).
- Pre-existing tests updated in-place (no count change): `parses_bootstrap_with_clusters_stub` renamed to `parses_bootstrap_with_single_endpoint_cluster` with full cluster shape YAML; `rejects_unknown_cluster_field` YAML expanded to full cluster shape with `bogus: 1` sibling to `name`.
- Also re-expanded the `parses_bootstrap_with_tcp_proxy_filter` YAML cluster block to the full Task-3 topology (reversing Task 2's authorized YAML simplification, now that the schema supports it).

## Task 4 — validator extensions + 5 ConfigError variants (2026-04-24)

- Commit: 3dde2a6
- Change: added 5 `ConfigError` variants (`MissingTypedConfig`, `UnexpectedTypedConfig`, `UnknownCluster`, `LoadAssignmentNameMismatch`, `EmptyClusterEndpoints`); extended `pub use` in `lib.rs` with 8 new public types (`TypedConfig`, `TcpProxyConfig`, `ClusterType`, `LbPolicy`, `LoadAssignment`, `LocalityLbEndpoints`, `LbEndpoint`, `Endpoint`); added `TCP_PROXY_FILTER` constant; extended `validate` with per-cluster invariants (cluster_name match, ≥1 lb_endpoint) and widened per-listener allow-list to `{echo, tcp_proxy}` with typed_config structural rules; renamed `rejects_non_echo_filter` → `rejects_unknown_filter_name` (YAML uses `rbac` instead of `tcp_proxy` so the test remains in the UnsupportedFilter path after the allow-list widening); added 10 new tests.
- Verification (envoy-config): `cargo test -p envoy-config` → `test result: ok. 37 passed; 0 failed` (27 prior + 10 new in this task).
- Verification (workspace): `cargo test --workspace --exclude differential` → green; Docker-gated `admin_ready_fixture` fails as expected (no Docker socket in this environment); `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit 0; `cargo fmt --all -- --check` → exit 0.

## Task 5 — scaffold envoy-cluster crate (2026-04-24)

- Commit: ed02a07
- Change: created `crates/envoy-cluster/` library crate with minimum-viable scaffolding: `Cargo.toml` (dependencies: `envoy-config` path, `thiserror = "2"`), `src/lib.rs` (module-level doc comment + stable re-export names), `src/cluster.rs` (placeholder types: `Cluster`, `ClusterHandle`, `ClusterManager`, `ClusterError`, `from_bootstrap`). Added `crates/envoy-cluster` to root `Cargo.toml` `[workspace] members` (alphabetically between `envoy-bin` and `envoy-config`). Placeholder fields annotated with `#[allow(dead_code)]` to pass clippy.
- Verification: `cargo check --workspace` → green; `cargo clippy -p envoy-cluster --all-targets --all-features -- -D warnings` → exit 0; `cargo fmt --all -- --check` → exit 0; `cargo test --workspace` → `test result: ok. 22 passed; 0 failed; 1 ignored` (envoy-cluster contributes 0 tests; Docker-gated `admin_ready_fixture` fails as expected); `cargo deny check` → all ok.

## Task 6 — envoy-cluster::cluster round-robin endpoint picker (2026-04-24)

- Commit: 35eac2e
- Change: replaced `Cluster` and `ClusterHandle` placeholder bodies with real implementations. `Cluster` derives `Debug` and owns `name: String`, `endpoints: Vec<SocketAddr>`, and `cursor: AtomicUsize`; provides private `pick() -> Option<SocketAddr>` using `fetch_add(1, Ordering::Relaxed)` + modulo. `ClusterHandle` derives `Clone, Debug`, wraps `Arc<Cluster>`, and exposes `pub fn pick_endpoint(&self) -> Option<SocketAddr>`. Removed struct-level `#[allow(dead_code)]` from both types (fields now used); added field-level `#[allow(dead_code)]` on `name` only (used in Task 7). `ClusterManager`, `ClusterError`, and `from_bootstrap` placeholders untouched.
- Test verification: `cargo test -p envoy-cluster` → `test result: ok. 3 passed; 0 failed` (`pick_endpoint_cycles_over_three_endpoints`, `pick_endpoint_is_stable_under_concurrent_calls`, `handle_clone_shares_cursor`).
- envoy-config count held: `cargo test -p envoy-config` → `test result: ok. 37 passed; 0 failed`.
- Workspace lint: `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit 0; `cargo fmt --all -- --check` → exit 0.
