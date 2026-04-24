# Phase 02.1 — Config schema + cluster manager + echo-server helper

- **Phase id:** `02.1`
- **Parent phase:** `02-tcp-proxy` (split per ADR-0013)
- **Title:** Config schema + cluster manager + echo-server helper
- **Depends on:** `01` (done as of commit `aef36ce`)
- **Differential surface when done:** none new; existing differential fixtures `0001-tcp-echo` and `0002-static-admin-ready` remain green. No new `tests/fixtures/NNNN-*/` directory lands this sub-phase. Fixture `0003-tcp-proxy` ships in sub-phase 02.2.
- **Seeded by:** `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (parent, committed at SHA `50349da`) §§D1, D3, D6, D9 (item I3 only), D10; split decision at ADR-0013.

This SPEC is the design contract for sub-phase 02.1. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) must be able to execute it without consulting the parent `02-tcp-proxy/SPEC.md`.

---

## 1. Goal and acceptance signal

**Goal.** Extend `envoy-config` with the type tree and validator rules Envoy's `envoy.filters.network.tcp_proxy` + static `STATIC` cluster + `lb_policy: ROUND_ROBIN` grammar requires; land a new `envoy-cluster` library crate that owns the static-cluster data model + round-robin LB state; land a `tests/helpers/tcp-echo-server/` helper binary crate that sub-phase 02.2's fixture 0003 will later dial. Close phase-01 REVIEW §9 starter item **I3** (`decode_chunked` unit tests) in the differential harness. Land two new fuzz-corpus seeds exercising the new YAML grammar.

Sub-phase 02.1 does **not** ship the listener (`envoy-listener`) or the TCP proxy filter (`envoy-tcp`). `envoy-bin` is not re-wired this sub-phase; it continues to treat `envoy.filters.network.tcp_proxy` as an unknown filter at runtime dispatch time, consistent with phase-01 behavior. (The parser now accepts the YAML because downstream 02.2 needs this coverage landed first; the runtime dispatch gap is closed in 02.2 where `envoy-listener` + `envoy-tcp` exist.)

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 02.1's feature surface:

- (a) no new differential fixture; pre-existing fixtures `tests/fixtures/0001-tcp-echo/` and `tests/fixtures/0002-static-admin-ready/` remain green (unchanged);
- (b) no conformance suites run this sub-phase (first one — `h2spec` — attaches in phase 05);
- (c) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo fuzz run parse_bootstrap -- -max_total_time=30`) against the extended corpus that now includes two TCP-proxy-shaped seeds (§D5);
- (d) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- (e) `REVIEW.md` for this sub-phase is approved.

**Scope shape (inherited from parent-phase brainstorm).** Five forks were resolved during the parent-phase brainstorm; the two that bind on 02.1 are:

- **Crate layout — three new crates.** `envoy-cluster` ships in 02.1. `envoy-listener` and `envoy-tcp` ship in 02.2. Phase 07 owns the filter-chain framework (`envoy-filter`); nothing here preempts it.
- **`typed_config` deserialization — YAML-native.** A Rust enum `TypedConfig` discriminated on the `@type` URL string literal. No `prost` / `envoy-protos` in phase 02; those defer to the xDS family per ADR-0014 (landed in 02.1; see §7). ADR-0014 in this SPEC is the renumbered counterpart of parent-SPEC §7's ADR-0013 after the ADR-0013 split decision.

The three forks that bind on 02.2 (round-robin LB differential scope — minimum; upstream backend — in-tree Rust helper crate; phase-01 rollovers I4 + M1) are out of scope here. The remaining rollover **I3** (`decode_chunked` unit tests, harness-only) lands in 02.1 (§D4).

---

## 2. Behavior-contract scope for sub-phase 02.1

Sub-phase 02.1 engages **no row** of the `BEHAVIOR_CONTRACT.md` §7.2 equivalence matrix at runtime, because no new differential fixture ships. Pre-existing fixtures 0001 and 0002 continue to exercise row 2 (response body byte-exact) and the ADR-0011-deferred partial coverage of row 3 (response headers) respectively; 02.1 does not touch their acceptance paths.

No `BEHAVIOR_CONTRACT.md` edits in 02.1. The currently-empty subsections (`Header allow-list`, `Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances`) remain empty.

---

## 3. Deliverables

### D1 — New library crate `crates/envoy-cluster/`

Added to the root `Cargo.toml` `[workspace] members`. Owns the static-cluster data model, endpoint resolution, and the round-robin LB state. Depends only on the D-3.2 permitted-foundations list.

