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

## Task 7 — ClusterManager + from_bootstrap (2026-04-24)

- Commit: 72d9dcb
- Change: replaced `ClusterManager`, `ClusterError`, and `from_bootstrap` placeholders with real implementations. `ClusterManager` derives `Debug`, holds `clusters: HashMap<String, Arc<Cluster>>`, and exposes `pub fn get(&self, name: &str) -> Option<ClusterHandle>` (returns a fresh `ClusterHandle` wrapping an `Arc::clone`). `ClusterError` is a `thiserror` enum with three variants: `EmptyCluster { name }`, `DuplicateClusterName { name }`, `EndpointParse { cluster, addr, #[source] source: AddrParseError }`. `from_bootstrap` iterates `bootstrap.static_resources.clusters`, parses each `"address:port"` string via `.parse::<SocketAddr>()` (wrapping failure in `EndpointParse`), rejects empty endpoint lists (`EmptyCluster`), inserts into a `HashMap`, and rejects collisions via `HashMap::insert` returning `Some(_)` (`DuplicateClusterName`). Removed `#[allow(dead_code)]` from `ClusterManager` (now used); `Cluster.name` retains its field-level `#[allow(dead_code)]` because it is written at construction time but never read back from a `Cluster` instance — the HashMap key carries the lookup identity. Added `#[derive(Debug)]` to `ClusterManager` (required by test `expect_err` bounds; not specified in plan but necessary for compilation).
- Test verification: `cargo test -p envoy-cluster` → `test result: ok. 8 passed; 0 failed` (3 Task-6 + 5 new: `from_bootstrap_builds_single_endpoint_cluster`, `from_bootstrap_builds_three_endpoint_cluster`, `from_bootstrap_rejects_empty_cluster`, `from_bootstrap_rejects_duplicate_cluster_name`, `from_bootstrap_rejects_malformed_endpoint_address`).
- envoy-config count held: `cargo test -p envoy-config` → `test result: ok. 37 passed; 0 failed`.
- Workspace lint: `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit 0; `cargo fmt --all -- --check` → exit 0.
- Pre-existing Docker-gated `differential::admin_ready_fixture` failure unchanged (no Docker socket; pre-dates this task).

## Task 8 — scaffold tcp-echo-server helper crate (2026-04-24)

- Commit: 81f5f5b
- Change: created `tests/helpers/tcp-echo-server/` binary crate with minimum-viable scaffolding: `Cargo.toml` (dependencies: `anyhow = "1"`, `thiserror = "2"`, `tokio` with features `["rt-multi-thread", "net", "io-util", "macros", "signal"]`, `tracing = "0.1"`, `tracing-subscriber` with features `["env-filter", "fmt"]`), `src/main.rs` (placeholder with `unimplemented!()` macro; Tasks 9 and 10 provide substantive runtime). Added `tests/helpers/tcp-echo-server` to root `Cargo.toml` `[workspace] members`.
- Verification (gates): `cargo build --workspace --all-targets` → `Finished dev profile […] in 0.39s` (compiles tcp-echo-server cleanly); `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit 0; `cargo fmt --all -- --check` → exit 0.
- Verification (tests): `cargo test -p envoy-config` → `test result: ok. 37 passed; 0 failed` (unchanged). `cargo test -p envoy-cluster` → `test result: ok. 8 passed; 0 failed` (unchanged). `tcp-echo-server` contributes 0 tests.
- Verification (deny): `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (no new license surface; `tracing-subscriber` + `anyhow` + `thiserror` already reachable transitively via `envoy-bin`, per SPEC §D3.5).

## Task 9 — tcp-echo-server argv parser (2026-04-24)

- Commit: d5b6afa
- Change: replaced `src/main.rs` placeholder with full argv-parser module: `Args { port: u16 }`, `ArgvError` (6 variants: `MissingFlag(&'static str)`, `MissingValue`, `InvalidPort`, `Trailing`, `HelpRequested`, `VersionRequested`), `parse_argv(args: &[String]) -> Result<Args, ArgvError>` using hand-written index loop. `main()` remains `unimplemented!()` (Task 10 lands the tokio runtime). Added `#[allow(dead_code)]` to `Args`, `ArgvError`, and `parse_argv` (items are test-only until Task 10 wires them into `main`).
- Verification (tcp-echo-server): `cargo test -p tcp-echo-server` → `test result: ok. 6 passed; 0 failed` (`argv_parses_port`, `argv_rejects_missing_port_flag`, `argv_rejects_missing_value`, `argv_rejects_non_numeric_port`, `argv_rejects_trailing_argument`, `argv_shows_help`).
- Verification (envoy-config): `cargo test -p envoy-config` → `test result: ok. 37 passed; 0 failed` (unchanged).
- Verification (envoy-cluster): `cargo test -p envoy-cluster` → `test result: ok. 8 passed; 0 failed` (unchanged).
- Verification (lint): `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit 0; `cargo fmt --all -- --check` → exit 0.
- Deviation: plan's code blocks do not include `#[allow(dead_code)]` annotations, but clippy `-D warnings` with `dead_code` errors out on `Args`, `ArgvError`, and `parse_argv` when compiled as a binary target (they are only reachable from `#[cfg(test)]` until Task 10). Added field-granular `#[allow(dead_code)]` per the established pattern from Task 6's `Cluster.name` handling. Not a spec drift; the plan's intent is correct Rust — the annotations are a mechanical requirement of the clippy gate, not a design change.

## Task 10 — tcp-echo-server runtime + tokio tests (2026-04-24)

- Commit: 5ae33ef ("phase 02.1: tcp-echo-server runtime + drain")
- Change: replaced `unimplemented!()` in `main()` with full tokio runtime. Added `run_on(TcpListener, shutdown: Future) -> Result<()>` (tokio `select!` between `listener.accept()` and shutdown; per-connection `JoinSet::spawn` with `io::copy`; 5-second drain via `tokio::time::timeout(DRAIN_BUDGET, ...)`); `run(port) -> Result<()>` (bind `127.0.0.1:<port>` then `run_on` with `ctrl_c` shutdown); `#[tokio::main(flavor = "multi_thread")] async fn main() -> ExitCode` (tracing init on stderr, parse_argv, `HelpRequested`/`VersionRequested` → exit 0, other argv errors → exit 2, runtime error → exit 1). Removed all 3 `#[allow(dead_code)]` annotations from Task 9 (`Args`, `ArgvError`, `parse_argv` now reachable via `main`). Added 2 tokio tests: `echoes_round_trip` (bind ephemeral port, write 32-byte payload, verify exact echo, send oneshot shutdown) and `drain_exits_within_budget` (stalled connection, oneshot shutdown, assert server resolves within `DRAIN_BUDGET + 500ms`).
- Cargo.toml extension: added `"time"` and `"sync"` to the tokio features list. `"time"` is plan-prescribed (plan Step 1). `"sync"` is required by `tokio::sync::oneshot` used in plan's test code blocks — the plan's test code calls `oneshot::channel` which is gated behind the `sync` feature; adding it is a mechanical requirement of the plan's test shape.
- Verification (tcp-echo-server): `cargo test -p tcp-echo-server` → `test result: ok. 8 passed; 0 failed` (6 argv + 2 tokio; finished in 5.01s).
- Verification (envoy-config): `cargo test -p envoy-config` → `test result: ok. 37 passed; 0 failed` (unchanged).
- Verification (envoy-cluster): `cargo test -p envoy-cluster` → `test result: ok. 8 passed; 0 failed` (unchanged).
- Verification (workspace): `cargo test --workspace` → `test result: ok. 22 passed; 0 failed; 1 ignored` (integration suite) + `FAILED 0 passed; 1 failed` for `differential::admin_ready_fixture` (Docker-gated, pre-existing, no Docker socket in this environment).
- Verification (lint): `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit 0 (fixed two `clippy::let_unit_value` hits in `drain_exits_within_budget` caused by the plan's `let result = ...` + `let _ = result;` pattern — inlined the chain since the return type is `()`); `cargo fmt --all -- --check` → exit 0 (reformatted `run_on` signature and `tracing::warn!` call to satisfy the 100-char column limit).
- Verification (deny): `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (no new license surface from `tokio "time"` + `"sync"` features).
- Self-review: `grep '#\[allow(dead_code)\]' tests/helpers/tcp-echo-server/src/main.rs` → 0 matches (confirmed).