- `crates/envoy-cluster/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Dependencies: `envoy-config = { path = "../envoy-config" }`, `thiserror`. No `tokio`, no `tracing` (no async surface in 02.1 — the cluster crate is a pure data-model + `AtomicUsize` cursor).
- `crates/envoy-cluster/src/lib.rs` starts with `#![forbid(unsafe_code)]` per D-3.8. Public surface:

    ```rust
    pub fn from_bootstrap(bootstrap: &envoy_config::Bootstrap)
        -> Result<ClusterManager, ClusterError>;

    pub struct ClusterManager { /* HashMap<String, Arc<Cluster>> */ }

    impl ClusterManager {
        pub fn get(&self, name: &str) -> Option<ClusterHandle>;
    }

    #[derive(Clone)]
    pub struct ClusterHandle { /* Arc<Cluster> */ }

    impl ClusterHandle {
        /// Returns `None` only when the cluster is empty — which `from_bootstrap`
        /// rejects at construction time, so this is effectively infallible in
        /// phase 02. Option<_> is preserved for phase-06+ health checking.
        pub fn pick_endpoint(&self) -> Option<std::net::SocketAddr>;
    }

    #[derive(Debug, thiserror::Error)]
    pub enum ClusterError {
        #[error("cluster '{name}' has no lb_endpoints")]
        EmptyCluster { name: String },
        #[error("duplicate cluster name '{name}'")]
        DuplicateClusterName { name: String },
        #[error("cluster '{cluster}' endpoint address {addr:?} is not a valid SocketAddr: {source}")]
        EndpointParse {
            cluster: String,
            addr: String,
            #[source] source: std::net::AddrParseError,
        },
    }
    ```

- `crates/envoy-cluster/src/cluster.rs` hosts `Cluster` with fields `name: String`, `endpoints: Vec<SocketAddr>`, `cursor: AtomicUsize`. `pick_endpoint`:
    ```rust
    let i = self.cursor.fetch_add(1, Ordering::Relaxed);
    Some(self.endpoints[i % self.endpoints.len()])
    ```
  The modulo guards against counter overflow over very long runs; `Relaxed` is sufficient because there is no cross-observation of the cursor value. Empty-endpoint guard is enforced at `from_bootstrap` construction — `pick_endpoint` never has to check.

- `from_bootstrap` iterates `bootstrap.static_resources.clusters`, validates each (`type == STATIC`, `lb_policy == ROUND_ROBIN`, `load_assignment.cluster_name == name`, ≥1 `lb_endpoint`, each `endpoint.address.socket_address` parses as `SocketAddr` via `format!("{}:{}", addr, port).parse()`), and inserts into the `HashMap`. Unsupported `ClusterType`/`LbPolicy` variants are rejected at the `envoy-config` layer (§D2) so they cannot reach `envoy-cluster`.

- Unit tests in `crates/envoy-cluster/src/cluster.rs::tests` (eight tests):
  - `pick_endpoint_cycles_over_three_endpoints` — N=3 endpoints; call `pick_endpoint` 7 times; assert sequence `[0, 1, 2, 0, 1, 2, 0]`.
  - `pick_endpoint_is_stable_under_concurrent_calls` — N=3; spawn 1000 concurrent `pick_endpoint` calls across threads (using `std::thread::spawn`, no tokio); assert each endpoint is picked ~333 times (±10 %).
  - `from_bootstrap_rejects_empty_cluster` — cluster with zero `lb_endpoints` → `ClusterError::EmptyCluster`. (Note: `envoy-config` also rejects this at parse time — §D2; `envoy-cluster`'s check is defense-in-depth.)
  - `from_bootstrap_rejects_duplicate_cluster_name` — two clusters named `"backend"` → `ClusterError::DuplicateClusterName`.
  - `from_bootstrap_rejects_malformed_endpoint_address` — cluster with `address: "not-a-host"` → `ClusterError::EndpointParse`. (Parsed-valid YAML with an invalid `SocketAddr` cannot be rejected at `envoy-config` serde-level; this is the only ClusterError that is non-defense-in-depth.)
  - `from_bootstrap_builds_single_endpoint_cluster` — happy path.
  - `from_bootstrap_builds_three_endpoint_cluster` — happy path for the round-robin test.
  - `handle_clone_shares_cursor` — `ClusterHandle::clone` returns a handle whose `pick_endpoint` advances the *same* `AtomicUsize`, proving `Arc` semantics.

### D2 — `envoy-config` schema extensions

`crates/envoy-config/src/bootstrap.rs` gains the `typed_config` envelope, the cluster topology, and the associated validator rules. The `Node` open-schema asymmetry from phase 01 is **not** widened here.

- **Filter envelope.** `NetworkFilter` grows an optional `typed_config`:

    ```rust
    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    pub struct NetworkFilter {
        pub name: String,
        #[serde(default)]
        pub typed_config: Option<TypedConfig>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(tag = "@type", deny_unknown_fields)]
    pub enum TypedConfig {
        #[serde(rename = "type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy")]
        TcpProxy(TcpProxyConfig),
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    pub struct TcpProxyConfig {
        pub stat_prefix: String,   // required in Envoy; accepted + unused until phase 06
        pub cluster: String,
    }
    ```

  `typed_config` as `Option<…>` preserves fixture 0001's echo-filter YAML (which has no `typed_config`) verbatim. `deny_unknown_fields` on `TcpProxyConfig` rejects every other Envoy tcp_proxy field (`idle_timeout`, `access_log`, `max_connect_duration`, `tunneling_config`, `on_new_connection`, …).

- **Cluster topology.** `Cluster` is fleshed out from phase 01's name-only stub:

    ```rust
    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    pub struct Cluster {
        pub name: String,
        #[serde(rename = "type")]
        pub cluster_type: ClusterType,
        pub lb_policy: LbPolicy,
        pub load_assignment: LoadAssignment,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
    pub enum ClusterType { Static }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
    pub enum LbPolicy { RoundRobin }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    pub struct LoadAssignment {
        pub cluster_name: String,
        pub endpoints: Vec<LocalityLbEndpoints>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    pub struct LocalityLbEndpoints {
        pub lb_endpoints: Vec<LbEndpoint>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    pub struct LbEndpoint {
        pub endpoint: Endpoint,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    pub struct Endpoint {
        pub address: Address,   // reuses phase-01 Address / SocketAddress
    }
    ```

  The five-level depth (`endpoints > lb_endpoints > endpoint > address > socket_address`) is intentionally Envoy-verbatim. Verbose for a single endpoint; exactly right for phase 06+ when locality / weight / health_status / metadata fields land.

- **Validator extensions** in `envoy-config::bootstrap::validate`:

  - Per listener, the allowed filter names are `{envoy.filters.network.echo, envoy.filters.network.tcp_proxy}`. Echo still accepted for fixture 0001. Any other name → `ConfigError::UnsupportedFilter` (variant unchanged from phase 01).
  - `tcp_proxy` requires `typed_config: Some(TypedConfig::TcpProxy { .. })`; missing → new `ConfigError::MissingTypedConfig(&'static str)` variant.
  - `echo` requires `typed_config: None`; present → new `ConfigError::UnexpectedTypedConfig(&'static str)` variant.
  - For each tcp_proxy filter, `typed_config.cluster` must name a cluster in `static_resources.clusters`. Missing → new `ConfigError::UnknownCluster(String)` variant.
  - Per cluster: `load_assignment.cluster_name == name`. Mismatch → new `ConfigError::LoadAssignmentNameMismatch { cluster: String, assignment: String }` variant.
  - Per cluster: `load_assignment.endpoints` flattened across all `LocalityLbEndpoints` must have ≥1 total `LbEndpoint`. Zero → new `ConfigError::EmptyClusterEndpoints(String)` variant. (Named distinctly from `envoy-cluster::EmptyCluster` so the two layers' errors remain distinguishable.)
  - `listeners.len() ∈ {0, 1}` cap stays (phase-01 invariant); `clusters.len()` is unbounded.

- Unit tests appended to `crates/envoy-config/src/bootstrap.rs::tests` (sixteen new tests):
  - `parses_bootstrap_with_tcp_proxy_filter` — full happy-path fixture (listener → tcp_proxy → cluster with one endpoint).
  - `parses_bootstrap_with_round_robin_multi_endpoint_cluster` — N=3 endpoints parse successfully.
  - `rejects_tcp_proxy_without_typed_config` — `ConfigError::MissingTypedConfig`.
  - `rejects_echo_with_typed_config` — `ConfigError::UnexpectedTypedConfig`.
  - `rejects_typed_config_unknown_type_url` — serde tagged-enum default rejection.
  - `rejects_unknown_tcp_proxy_config_field` (e.g. `idle_timeout: 0s`) — `deny_unknown_fields`.
  - `rejects_cluster_type_logical_dns` — asserts `ClusterType::Static` is the only accepted variant.
  - `rejects_lb_policy_least_request` — asserts `LbPolicy::RoundRobin` is the only accepted variant.
  - `rejects_tcp_proxy_naming_missing_cluster` — `ConfigError::UnknownCluster`.
  - `rejects_load_assignment_cluster_name_mismatch` — `ConfigError::LoadAssignmentNameMismatch`.
  - `rejects_empty_lb_endpoints` — `ConfigError::EmptyClusterEndpoints`.
  - `rejects_malformed_endpoint_address` (e.g. `address: "not-a-host"`) — `envoy-config` accepts as serde-valid; `envoy-cluster::from_bootstrap` (D1) rejects at construction. This test asserts the parse-layer *acceptance* (negative-test on the parse path), paired with the `envoy-cluster` D1 test on the rejection path.
  - `rejects_unknown_cluster_field` — `deny_unknown_fields` regression on `Cluster`.
  - `rejects_unknown_load_assignment_field` — `deny_unknown_fields` regression on `LoadAssignment`.
  - `rejects_unknown_locality_lb_endpoints_field` — `deny_unknown_fields` regression on `LocalityLbEndpoints`.
  - `rejects_unknown_lb_endpoint_field` — `deny_unknown_fields` regression on `LbEndpoint`.

  Note: the list above reads as seventeen lines but compresses to sixteen distinct tests because `rejects_malformed_endpoint_address` is a single parse-acceptance test shared across the boundary with D1. If during plan-writing the test boundary shifts (e.g., `Endpoint.address` shapes change such that malformed addresses *do* reject at serde layer), add a corresponding `rejects_unknown_endpoint_field` test and fold the address-shape tests accordingly — the net coverage goal is `deny_unknown_fields` regression on every new struct level (six struct levels: `TypedConfig`, `TcpProxyConfig`, `Cluster`, `LoadAssignment`, `LocalityLbEndpoints`, `LbEndpoint`).

- `crates/envoy-config/src/lib.rs` re-exports new public types (`TypedConfig`, `TcpProxyConfig`, `ClusterType`, `LbPolicy`, `LoadAssignment`, `LocalityLbEndpoints`, `LbEndpoint`, `Endpoint`); extends `ConfigError` with `MissingTypedConfig`, `UnexpectedTypedConfig`, `UnknownCluster`, `LoadAssignmentNameMismatch`, `EmptyClusterEndpoints`.

### D3 — `tcp-echo-server` helper crate

New workspace member `tests/helpers/tcp-echo-server/`. First crate under `tests/helpers/`; the `tests/helpers/` directory itself is already named in the §4 layout of `BOOTSTRAP_PROMPT.md` so no ADR is required for its creation.

Landing this crate in 02.1 (ahead of sub-phase 02.2's fixture 0003) keeps 02.2 focused on the listener/proxy/harness integration and lets the helper's own tests run in isolation under 02.1's CI gate before being composed with `TcpProxyBackend` in 02.2.

- `tests/helpers/tcp-echo-server/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Deps from the D-3.2 list only: `tokio` (features `rt-multi-thread`, `net`, `io-util`, `macros`, `signal`), `anyhow`, `thiserror`, `tracing`, `tracing-subscriber`. (`anyhow` is permitted in binary crates per D-3.2; `tcp-echo-server` is a binary crate, so `anyhow` is in scope here.)

- `tests/helpers/tcp-echo-server/src/main.rs` starts with `#![forbid(unsafe_code)]`. Contract:
  - Hand-parsed argv mirroring `crates/envoy-bin/src/argv.rs` (from phase-01 Task 12): `--port <u16>` required, `--help`, `--version`. `ArgvError` typed via `thiserror` (variants: `MissingFlag(&'static str)`, `MissingValue`, `InvalidPort`, `Trailing`, `HelpRequested`, `VersionRequested`).
  - Runtime: `tokio::net::TcpListener::bind(("127.0.0.1", port))`, accept loop with `tokio::select!` between `accept()` and `tokio::signal::ctrl_c()`, each accepted stream spawned onto a `tokio::task::JoinSet` running `let (mut r, mut w) = stream.split(); tokio::io::copy(&mut r, &mut w).await`. On shutdown: stop accepting, drain with `DRAIN_BUDGET = Duration::from_secs(5)`, abort stragglers, return 0.
  - Logs on `stderr` via `tracing_subscriber::fmt`, similar to `envoy-bin`.
  - Exit codes: `0` clean, `1` runtime error, `2` argv error. Mirrors `envoy-bin`'s argv-vs-runtime-vs-clean exit-code convention established in phase 01.

- Unit tests in `tests/helpers/tcp-echo-server/src/main.rs::tests` (eight tests):
  - `argv_parses_port` — `--port 10042` → `Ok(Args { port: 10042 })`.
  - `argv_rejects_missing_port_flag` — empty argv → `Err(ArgvError::MissingFlag("--port"))`.
  - `argv_rejects_missing_value` — `--port` alone → `Err(ArgvError::MissingValue)`.
  - `argv_rejects_non_numeric_port` — `--port abc` → `Err(ArgvError::InvalidPort)`.
  - `argv_rejects_trailing_argument` — `--port 10042 --junk` → `Err(ArgvError::Trailing)`.
  - `argv_shows_help` — `--help` → `Err(ArgvError::HelpRequested)` (exit 0 path via main's translation).
  - `echoes_round_trip` — `#[tokio::test(flavor="multi_thread")]`: reserve a port, spawn the server in a task on that port, connect, write 32-byte payload, `read_exact` 32 bytes, assert equal.
  - `drain_exits_within_budget` — spawn the server, open a stalled connection (peer stops reading), fire shutdown, assert server returns within `DRAIN_BUDGET + ε` (ε = 500 ms).

### D4 — Phase-01 rollover I3: `decode_chunked` unit tests

`tests/differential/src/lib.rs::tests` gains four unit tests closing phase-01 REVIEW §9 starter item I3. The `decode_chunked` helper lives at `tests/differential/src/lib.rs` (landed during phase 01 for HTTP chunked-transfer handling in the admin `/ready` fixture); it is currently implemented but unit-test-unverified.

- `decode_chunked_empty_stream` — input `b"0\r\n\r\n"` → `Ok(vec![])`.
- `decode_chunked_with_chunk_extension` — input `b"5;name=value\r\nhello\r\n0\r\n\r\n"` → `Ok(b"hello".to_vec())`. (Envoy never emits chunk extensions for `/ready`; this test proves the decoder tolerates them rather than silently mis-parsing.)
- `decode_chunked_truncated_size_line` — input missing `\r\n` after the chunk size or hex-byte truncation → `Err(...)`, not silent `Ok(partial)`.
- `decode_chunked_ignores_trailer_bytes` — input `b"3\r\nabc\r\n0\r\nTrailer-Name: value\r\n\r\n"` → `Ok(b"abc".to_vec())`.

No production code change; tests only. The phase-01 REVIEW cross-reference (§9 item I3) is closed-in-02.1 via this deliverable.

### D5 — Fuzz corpus extension

`crates/envoy-config/fuzz/corpus/parse_bootstrap/` gains two seeds to keep the fuzzer's structural coverage current with the phase-02 grammar:

- `tcp_proxy_single_endpoint.yaml` — a full bootstrap fixture with listener → tcp_proxy → cluster with one endpoint. Concrete contents (literal file, no template substitution — the fuzz corpus is not rendered through the harness):

    ```yaml
    node:
      id: fuzz-seed-single
      cluster: fuzz
    static_resources:
      listeners:
        - name: l
          address:
            socket_address:
              address: 127.0.0.1
              port_value: 10000
          filter_chains:
            - filters:
                - name: envoy.filters.network.tcp_proxy
                  typed_config:
                    "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                    stat_prefix: ingress_tcp
                    cluster: backend
      clusters:
        - name: backend
          type: STATIC
          lb_policy: ROUND_ROBIN
          load_assignment:
            cluster_name: backend
            endpoints:
              - lb_endpoints:
                  - endpoint:
                      address:
                        socket_address:
                          address: 127.0.0.1
                          port_value: 10001
    ```

- `tcp_proxy_round_robin_triple.yaml` — same shape with three `lb_endpoints` under the single `LocalityLbEndpoints` (e.g., `127.0.0.1:10001`, `127.0.0.1:10002`, `127.0.0.1:10003`), exercising the multi-endpoint parse path.

No new fuzz target ships. The existing `parse_bootstrap` target covers the extended grammar because `TypedConfig`, `Cluster`, and nested endpoint shapes are all reachable via `envoy_config::parse_bootstrap`. The fuzz job's `-max_total_time=30` budget (per ADR-0010) is unchanged.

### D6 — CI workflow

`.github/workflows/ci.yml` changes: none. The existing `build` job runs `cargo test --workspace`, which picks up the new `envoy-cluster` and `tcp-echo-server` crates automatically. The existing `fuzz` job exercises the extended corpus via the same `cargo fuzz run parse_bootstrap -- -max_total_time=30` invocation. `cargo build --workspace --all-targets` compiles the new `tcp-echo-server` binary; the `CARGO_BIN_EXE_tcp-echo-server` env var becomes available to in-same-package tests (the `tests/helpers/tcp-echo-server/src/main.rs::tests` tests, but *not* to `tests/differential/` — cross-package `CARGO_BIN_EXE_*` is unavailable per Cargo semantics; `tests/differential/` uses `env!("CARGO_MANIFEST_DIR") + "/../../target/<profile>/tcp-echo-server"` in 02.2 when the `TcpProxyBackend` helper lands).

### D7 — ADR-0014 (renumbered from parent-SPEC ADR-0013) — YAML-native `typed_config`

Exactly one ADR lands during 02.1 execution, appended to `docs/envoy-rust/DECISIONS.md`. See §7 of this SPEC for the ADR text.

No other ADRs in 02.1. ADR-0015 (host-docker + host-gateway, renumbered from parent-SPEC ADR-0014) and ADR-0016 (`enable_half_close: false` default, renumbered from parent-SPEC ADR-0015) both land during sub-phase 02.2 — they motivate harness plumbing and fixture-0003 posture that are 02.2-scoped.

Additional ADRs may be required during execution per D-3.5 if `cargo deny check` flips red on any new transitive surface from the `tcp-echo-server` helper crate (`tracing-subscriber`'s dep graph is already in scope via `envoy-bin`, so no new exposure is expected; verify during execution).

---

## 4. Non-goals (deferred to 02.2 or later phases)

Deferred explicitly to sub-phase 02.2:

- `envoy-listener` crate (listener bind + accept + drain).
- `envoy-tcp` crate (TCP proxy filter data-plane: `tokio::io::copy` bidirectional, `TcpProxyError`).
- `envoy-bin` wiring: `ClusterManager` construction + dispatch between echo (`envoy.filters.network.echo`) and tcp_proxy (`envoy.filters.network.tcp_proxy`) branches in `main::run`.
- `tests/fixtures/0003-tcp-proxy/` (envoy.yaml, envoy-rust.yaml, inputs/payload.bin, expectations.yaml, README.md).
- Differential harness extensions: `TcpProxyBackend` module (`tests/differential/src/backend.rs`), `render_yaml` backend-key substitution, `run_fixture` dispatch on `{{BACKEND_PORT}}`, upstream `with_host("host.docker.internal", Host::HostGateway)`.
- Phase-01 rollovers I4 (admin 8 KiB cap tightening in `crates/envoy-bin/src/admin.rs`) and M1 (stale `TODO(phase-01)` retarget in `tests/differential/src/subject.rs`).
- ADR-0015 (host-docker + host-gateway), ADR-0016 (`enable_half_close: false` default).
- Integration test `crates/envoy-bin/tests/tcp_proxy.rs` backstop.
- Docker-gated integration test `tests/differential/tests/tcp_proxy.rs`.

Deferred to later phases (unchanged from parent-SPEC §4):

- TLS on listener (downstream) or cluster (upstream) — phase 03.
- `envoy.filters.network.echo`'s deprecation — stays supported for fixture 0001.
- `idle_timeout`, `max_connect_duration`, `tunneling_config`, `access_log` on tcp_proxy — phase 06 and later.
- Multiple listeners per process — `listeners.len() ∈ {0, 1}` cap stays.
- Cluster health checking, outlier detection, circuit breakers — §9 upstream-robustness family.
- `type: LOGICAL_DNS`, `type: STRICT_DNS`, `type: EDS` — 02.1 accepts only `STATIC`.
- `lb_policy: LEAST_REQUEST`, `RANDOM`, `RING_HASH`, `MAGLEV`, subset LB, locality-weighted LB, priority LB, panic thresholds — §9 load-balancing family.
- Listener filters (`listener_filters`), filter chain matchers (`filter_chain_matcher`), transport_socket, `per_connection_buffer_limit_bytes`, `connection_balance_config` — out of phase-02 surface.
- Filter chain framework / extension registry (trait registry, per-route config, iteration protocol) — phase 07.
- Stats subsystem, access logs, Prometheus — phase 06.
- Admin endpoints beyond phase 01's `/ready` (`/stats`, `/clusters`, `/config_dump`, `/server_info`, `/drain_listeners`, `/healthcheck/fail`) — phase 08.
- Distribution-equivalence assertion on round-robin — parent-brainstorm Q1 decision: unit-test-only.
- `envoy-protos` crate + `prost` / `prost-build` + proto-tree vendoring — xDS family (§9) per ADR-0014.
- `envoy-filter/` crate — phase 07.
- Long-budget nightly fuzz CRON — a future, scheduled phase.

---

## 5. Splitting guidance for the planner

Estimated scope:

| Surface | Net LoC (impl + tests) |
|---|---|
| envoy-config schema (filter envelope + cluster topology + 16 new tests) | ~120 + ~250 |
| envoy-cluster crate (data model + round-robin LB + 8 tests) | ~150 + ~200 |
| tcp-echo-server helper crate (runtime + 8 tests including argv parse) | ~80 + ~80 |
| Phase-01 rollover I3 (4 `decode_chunked` unit tests) | ~40 |
| Fuzz corpus seeds (2 new YAML fixtures) | ~60 |
| ADR-0014 (docs) | ~0 |
| **Total** | **~980 LoC; ~13 tasks** |

Both `BOOTSTRAP_PROMPT.md` §6 gates (> ~25 tasks OR > ~1500 LoC) hold comfortably at ~13 tasks / ~980 LoC. **Do not split 02.1 further**. If the plan as actually written crosses either gate mid-write, invoke `superpowers:systematic-debugging` before attempting a nested split — nested splits of a split sub-phase were not anticipated at the parent-phase brainstorm and deserve a fresh root-cause analysis (scope creep vs. planner overdecomposition).

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the planner resolves them in-plan rather than mid-execution.

1. **Task ordering: `envoy-config` extensions ship before `envoy-cluster`.** `envoy-cluster::from_bootstrap` consumes `envoy_config::Bootstrap`, which means its signature depends on the new types (`Cluster`, `LoadAssignment`, etc.) being landed first. The tcp-echo-server helper is independent and can land in parallel, but it is grouped after `envoy-cluster` in the plan-writer's task sequence to keep test-run cadence monotone (each task block either touches `envoy-config` or lands a new crate).

2. **`from_bootstrap`'s empty-cluster check duplicates a validator rule.** `envoy-config::bootstrap::validate` rejects `load_assignment.endpoints` with zero total `LbEndpoint`s (`ConfigError::EmptyClusterEndpoints`). `envoy-cluster::from_bootstrap` also rejects `EmptyCluster` (`ClusterError::EmptyCluster`). This is defense-in-depth: the cluster crate is a library with its own invariants, and its `pick_endpoint` relies on `!endpoints.is_empty()`. Review should flag any later phase that removes one of the two checks without also removing the invariant.

3. **Round-robin's `AtomicUsize` cursor wraps around.** `cursor.fetch_add(1, Ordering::Relaxed) % endpoints.len()` is correct even when the counter wraps (Rust `AtomicUsize::fetch_add` wraps on overflow, and the modulo is stable under wraparound because `endpoints.len()` is bounded). `Relaxed` ordering is sufficient because there's no cross-observation of the cursor between threads — each call reads-modifies-writes atomically, and "which endpoint" doesn't need any happens-before relationship with other operations.

4. **Error enum constructors use `#[from]` where possible.** `thiserror`'s `#[from]` on `ConfigError::Yaml` (phase-01 artifact, unchanged) keeps call-site code concise. `ClusterError::EndpointParse` uses named fields (`{ cluster, addr, source }`) which don't work with `#[from]`; use `.map_err(|source| ClusterError::EndpointParse { cluster, addr, source })` at those call sites.

5. **`tcp-echo-server`'s argv parser reuses `envoy-bin`'s idiom, not code.** The `argv.rs` pattern (hand-parsed `parse_argv(&[String]) -> Result<Args, ArgvError>`) is copied structurally but not literally. Cross-crate argv sharing would mean extracting a third crate (e.g., `envoy-argv`); not worth it for two ~100-LoC parsers that don't share argument shapes. If a third argv-parsing binary crate lands in a future phase, consider extraction then under its own ADR.

6. **`envoy-bin` is not re-wired in 02.1.** `envoy-bin/Cargo.toml` does *not* grow an `envoy-cluster` dep in 02.1 — the cluster crate has no consumer yet. It grows one in sub-phase 02.2 alongside `envoy-listener` and `envoy-tcp`. Review should flag a 02.1-proposed edit to `envoy-bin/Cargo.toml` that pulls in `envoy-cluster` pre-emptively.

7. **Parser acceptance vs. runtime dispatch asymmetry.** After 02.1, `envoy-config::parse_bootstrap` accepts any valid tcp_proxy-shaped YAML, but `envoy-bin::main::run` still errors out at runtime dispatch because the `envoy.filters.network.tcp_proxy` branch has no implementation. This is an intentional, temporary gap — it enables 02.1's acceptance tests and fuzz corpus to land in isolation before 02.2 wires the runtime. A deliberate integration-test gap is *not* a violation of D-3.6 (`no failing unit tests` ≠ `every parse-accepted config must run`). 02.2's acceptance is what closes the gap.

8. **`deny_unknown_fields` discipline on every new struct.** Each of the six new struct/enum types (`TypedConfig`, `TcpProxyConfig`, `Cluster`, `LoadAssignment`, `LocalityLbEndpoints`, `LbEndpoint`) gets `#[serde(deny_unknown_fields)]`. The sixteen new unit tests in §D2 include `deny_unknown_fields` regression coverage at every struct level — mirrors phase-01 Task 4 Step 4's discipline and closes the phase-00 N2 cross-reference precedent.

9. **`tracing-subscriber` license exposure.** The `tcp-echo-server` crate takes `tracing-subscriber` as a direct dep. `tracing-subscriber` is already transitively pulled in by `envoy-bin` via its own direct dep (landed in phase 01), so `cargo deny check`'s transitive surface should be unchanged. Verify at phase-done gate time; if a new license surface appears, land it under a new ADR (likely ADR-0017 or later, after 02.2's ADR-0015/0016).

10. **No `tokio` in `envoy-cluster`.** The cluster crate is synchronous: its public API is `from_bootstrap`, `get`, `pick_endpoint` — no `async fn`, no `Future`, no `tokio::spawn`. Keeping async out of the cluster layer in 02.1 matches Envoy's internal structure (cluster manager is sync; async lives at the I/O boundary) and keeps 02.2's `envoy-tcp::handle` call into `pick_endpoint()` zero-cost synchronous.

11. **`HashMap` vs. `BTreeMap` for `ClusterManager`'s cluster map.** Use `HashMap<String, Arc<Cluster>>`. Insertion order is irrelevant (look-up is by name), and cluster-count is small (O(10) expected). `BTreeMap` would buy deterministic iteration order for debug output — not needed at this phase. If a later phase (08 admin `/clusters`) needs ordered output, it sorts at output time rather than changing the storage map.

---

## 7. ADRs expected from this sub-phase

Exactly one ADR lands in 02.1, appended to `docs/envoy-rust/DECISIONS.md`. The numbering reflects the ADR-0013 split: parent-SPEC §7's ADR-0013 becomes this ADR-0014.

### ADR-0014 — YAML-native `typed_config` deserialization until the xDS/protos family lands

- Context: Sub-phase 02.1 is the first phase to surface Envoy's `typed_config` envelope (`envoy.filters.network.tcp_proxy`). The `envoy-protos` crate + `prost` / `prost-build` + upstream proto-tree vendoring were deferred at phase-00 bootstrap to the xDS family (ROADMAP §9). 02.1 must choose: bring the protos stack forward now, or ship a narrower shim.
- Options considered:
  - **(i) YAML-native — one Rust enum discriminated on the `@type` URL string literal, fields deserialized by serde.** Minimal surface, scoped to this sub-phase's needs. Grows one enum variant per filter across phases 04/05/06 until the xDS family ships.
  - **(ii) Bring `prost` + `envoy-protos` in as part of 02.1.** Pulls forward multi-phase proto-tree vendoring. Out of ROADMAP row-02 scope; would trigger a further split by itself.
  - **(iii) Non-Envoy `raw_config` YAML key.** Diverges `envoy.yaml` and `envoy-rust.yaml` on filter shape, breaking the fixture principle that configs are initially identical.
- Decision: (i). `TypedConfig` enum in `envoy-config::bootstrap` with a `#[serde(tag = "@type")]` discriminator; one variant for TCP proxy in 02.1; extended per filter across future phases.
- Rationale: keeps 02.1 within row-02 scope; defers the `envoy-protos` multi-phase work until it pays for itself. Reviewable by shape — a stranger reading the YAML can see which filters are supported.
- Consequences: unknown `@type` URLs reject at parse time via serde's tagged-enum default behavior. Every new filter in phase 04 / 05 / 06 extends the enum by one variant. An `envoy-protos` supersession ADR in the xDS family re-routes the `@type` URL to prost-generated message types in one sweep and retires this shim.

---

## 8. Artifacts this sub-phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/02.1-config-cluster/PLAN.md`
- `docs/envoy-rust/phases/02.1-config-cluster/PROGRESS.md`
- `docs/envoy-rust/phases/02.1-config-cluster/REVIEW.md`
- `crates/envoy-cluster/Cargo.toml`
- `crates/envoy-cluster/src/lib.rs`
- `crates/envoy-cluster/src/cluster.rs`
- `tests/helpers/tcp-echo-server/Cargo.toml`
- `tests/helpers/tcp-echo-server/src/main.rs`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml`

Amended during execution:

- Root `Cargo.toml` — add `crates/envoy-cluster`, `tests/helpers/tcp-echo-server` to `[workspace] members`. `envoy-listener` and `envoy-tcp` are *not* added here — they land in 02.2.
- `crates/envoy-config/src/bootstrap.rs` — add `TypedConfig`, `TcpProxyConfig`, fleshed-out `Cluster`/`LoadAssignment`/`LocalityLbEndpoints`/`LbEndpoint`/`Endpoint`, extended validator rules, 16 new unit tests.
- `crates/envoy-config/src/lib.rs` — re-export new public types (`TypedConfig`, `TcpProxyConfig`, `ClusterType`, `LbPolicy`, `LoadAssignment`, `LocalityLbEndpoints`, `LbEndpoint`, `Endpoint`); extend `ConfigError` with `MissingTypedConfig`, `UnexpectedTypedConfig`, `UnknownCluster`, `LoadAssignmentNameMismatch`, `EmptyClusterEndpoints`.
- `tests/differential/src/lib.rs` — add 4 `decode_chunked` unit tests (I3). No other changes in 02.1.
- `docs/envoy-rust/DECISIONS.md` — ADR-0014 appended.
- `docs/envoy-rust/ROADMAP.md` — row 02.1 `status` → `done` in the final commit.
- `docs/envoy-rust/STATE.md` — active → `02.2-listener-tcp-proxy`, next-skill → `superpowers:writing-plans`, state → 2 (SPEC.md exists, PLAN.md does not; 02.2's SPEC landed alongside this one during the ADR-0013 split session).
- `deny.toml` — only if `cargo deny check` flags a new transitive license from `tcp-echo-server` (expected: no; `tracing-subscriber` is already in scope via `envoy-bin`).

Not touched in 02.1 (belong to 02.2 or are frozen):

- `crates/envoy-bin/Cargo.toml`, `crates/envoy-bin/src/main.rs`, `crates/envoy-bin/src/admin.rs` — untouched in 02.1.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/` — unchanged.
- `tests/differential/Cargo.toml` — unchanged (no new deps required for the 4 I3 tests; they use `decode_chunked` which is already in-crate).
- `tests/differential/src/subject.rs`, `tests/differential/src/upstream.rs` — untouched in 02.1.
- `.github/workflows/ci.yml` — untouched.
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `50349da`.

---

## 9. Final commit message format (for state 6 of the 02.1 lifecycle)

```
phase 02.1: Config schema + cluster manager + echo-server helper [ADR-0014]

envoy-config grows the typed_config envelope (TypedConfig / TcpProxyConfig)
and the five-level Cluster / LoadAssignment / LocalityLbEndpoints /
LbEndpoint / Endpoint topology with 16 new validator tests.
envoy-cluster crate lands with static-cluster data model + round-robin LB
cursor (AtomicUsize) and 8 unit tests. tests/helpers/tcp-echo-server/ is
the first crate under tests/helpers/ — a ~160-LoC tokio echo binary that
sub-phase 02.2's fixture 0003 will dial. Phase-01 REVIEW §9 starter
item I3 (decode_chunked unit tests) lands alongside.

Differential surface: no new fixture in 02.1; fixtures 0001-tcp-echo and
  0002-static-admin-ready remain green (unchanged). Fuzz corpus for
  parse_bootstrap now includes single-endpoint and three-endpoint
  tcp_proxy seeds.
Conformance: none.
```
