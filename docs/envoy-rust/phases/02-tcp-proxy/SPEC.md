# Phase 02 — Listener + TCP proxy filter + static cluster + round-robin LB (plaintext)

- **Phase id:** `02`
- **Title:** Listener + TCP proxy filter + static cluster + round-robin LB (plaintext)
- **Depends on:** `01` (done as of commit `aef36ce`).
- **Differential surface when done:** `tests/fixtures/0003-tcp-proxy/` green against upstream `envoyproxy/envoy:v1.33.0` (byte-exact payload round-trip through a TCP-proxy → in-tree `tcp-echo-server` backend). Pre-existing fixtures `0001-tcp-echo` and `0002-static-admin-ready` remain green.
- **Seeded by:** `BOOTSTRAP_PROMPT.md` §8 row 02; `ROADMAP.md` row 02.

This spec is the design contract for phase 02. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is intentionally concrete enough to be turned into a plan by a stranger with zero prior context per doctrine D-3.4.

---

## 1. Goal and acceptance signal

**Goal.** Extend envoy-rust so that a realistic Envoy static bootstrap with a single `envoy.filters.network.tcp_proxy` filter routing to a static `STATIC` cluster with `lb_policy: ROUND_ROBIN` binds a listener, accepts downstream TCP connections, picks an upstream endpoint, dials it plaintext, and bidirectionally copies bytes until either side closes. This phase stands up the first real data-plane path in the project: all prior phases have been scaffolding, config parsing, and the admin `/ready` endpoint.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to this phase's feature surface:

- (a) the new differential fixture `tests/fixtures/0003-tcp-proxy/` is green;
- (b) the pre-existing differential fixtures `tests/fixtures/0001-tcp-echo/` and `tests/fixtures/0002-static-admin-ready/` remain green;
- (c) no conformance suites run this phase (first one — `h2spec` — attaches in phase 05);
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`-max_total_time=30`) against an extended corpus that includes TCP-proxy-shaped seeds; no new fuzz target ships this phase;
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- (f) `REVIEW.md` for this phase is approved.

**Scope shape (brainstorm-fixed choices).** Five forks were resolved during brainstorming; downstream planning inherits them:

1. **Round-robin LB differential scope — minimum.** One differential fixture with a *single* endpoint exercises the TCP-proxy data-plane end-to-end. Round-robin correctness over N≥3 endpoints is proved by unit tests at the `envoy-cluster` boundary (`AtomicUsize` cursor; assert sequence `0,1,2,0,1,2,...`). No differential assertion on distribution, which would run into Envoy's per-worker RR sharding (`source/common/upstream/load_balancer_impl.h` at `v1.33.0`) and require an ADR carving out a softened equivalence rule.
2. **Upstream backend — in-tree Rust helper crate.** A new `tests/helpers/tcp-echo-server/` crate ships a ~60 LoC tokio echo server that both proxies dial. No third-party image, no second Envoy container.
3. **Crate layout — three new crates.** `envoy-listener`, `envoy-cluster`, `envoy-tcp` — one per primitive, matching the §4 layout of `BOOTSTRAP_PROMPT.md`. Phase 07 owns the filter-chain framework (`envoy-filter`); nothing in phase 02 preempts it.
4. **`typed_config` deserialization — YAML-native.** A Rust enum `TypedConfig` discriminated on the `@type` URL string literal. No `prost` / `envoy-protos` in phase 02; those defer to the xDS family per ADR-0013 (§7).
5. **Phase-01 rollovers — all three folded in.** I3 (`decode_chunked` unit tests), I4 (admin 8 KiB cap tightening), M1 (stale TODO retarget) ride with this phase. See §D9.

---

## 2. Behavior-contract scope for phase 02

Phase 02 continues to exercise only row 2 of the `BEHAVIOR_CONTRACT.md` §7.2 equivalence matrix:

- **Response body — Byte-exact for deterministic handlers.** The `drive_tcp` helper (ADR-0006/0007) writes `payload` bytes downstream, reads exactly `payload.len()` bytes back, asserts byte-equality, and closes. The echo backend guarantees the 1:1 contract regardless of whether the bytes pass through Envoy's echo *filter* (fixture 0001) or through `tcp_proxy → tcp-echo-server` (fixture 0003).

No other dimension is engaged. No response status (TCP, no HTTP). No access logs (phase 06). No stats (phase 06). No headers (phase 04 for HTTP; TCP has none). No xDS (§9).

**No `BEHAVIOR_CONTRACT.md` edits in phase 02.** The currently-empty subsections (`Header allow-list`, `Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances`) remain empty.

---

## 3. Deliverables

### D1 — New library crate `crates/envoy-cluster/`

Added to the root `Cargo.toml` `[workspace] members`. Owns the static-cluster data model, endpoint resolution, and the round-robin LB state.

- `crates/envoy-cluster/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Dependencies from the D-3.2 permitted-foundations list only: `envoy-config = { path = "../envoy-config" }`, `thiserror`.
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

- `crates/envoy-cluster/src/cluster.rs` hosts `Cluster` with fields `name: String`, `endpoints: Vec<SocketAddr>`, `cursor: AtomicUsize`. `pick_endpoint`: `let i = self.cursor.fetch_add(1, Ordering::Relaxed); Some(self.endpoints[i % self.endpoints.len()])` (the modulo guards against counter overflow over very long runs; `Relaxed` is sufficient because there is no cross-observation of the cursor value). Empty-endpoint guard is enforced at `from_bootstrap` construction — `pick_endpoint` never has to check.

- `from_bootstrap` iterates `bootstrap.static_resources.clusters`, validates each (`type == STATIC`, `lb_policy == ROUND_ROBIN`, `load_assignment.cluster_name == name`, ≥1 `lb_endpoint`, each `endpoint.address.socket_address` parses as `SocketAddr` via `format!("{}:{}", addr, port).parse()`), and inserts into the `HashMap`. Unsupported `ClusterType`/`LbPolicy` variants are rejected at the `envoy-config` layer (§D3) so they cannot reach `envoy-cluster`.

- Unit tests in `crates/envoy-cluster/src/cluster.rs::tests`:
  - `pick_endpoint_cycles_over_three_endpoints` — N=3 endpoints; call `pick_endpoint` 7 times; assert sequence `0,1,2,0,1,2,0`.
  - `pick_endpoint_is_stable_under_concurrent_calls` — N=3; spawn 1000 concurrent `pick_endpoint` calls across threads; assert each endpoint is picked ~333 times (±10%).
  - `from_bootstrap_rejects_empty_cluster` — cluster with zero `lb_endpoints` → `ClusterError::EmptyCluster`. (Note: `envoy-config` also rejects this at parse time; `envoy-cluster`'s check is defense-in-depth.)
  - `from_bootstrap_rejects_duplicate_cluster_name` — two clusters named `"backend"` → `ClusterError::DuplicateClusterName`.
  - `from_bootstrap_rejects_malformed_endpoint_address` — cluster with `address: "not-a-host"` → `ClusterError::EndpointParse`.
  - `from_bootstrap_builds_single_endpoint_cluster` — happy path.
  - `from_bootstrap_builds_three_endpoint_cluster` — happy path for the round-robin test.
  - `handle_clone_shares_cursor` — `ClusterHandle::clone` returns a handle whose `pick_endpoint` advances the *same* `AtomicUsize`, proving `Arc` semantics.

### D2 — New library crate `crates/envoy-listener/`

Added to root `[workspace] members`. Owns listener binding, accept loops, and graceful drain.

- `crates/envoy-listener/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Deps: `envoy-config = { path = "../envoy-config" }`, `tokio` (with features `rt`, `net`, `macros`, `time`, `sync`), `thiserror`, `tracing`. Dev-deps: `tokio` additionally gains `rt-multi-thread`, `io-util` for tests.

- `crates/envoy-listener/src/lib.rs` starts with `#![forbid(unsafe_code)]`. Public surface:

    ```rust
    pub struct Listener { /* TcpListener + handler */ }

    impl Listener {
        pub async fn bind(
            cfg: &envoy_config::Listener,
            handler: std::sync::Arc<dyn ConnectionHandler>,
        ) -> Result<Self, ListenerError>;

        pub async fn serve(
            self,
            shutdown: impl std::future::Future<Output = ()> + Send + 'static,
        ) -> Result<(), ListenerError>;

        pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr>;
    }

    pub trait ConnectionHandler: Send + Sync + 'static {
        fn handle(
            &self,
            downstream: tokio::net::TcpStream,
        ) -> futures::future::BoxFuture<'static, Result<(), anyhow::Error>>;
    }

    #[derive(Debug, thiserror::Error)]
    pub enum ListenerError {
        #[error("binding listener address {addr}: {source}")]
        Bind { addr: std::net::SocketAddr, #[source] source: std::io::Error },
        #[error("accept loop terminated: {0}")]
        Accept(#[source] std::io::Error),
        #[error("drain timed out after {0:?}")]
        DrainTimeout(std::time::Duration),
    }
    ```

  `futures` is a forbidden direct dep — we must *not* pull `futures = "…"` in. Instead, the `BoxFuture` alias lives in `envoy-listener` itself as `pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;` so no new crate is needed. `anyhow` is permitted only in the binary crate per D-3.2; the trait returns `Result<(), anyhow::Error>` by way of the binary-crate wiring — re-express it as `Result<(), Box<dyn std::error::Error + Send + Sync>>` to stay within permitted foundations for library crates. (The planner may revisit this choice; see signpost 3.)

- `Listener::bind` resolves `cfg.address.socket_address` to a `SocketAddr` (same helper pattern as phase-01's `resolve_socket`), calls `tokio::net::TcpListener::bind`, stores the bound listener + `Arc<dyn ConnectionHandler>`. Returns `ListenerError::Bind` with the pre-resolved `addr` so errors point at the intended bind target.

- `Listener::serve` uses the documented `tokio::select!` + `JoinSet<Result<()>>` shape (mirror of `admin::serve` / `echo::serve`):

    ```rust
    const DRAIN_BUDGET: Duration = Duration::from_secs(5);
    let mut join_set: JoinSet<Result<(), _>> = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = self.listener.accept() => {
                let (stream, _peer) = accepted.map_err(ListenerError::Accept)?;
                let handler = self.handler.clone();
                join_set.spawn(async move { handler.handle(stream).await });
            }
            Some(done) = join_set.join_next(), if !join_set.is_empty() => {
                if let Err(e) = done { tracing::warn!(%e, "connection task failed"); }
            }
        }
    }
    // drain
    let drain = async {
        while let Some(res) = join_set.join_next().await {
            if let Err(e) = res { tracing::warn!(%e, "connection task failed during drain"); }
        }
    };
    if tokio::time::timeout(DRAIN_BUDGET, drain).await.is_err() {
        join_set.abort_all();
        return Err(ListenerError::DrainTimeout(DRAIN_BUDGET));
    }
    Ok(())
    ```

- Unit tests in `crates/envoy-listener/src/lib.rs::tests`:
  - `bind_returns_socket_address` — ephemeral port, assert `local_addr()` matches the bound port.
  - `serves_accepts_and_dispatches_to_handler` — trivial `EchoHandler` for test; open a TCP connection, send bytes, receive them back.
  - `serves_honors_shutdown_signal` — spawn `serve`, send shutdown, assert return within DRAIN_BUDGET + ε.
  - `serves_drains_in_flight_connection_within_budget` — hold one in-flight connection alive, fire shutdown, assert return within `DRAIN_BUDGET + ε`.
  - `serves_aborts_stragglers_past_drain_budget` — the in-flight connection intentionally stalls past 5s; assert `Err(ListenerError::DrainTimeout)`.
  - `bind_fails_cleanly_on_address_in_use` — bind twice to the same port; second call returns `ListenerError::Bind`.

### D3 — `envoy-config` schema extensions

`crates/envoy-config/src/bootstrap.rs` gains the typed_config envelope, cluster topology, and the associated validator rules. The `Node` open-schema asymmetry is not widened.

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

  - Per listener, the allowed filter names are `{envoy.filters.network.echo, envoy.filters.network.tcp_proxy}`. Echo still accepted for fixture 0001. Any other name → `ConfigError::UnsupportedFilter` (unchanged variant).
  - `tcp_proxy` requires `typed_config: Some(TypedConfig::TcpProxy { .. })`; missing → new `ConfigError::MissingTypedConfig("envoy.filters.network.tcp_proxy")`.
  - `echo` requires `typed_config: None`; present → new `ConfigError::UnexpectedTypedConfig("envoy.filters.network.echo")`.
  - For each tcp_proxy filter, `typed_config.cluster` must name a cluster in `static_resources.clusters`. Missing → new `ConfigError::UnknownCluster(String)`.
  - Per cluster: `load_assignment.cluster_name == name`. Mismatch → new `ConfigError::LoadAssignmentNameMismatch { cluster: String, assignment: String }`.
  - Per cluster: `load_assignment.endpoints` flattened across all `LocalityLbEndpoints` must have ≥1 total `LbEndpoint`. Zero → new `ConfigError::EmptyClusterEndpoints(String)` (named distinctly from `envoy-cluster::EmptyCluster` to keep the two layers' errors distinguishable).
  - `listeners.len() ∈ {0, 1}` cap stays; `clusters.len()` is unbounded.

- Unit tests appended to `crates/envoy-config/src/bootstrap.rs::tests`:
  - `parses_bootstrap_with_tcp_proxy_filter` — full happy-path fixture (listener → tcp_proxy → cluster with one endpoint).
  - `parses_bootstrap_with_round_robin_multi_endpoint_cluster` — N=3 endpoints parse.
  - `rejects_tcp_proxy_without_typed_config`.
  - `rejects_echo_with_typed_config`.
  - `rejects_typed_config_unknown_type_url`.
  - `rejects_unknown_tcp_proxy_config_field` (e.g. `idle_timeout: 0s`).
  - `rejects_cluster_type_logical_dns` — asserts `ClusterType::Static` is the only accepted variant.
  - `rejects_lb_policy_least_request`.
  - `rejects_tcp_proxy_naming_missing_cluster`.
  - `rejects_load_assignment_cluster_name_mismatch`.
  - `rejects_empty_lb_endpoints`.
  - `rejects_malformed_endpoint_address` (e.g. `address: "not-a-host"`).
  - `rejects_unknown_cluster_field`.
  - `rejects_unknown_load_assignment_field`.
  - `rejects_unknown_locality_lb_endpoints_field`.
  - `rejects_unknown_lb_endpoint_field`.
  - `rejects_unknown_endpoint_field`.

  (Sixteen new tests; `deny_unknown_fields` regression coverage for each new struct level mirrors phase-01 Task 4's Step 4 discipline.)

### D4 — New library crate `crates/envoy-tcp/`

Added to root `[workspace] members`. Owns the TCP proxy filter logic.

- `crates/envoy-tcp/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Deps: `envoy-cluster = { path = "../envoy-cluster" }`, `envoy-config = { path = "../envoy-config" }`, `envoy-listener = { path = "../envoy-listener" }`, `tokio` (features `rt`, `net`, `io-util`, `macros`), `thiserror`, `tracing`. Dev-deps: `tokio` adds `rt-multi-thread`.

- `crates/envoy-tcp/src/lib.rs` starts with `#![forbid(unsafe_code)]`. Public surface:

    ```rust
    pub struct TcpProxy { /* Arc<envoy_cluster::ClusterHandle> + cfg fields */ }

    impl TcpProxy {
        pub fn new(
            cluster: envoy_cluster::ClusterHandle,
            cfg: &envoy_config::TcpProxyConfig,
        ) -> Self;
    }

    impl envoy_listener::ConnectionHandler for TcpProxy {
        fn handle(&self, downstream: tokio::net::TcpStream)
            -> envoy_listener::BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>;
    }

    #[derive(Debug, thiserror::Error)]
    pub enum TcpProxyError {
        #[error("no healthy endpoint available for cluster '{cluster}'")]
        NoHealthyEndpoint { cluster: String },
        #[error("connecting to upstream {addr}: {source}")]
        UpstreamConnect { addr: std::net::SocketAddr, #[source] source: std::io::Error },
        #[error("bidirectional copy failed: {source}")]
        CopyFailed { #[source] source: std::io::Error },
    }
    ```

- `handle` implementation:
  1. `let addr = self.cluster.pick_endpoint().ok_or(TcpProxyError::NoHealthyEndpoint { cluster: self.cluster_name.clone() })?;`
  2. `let upstream = tokio::net::TcpStream::connect(addr).await.map_err(|source| TcpProxyError::UpstreamConnect { addr, source })?;`
  3. `let (mut dr, mut dw) = downstream.into_split(); let (mut ur, mut uw) = upstream.into_split();`
  4. `let d2u = tokio::io::copy(&mut dr, &mut uw); let u2d = tokio::io::copy(&mut ur, &mut dw); tokio::try_join!(d2u, u2d).map_err(|source| TcpProxyError::CopyFailed { source })?;`
  5. `tracing::debug!(%addr, d2u_bytes = ?…, u2d_bytes = ?…, "tcp proxy connection complete");`
  6. `Ok(())`.

  Half-close is **not** set (`enable_half_close: false`, Envoy's v1.33.0 default). Per ADR-0015, the `drive_tcp` 1:1 echo pattern does not depend on FIN propagation; a fixture that needs it lands its own ADR flipping the toggle.

- Per-connection errors (from `handle`) return `Err(Box<dyn …>)`. The listener's accept loop logs them at `warn!` and drops the connection. The listener remains up.

- Unit tests in `crates/envoy-tcp/src/lib.rs::tests`:
  - `proxies_payload_end_to_end` — spawn an in-process `tokio` echo server on an ephemeral port; build a single-endpoint cluster + `TcpProxy`; `handle` a connected `TcpStream`; assert `payload.len()` bytes round-trip byte-exact.
  - `proxies_closes_downstream_on_upstream_close` — upstream half-closes; assert downstream sees the already-echoed bytes + EOF.
  - `proxies_closes_upstream_on_downstream_close` — downstream drops; assert upstream gets FIN and closes cleanly.
  - `proxies_returns_err_on_upstream_connect_refused` — cluster points at `127.0.0.1:1` (unused); assert `Err(TcpProxyError::UpstreamConnect)`.

### D5 — Binary crate wiring — `envoy-bin`

`crates/envoy-bin/src/main.rs::run` is updated to:
1. Parse bootstrap.
2. Construct `envoy_cluster::from_bootstrap(&bootstrap)?`; wrap in `Arc<ClusterManager>`.
3. For each listener (0 or 1), inspect the single filter:
   - `envoy.filters.network.echo` → `echo::serve` path, unchanged.
   - `envoy.filters.network.tcp_proxy` → `let cluster = cluster_mgr.get(&tcp_proxy_cfg.cluster).expect("validator ensured present");` (parser already rejected missing clusters). Build `TcpProxy::new(cluster, &tcp_proxy_cfg)`. Build `envoy_listener::Listener::bind(&listener_cfg, Arc::new(tcp_proxy)).await?`. Spawn `listener.serve(shutdown.cancelled())`.
4. Admin listener, if configured, unchanged.

`crates/envoy-bin/Cargo.toml` adds: `envoy-cluster = { path = "../envoy-cluster" }`, `envoy-listener = { path = "../envoy-listener" }`, `envoy-tcp = { path = "../envoy-tcp" }`. No new transitive crates outside D-3.2 foundations.

A new integration test `crates/envoy-bin/tests/tcp_proxy.rs` (backstop to the differential fixture): spawn `envoy-bin` as a subprocess with a tcp-proxy config pointing at an in-process echo server (reserve the backend port in the test, run a tokio echo server in a spawned task), open a TCP connection to the listener, write a payload, read-exact, assert equality. Mirrors the shape of phase 01's `admin_only.rs`.

### D6 — `tcp-echo-server` helper crate

New workspace member `tests/helpers/tcp-echo-server/`. First crate under `tests/helpers/`; the `tests/helpers/` directory itself is already named in the §4 layout of `BOOTSTRAP_PROMPT.md` so no ADR is required for its creation.

- `tests/helpers/tcp-echo-server/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Deps from the D-3.2 list only: `tokio` (features `rt-multi-thread`, `net`, `io-util`, `macros`, `signal`), `anyhow`, `thiserror`, `tracing`, `tracing-subscriber`.
  (`anyhow` is permitted in binary crates per D-3.2. `tcp-echo-server` is a binary crate, so `anyhow` is in scope here.)

- `tests/helpers/tcp-echo-server/src/main.rs` starts with `#![forbid(unsafe_code)]`. Contract:
  - Hand-parsed argv mirroring `crates/envoy-bin/src/argv.rs`: `--port <u16>` required, `--help`, `--version`. `ArgvError` typed via `thiserror`.
  - Runtime: `tokio::net::TcpListener::bind(("127.0.0.1", port))`, accept loop with `tokio::select!` between `accept()` and `tokio::signal::ctrl_c()`, each accepted stream spawned onto a `JoinSet` running `let (mut r, mut w) = stream.split(); tokio::io::copy(&mut r, &mut w).await`. On shutdown: stop accepting, drain with `DRAIN_BUDGET = Duration::from_secs(5)`, abort stragglers, return 0.
  - Logs on `stderr` via `tracing_subscriber::fmt`, similar to envoy-bin.
  - Exit codes: `0` clean, `1` runtime error, `2` argv error. Mirrors `envoy-bin`.

- Unit tests in `tests/helpers/tcp-echo-server/src/main.rs::tests`:
  - `argv_parses_port` — `--port 10042` → `Ok(Args { port: 10042 })`.
  - `argv_rejects_missing_port_flag` — empty argv → `Err(ArgvError::MissingFlag("--port"))`.
  - `argv_rejects_missing_value` — `--port` alone → `Err(ArgvError::MissingValue)`.
  - `argv_rejects_non_numeric_port` — `--port abc` → `Err(ArgvError::InvalidPort)`.
  - `argv_rejects_trailing_argument` — `--port 10042 --junk` → `Err(ArgvError::Trailing)`.
  - `argv_shows_help` — `--help` → `Err(ArgvError::HelpRequested)` (exit 0 path via main's translation).
  - `echoes_round_trip` — `#[tokio::test(flavor="multi_thread")]`: reserve a port, spawn the server in a task on that port, connect, write 32-byte payload, `read_exact` 32 bytes, assert equal.
  - `drain_exits_within_budget` — spawn the server, open a stalled connection (peer stops reading), fire shutdown, assert server returns within `DRAIN_BUDGET + ε`.

### D7 — Differential harness extensions

`tests/differential/src/lib.rs` and `tests/differential/src/subject.rs` extend to support host-local backends.

- **`TcpProxyBackend` helper** in `tests/differential/src/backend.rs` (new module):

    ```rust
    pub struct TcpProxyBackend {
        port: u16,
        child: tokio::process::Child,
    }

    impl TcpProxyBackend {
        pub async fn spawn() -> anyhow::Result<Self>;     // reserves port, spawns helper binary, waits until ready
        pub fn port(&self) -> u16;
        pub fn container_host(&self) -> &'static str;     // "host.docker.internal"
    }

    impl Drop for TcpProxyBackend { /* Child::start_kill() (SIGKILL on Unix via tokio::process — same posture as tests/differential/src/subject.rs, pending the phase-00 I3 rollover that takes the `nix` crate) + try_wait loop with 2s timeout */ }
    ```

  `spawn` locates the `tcp-echo-server` binary at test time. Cargo's `CARGO_BIN_EXE_<name>` env var is only populated for tests in the *same* package that owns the binary, so the differential crate cannot use it for a cross-package helper. The harness computes the path as `env!("CARGO_MANIFEST_DIR") + "/../../target/<profile>/tcp-echo-server"` with `profile = if cfg!(debug_assertions) { "debug" } else { "release" }`. `cargo test --workspace` builds all workspace binaries before running tests, so the binary is guaranteed present. Readiness is polled by attempting `tokio::net::TcpStream::connect(("127.0.0.1", port))` in a short backoff loop (≤1s total), matching the phase-00 `wait_accept_ready` pattern.

- **`render_yaml` per-driver key expansion.** The existing `render_yaml(template, kvs)` helper is generic over `kvs`. The per-driver key map grows:
  - `Driver::TcpEcho` with a template that does *not* contain `{{BACKEND_PORT}}` (fixture 0001) → substitute `{{PORT}}` only.
  - `Driver::TcpEcho` with a template that *does* contain `{{BACKEND_PORT}}` (fixture 0003) → substitute `{{PORT}}`, `{{BACKEND_PORT}}`, `{{BACKEND_HOST}}`. `{{BACKEND_HOST}}` is `"host.docker.internal"` for `envoy.yaml` (container side, per ADR-0014) and `"127.0.0.1"` for `envoy-rust.yaml` (host subprocess side).
  - `Driver::HttpGet { .. }` → substitute `{{ADMIN_PORT}}` (unchanged from phase 01).

  Detection is mechanical (string-contains on the template body before substitution). No new `Driver` variant is introduced — the harness's round-trip pattern (write payload, read-exact, compare) is identical whether bytes go through Envoy's echo filter or through tcp_proxy → echo backend.

- **`run_fixture` dispatch.** Before starting the two proxies, check whether either template contains `{{BACKEND_PORT}}`. If yes, spawn a `TcpProxyBackend`, fill that port into the substitution map, proceed; tear down the backend via `Drop` when `run_fixture` returns (either side).

- **Upstream container `host.docker.internal` setup.** `tests/differential/src/upstream.rs` adds `with_host("host.docker.internal", Host::HostGateway)` to the upstream-Envoy testcontainers image when the fixture uses a backend (= template contains `{{BACKEND_HOST}}`). Pattern stolen from the `testcontainers` `with_host` surface; no direct cast-cost beyond the existing testcontainers API already vetted by ADR-0005.

- **New harness unit tests** in `tests/differential/src/lib.rs::tests` (+ `tests/differential/src/backend.rs::tests`):
  - `tcp_proxy_backend_spawns_and_echoes` — spawn backend, connect, round-trip a payload, assert equal. Proves the helper in isolation before any fixture exercises it.
  - `tcp_proxy_backend_drop_terminates_child` — spawn, drop, assert child process exited.
  - `fixture_0003_expectations_parses_as_tcp_echo` — structural load, mirroring the 0001 migration regression.
  - `render_yaml_substitutes_backend_keys_for_envoy_side` — unit test on the substitution map.
  - `render_yaml_substitutes_backend_keys_for_envoy_rust_side`.

- **Integration test** `tests/differential/tests/tcp_proxy.rs` — Docker-gated, same `#[ignore]`-unless-`DOCKER=1` pattern as `admin_ready.rs`. Calls `run_fixture("0003-tcp-proxy")`.

### D8 — Fixture `tests/fixtures/0003-tcp-proxy/`

Files:

- `envoy.yaml`:

    ```yaml
    node:
      id: envoy-rust-phase-02-subject
      cluster: envoy-rust-phase-02

    admin:
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 0

    static_resources:
      listeners:
        - name: tcp_listener
          address:
            socket_address:
              address: 0.0.0.0
              port_value: {{PORT}}
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
                          address: {{BACKEND_HOST}}
                          port_value: {{BACKEND_PORT}}
    ```

  (Upstream Envoy requires admin.port_value; `0` asks the kernel for an ephemeral port. If v1.33.0 rejects `0` here, the fixture supplies a templated `{{ENVOY_ADMIN_PORT}}` and the harness reserves an extra host port. Plan execution resolves this against the real container, not at planning time.)

- `envoy-rust.yaml`:

    ```yaml
    node:
      id: envoy-rust-phase-02-subject
      cluster: envoy-rust-phase-02

    static_resources:
      listeners:
        - name: tcp_listener
          address:
            socket_address:
              address: 127.0.0.1
              port_value: {{PORT}}
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
                          port_value: {{BACKEND_PORT}}
    ```

  Divergences from `envoy.yaml`:
  - Listener bind `0.0.0.0` vs. `127.0.0.1` (same reason as 0002: host subprocess vs. container).
  - Endpoint host `{{BACKEND_HOST}}` (templates to `host.docker.internal` on the container side) vs. `127.0.0.1` on the host side.
  - No admin endpoint — envoy-rust's admin is phase-01-scope; fixture 0003 doesn't exercise it.

  Neither divergence requires a per-fixture ADR. Both are harness mechanics covered by ADR-0014 (cross-container host reachability).

- `inputs/payload.bin` — copy of phase-00's payload bytes (a deterministic non-empty blob). Exact contents do not matter beyond ≥1 byte and not being all-zero (which could mask copy bugs).

- `expectations.yaml`:

    ```yaml
    driver:
      kind: tcp_echo
    equivalence:
      response_body: byte_exact
    ```

- `README.md` — one paragraph:
    > This fixture drives an arbitrary byte payload through a listener configured with `envoy.filters.network.tcp_proxy` → static cluster `backend` (one endpoint) → a host-local `tcp-echo-server` helper process. Both upstream Envoy and envoy-rust dial the same backend. The `driver.kind: tcp_echo` value refers to the harness's round-trip pattern (write payload, read-exact, compare), not to Envoy's echo *filter* — reusing the same `TcpEcho` driver across fixtures 0001 and 0003 proves that the harness is data-plane-agnostic. Cross-container host reachability is covered by ADR-0014; the `{{BACKEND_HOST}}` divergence between `envoy.yaml` and `envoy-rust.yaml` is its only non-harness divergence. Half-close posture is Envoy's v1.33.0 default (`enable_half_close: false`), covered by ADR-0015.

### D9 — Phase-01 rollovers folded in

Per Q5 (brainstorm decision): all three phase-01 REVIEW §9 starter items land in phase 02.

- **I3 — `decode_chunked` unit tests.** Four tests appended to `tests/differential/src/lib.rs::tests`:
  - `decode_chunked_empty_stream` — input `b"0\r\n\r\n"` → `Ok(vec![])`.
  - `decode_chunked_with_chunk_extension` — `b"5;name=value\r\nhello\r\n0\r\n\r\n"` → `Ok(b"hello".to_vec())`.
  - `decode_chunked_truncated_size_line` — missing `\r\n` after the chunk size or hex-byte truncation → `Err`, not silent `Ok(partial)`.
  - `decode_chunked_ignores_trailer_bytes` — `b"3\r\nabc\r\n0\r\nTrailer-Name: value\r\n\r\n"` → `Ok(b"abc".to_vec())`.

- **I4 — admin 8 KiB header cap tightening.** In `crates/envoy-bin/src/admin.rs:158–170`:
  - Replace `let n = stream.read(&mut scratch).await?;` with `let remaining = MAX_REQUEST_HEAD - buf.len(); let n = stream.read(&mut scratch[..remaining]).await?;` so the buffer cannot exceed `MAX_REQUEST_HEAD` by even one byte.
  - Update `rejects_oversized_request_headers` (admin.rs:~302) to write exactly `MAX_REQUEST_HEAD + 1` bytes and assert the 431.
  - Add `accepts_requests_exactly_at_cap` — `MAX_REQUEST_HEAD` bytes with a valid terminating CRLF-CRLF (e.g. a maximally-padded header block that still parses) → assert the 404 (unknown path) fires, not 431.

- **M1 — stale `TODO(phase-01)` retarget.** In `tests/differential/src/subject.rs:25–32`, update the comment body from "deferred to phase 01 under its own ADR" to: the phase-00 I3 SIGKILL→SIGTERM switch awaits a future phase that takes the `nix` crate (or equivalent POSIX-signal surface) under its own ADR. Name no specific target phase — phase 01 and phase 02 both chose not to take `nix`, so the deferral is open-ended. Doc-only change; no functional behavior shift.

### D10 — Fuzz corpus extension

`crates/envoy-config/fuzz/corpus/parse_bootstrap/` gains two seeds to keep the fuzzer's structural coverage current with the phase-02 grammar:

- `tcp_proxy_single_endpoint.yaml` — the fixture 0003 `envoy-rust.yaml` with `{{PORT}}` → `10000`, `{{BACKEND_PORT}}` → `10001`.
- `tcp_proxy_round_robin_triple.yaml` — a three-endpoint cluster with the same filter shape.

No new fuzz target ships this phase. The existing `parse_bootstrap` target covers the extended grammar because `TypedConfig`, `Cluster`, and nested endpoint shapes are all reachable via `envoy_config::parse_bootstrap`.

### D11 — CI workflow

`.github/workflows/ci.yml` changes: none required. The existing `build` job runs `cargo test --workspace`, which picks up the new crates automatically. The existing `fuzz` job exercises the extended corpus via the same `cargo fuzz run parse_bootstrap -- -max_total_time=30` invocation.

The new `tcp-echo-server` binary is a workspace member, so `cargo build --workspace --all-targets` compiles it and exposes `CARGO_BIN_EXE_tcp-echo-server` to the differential harness.

The Docker-gated integration test `tests/differential/tests/tcp_proxy.rs` runs under the same gating pattern as `admin_ready.rs`.

### D12 — ADRs to land during execution

Three ADRs, appended to `docs/envoy-rust/DECISIONS.md` in order, per §7 of this SPEC (ADR-0013, ADR-0014, ADR-0015). Additional ADRs may be required during execution per D-3.5 if:

- Upstream Envoy v1.33.0's admin schema rejects `port_value: 0` on the fixture (land an ADR documenting the workaround: extra templated `{{ENVOY_ADMIN_PORT}}`).
- `cargo deny check` flips red on any new transitive surface from the helper crate (`tracing-subscriber`'s dep graph is already in scope via envoy-bin, so no new exposure is expected; verify during execution).
- Platform detection forces a second backend-host address (if `ubuntu-latest` CI's Docker install turns out to refuse `host-gateway`, fall back to `172.17.0.1` with an ADR; verify during execution).

---

## 4. Non-goals (deferred to later phases)

- TLS on listener (downstream) or cluster (upstream) — phase 03.
- `envoy.filters.network.echo`'s deprecation — stays supported for fixture 0001.
- `idle_timeout`, `max_connect_duration`, `tunneling_config`, `access_log` on tcp_proxy — phase 06 and later.
- `enable_half_close` configurability on tcp_proxy — deferred to a phase with a fixture that depends on FIN propagation (ADR-0015).
- Multiple listeners per process — `listeners.len() ∈ {0, 1}` cap stays.
- Cluster health checking, outlier detection, circuit breakers — §9 upstream-robustness family.
- `type: LOGICAL_DNS`, `type: STRICT_DNS`, `type: EDS` — phase 02 accepts only `STATIC`.
- `lb_policy: LEAST_REQUEST`, `RANDOM`, `RING_HASH`, `MAGLEV`, subset LB, locality-weighted LB, priority LB, panic thresholds — §9 load-balancing family.
- Listener filters (`listener_filters`), filter chain matchers (`filter_chain_matcher`), transport_socket, `per_connection_buffer_limit_bytes`, `connection_balance_config` — out of phase-02 surface.
- Filter chain framework / extension registry (trait registry, per-route config, iteration protocol) — phase 07.
- Stats subsystem, access logs, Prometheus — phase 06.
- Admin endpoints beyond phase 01's `/ready` (`/stats`, `/clusters`, `/config_dump`, `/server_info`, `/drain_listeners`, `/healthcheck/fail`) — phase 08.
- Distribution-equivalence assertion on round-robin — brainstorm Q1 decision: unit-test-only.
- `envoy-protos` crate + `prost` / `prost-build` + proto-tree vendoring — xDS family (§9) per ADR-0013.
- `envoy-filter/` crate — phase 07.
- Long-budget nightly fuzz CRON — a future, scheduled phase.

---

## 5. Splitting guidance for the planner

Estimated scope:

| Surface | Net LoC (impl + tests) |
|---|---|
| envoy-cluster crate | ~150 + ~200 |
| envoy-listener crate | ~200 + ~150 |
| envoy-tcp crate | ~120 + ~150 |
| envoy-bin wiring + integration test | ~80 + ~80 |
| tcp-echo-server helper | ~80 + ~80 |
| envoy-config schema (filter envelope + cluster topology) | ~120 + ~250 |
| harness extensions (`TcpProxyBackend`, render_yaml, upstream host-gateway, dispatch) | ~150 + ~100 |
| fixture 0003 + fuzz seeds + CI config | ~60 |
| Phase-01 rollovers I3/I4/M1 | ~40 + ~50 |
| ADRs 0013/0014/0015 | ~0 (docs) |
| **Total** | **~2060 LoC; ~22 tasks** |

The §6 gates are "> ~25 tasks OR > ~1500 LoC estimated." At ~22 tasks the task gate holds, but the ~2060 LoC estimate is ~37% above the LoC gate. The planner should read this estimate as a **soft signal that a split is likely**, then confirm by line-counting the actual plan text before deciding. If the plan lands under the gate (e.g. because tests compress in shared helpers), continue as one phase. If not, split at this boundary:

- **02.1 — Config schema + cluster manager + echo-server helper.** envoy-config extensions (all new tests land here, including D3's 16 tests and D9's I3 four chunked-decoder tests). envoy-cluster crate complete (static-cluster types, round-robin LB + unit tests). tcp-echo-server helper crate. No envoy-listener, no envoy-tcp, no fixture 0003. Acceptance: stable CI green, fuzz green on extended corpus, fixtures 0001 + 0002 unchanged. I3 lands here (harness-only); I4 and M1 defer to 02.2.
- **02.2 — Listener + TCP proxy + fixture 0003 + remaining rollovers.** envoy-listener + envoy-tcp crates. envoy-bin wiring. harness extensions (`TcpProxyBackend`, render_yaml expansion, upstream `with_host`). Fixture 0003 end-to-end green (differential gate). I4 + M1. ADRs 0014 + 0015. Acceptance: all three fixtures green; full phase-done gate.

Same anti-preemption posture as phase-01 SPEC §5: **do not pre-emptively split.** The thresholds exist to catch overscoping, not to enforce a shape. The plan-writer lands the thresholds first, then consults this section only if they're crossed.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the planner resolves them in-plan rather than mid-execution.

1. **Three new crates land before `envoy-bin` wires them in.** Task ordering puts `envoy-config` extensions first (they block cluster construction), then `envoy-cluster`, then `envoy-listener`, then `envoy-tcp` (depends on both listener trait + cluster handle), then the `tcp-echo-server` helper, then binary-crate wiring. The harness extension and fixture come last because they exercise the full integrated surface.

2. **`envoy-listener`'s handler trait boxes the future.** Rust async-trait-without-crate idiom: declare `trait ConnectionHandler` with `fn handle(&self, ...) -> BoxFuture<'static, Result<...>>` instead of `async fn handle(...)`. This keeps the trait object-safe (required because `Listener::serve` stores `Arc<dyn ConnectionHandler>`) without pulling `async-trait` — which isn't on the D-3.2 permitted-foundations list. If a future phase (07) builds a richer extension registry, it may land `async-trait` under a dedicated ADR; phase 02 sidesteps with hand-boxed futures.

3. **`ConnectionHandler`'s error type is a trait object, not `anyhow::Error`.** Library crates cannot depend on `anyhow` per D-3.2. The trait returns `Result<(), Box<dyn std::error::Error + Send + Sync>>`, which `envoy-tcp::TcpProxy` produces by `.into()`-converting its `TcpProxyError` (it implements `std::error::Error` via `thiserror`). The binary crate (`envoy-bin`) is the only place `anyhow` is imported.

4. **Upstream-container host reachability via `host-gateway` requires a recent-enough Docker daemon.** On `ubuntu-latest` GitHub Actions runners, Docker CE ≥ 20.10 is standard (the feature landed there). Local-dev Docker Desktop (macOS/Windows/Linux) always supports it. If a developer is on an ancient Docker (<20.10), `with_host("host.docker.internal", Host::HostGateway)` either errors at container start or resolves to nothing. The fallback — `172.17.0.1` (default bridge gateway) — is an ADR-able Plan B; verify at execution time, land the fallback ADR only if it trips.

5. **`from_bootstrap`'s empty-cluster check duplicates a validator rule.** `envoy-config::bootstrap::validate` already rejects `load_assignment.endpoints` with zero total `LbEndpoint`s. `envoy-cluster::from_bootstrap` also rejects `EmptyCluster`. This is defense-in-depth: the cluster crate is a library with its own invariants, and its `pick_endpoint` relies on `!endpoints.is_empty()`. Review should flag any later phase that removes one of the two checks without removing the invariant.

6. **Round-robin's `AtomicUsize` cursor wraps around.** `cursor.fetch_add(1, Ordering::Relaxed) % endpoints.len()` is correct even when the counter wraps (Rust `AtomicUsize::fetch_add` wraps on overflow, and the modulo is stable under wraparound because `endpoints.len()` is bounded). `Relaxed` ordering is sufficient because there's no cross-observation of the cursor between threads — each call reads-modifies-writes atomically, and "which endpoint" doesn't need any happens-before relationship with other operations.

7. **`tokio::io::copy` propagates EOF but not every error symmetry.** If downstream → upstream succeeds while upstream → downstream errors, `try_join!` returns the upstream-side error and drops the downstream-side future. `Drop` on the write halves closes their sockets, which RSTs the open direction. That's acceptable — it aligns with Envoy's behavior of closing both sides on an asymmetric error. Review should flag any future phase that promotes `tokio::io::copy` to a custom loop without preserving this property.

8. **`enable_half_close: false` is Envoy's v1.33.0 default.** Not writing the key in the fixture YAML is equivalent to writing `false`. Don't "defensively" include it — the fixture should be minimal. See ADR-0015.

9. **`{{BACKEND_PORT}}` and `{{PORT}}` must be distinct.** The harness reserves two separate ephemeral ports for fixture 0003; the templates MUST NOT reuse `{{PORT}}` for the backend port. Review should flag any future fixture that collapses them.

10. **Error enum constructors use `#[from]` where possible.** `thiserror`'s `#[from]` on `ConfigError::Yaml` and `ClusterError::EndpointParse` keeps call-site code concise. Named-field variants (`EndpointParse { cluster, addr, source }`) don't work with `#[from]`; use `.map_err(|source| …)` at those sites.

11. **`tcp-echo-server`'s argv parser reuses `envoy-bin`'s idiom, not code.** The `argv.rs` pattern (hand-parsed `parse_argv(&[String]) -> Result<Args, ArgvError>`) is copied structurally but not literally. Cross-crate argv sharing would mean extracting a third crate; not worth it for two ~100-LoC parsers that don't share argument shapes.

12. **`testcontainers`'s `with_host` API signature.** At the `testcontainers = "0.23"` version pinned by ADR-0005, `with_host` takes `(name: &str, value: Host)` where `Host::HostGateway` is a named enum variant. The planner verifies this against the actual crate version in `Cargo.lock` during execution — if the API has shifted, adjust accordingly; not an ADR surface because we're not extending the exemption. (The `testcontainers` exemption in `deny.toml` already covers its transitive graph.)

13. **Fixture 0003's `admin` block on envoy-rust side.** Intentionally absent. envoy-rust's admin endpoint lives in the phase-01 fixture 0002; fixture 0003 does not need it. Including `admin` would grow the differential surface into headers that ADR-0011 defers to phase 04.

14. **Payload choice for `inputs/payload.bin`.** Reuse fixture 0001's payload exactly (`hello, envoy-rust\n` or whatever bytes 0001 ships) to minimize cognitive load. The exact bytes don't matter — 1:1 echo semantics are what's tested.

---

## 7. ADRs expected from this phase

Three ADRs land during execution, in `docs/envoy-rust/DECISIONS.md`, in order.

### ADR-0013 — YAML-native `typed_config` deserialization until the xDS/protos family lands

- Context: Phase 02 is the first phase to surface Envoy's `typed_config` envelope (`envoy.filters.network.tcp_proxy`). The `envoy-protos` crate + `prost` / `prost-build` + upstream proto-tree vendoring were deferred at phase-00 bootstrap to the xDS family (ROADMAP §9). Phase 02 must choose: bring the protos stack forward now, or ship a narrower shim.
- Options considered:
  - **(i) YAML-native — one Rust enum discriminated on the `@type` URL string literal, fields deserialized by serde.** Minimal surface, scoped to this phase's needs. Grows one enum variant per filter across phases 04/05/06 until the xDS family ships.
  - **(ii) Bring `prost` + `envoy-protos` in as part of phase 02.** Pulls forward multi-phase proto-tree vendoring. Out of ROADMAP row-02 scope; would trigger a split by itself.
  - **(iii) Non-Envoy `raw_config` YAML key.** Diverges `envoy.yaml` and `envoy-rust.yaml` on filter shape, breaking the fixture principle that configs are initially identical.
- Decision: (i). `TypedConfig` enum in `envoy-config::bootstrap` with a `#[serde(tag = "@type")]` discriminator; one variant for TCP proxy in phase 02; extended per filter across future phases.
- Rationale: keeps phase 02 within row-02 scope; defers the `envoy-protos` multi-phase work until it pays for itself. Reviewable by shape — a stranger reading the YAML can see which filters are supported.
- Consequences: unknown `@type` URLs reject at parse time via serde's tagged-enum default behavior. Every new filter in phase 04 / 05 / 06 extends the enum by one variant. An `envoy-protos` supersession ADR in the xDS family re-routes the `@type` URL to prost-generated message types in one sweep and retires this shim.

### ADR-0014 — Cross-container host reachability via `host.docker.internal` + `host-gateway`

- Context: Fixture 0003's upstream backend is an in-tree host-running `tcp-echo-server` process. The upstream Envoy container (via testcontainers) and the envoy-rust host subprocess must both reach this backend. Container → host networking is platform-dependent: `host.docker.internal` resolves natively on Docker Desktop; Linux bridge networks require `--add-host=host.docker.internal:host-gateway`.
- Options considered:
  - **(i) Always-on `host.docker.internal` with `host-gateway` injected via testcontainers' `with_host`.** Standardizes on one name across dev/CI. `testcontainers` supports this natively.
  - **(ii) Runtime platform detection (`/.dockerenv`, `uname -r`, `docker info`) with `172.17.0.1` as a Linux-bridge fallback.** Two code paths; brittle against Docker config drift.
  - **(iii) Run the backend inside a Docker container on a shared network.** Loses the "backend is a host process" property; pulls container-network-management into every fixture's setup.
- Decision: (i). `with_host("host.docker.internal", Host::HostGateway)` on the upstream-Envoy container; `envoy.yaml` references `host.docker.internal:{{BACKEND_PORT}}`; `envoy-rust.yaml` references `127.0.0.1:{{BACKEND_PORT}}`.
- Rationale: one code path across macOS dev, Linux dev, and `ubuntu-latest` CI. `testcontainers` already handles the Docker-side plumbing.
- Consequences: every future fixture with a host-local backend follows the same pattern. If a later phase needs a backend inside a Docker network (e.g., multi-proxy topologies), that phase lands a separate testcontainers-networking ADR. If `ubuntu-latest`'s Docker turns out to refuse `host-gateway` (very unlikely; the feature has been GA since Docker CE 20.10), the fallback is `172.17.0.1` under a follow-up ADR — not a silent code change.

### ADR-0015 — Phase 02 TCP proxy runs with Envoy's default `enable_half_close: false`

- Context: ADR-0006/0007 documented the upstream-Envoy half-close-drops-pending-writes subtlety for the echo filter and the subsequent `drive_tcp` trailing-byte poll. `envoy.filters.network.tcp_proxy` has a YAML-visible `enable_half_close: true` toggle (unlike the echo filter, which has none). Fixture 0003's client pattern — `drive_tcp`: write → `read_exact(N)` → drop — does not depend on FIN propagation.
- Options considered:
  - **(i) Leave the default `false` on both `envoy.yaml` and the envoy-rust config.** Matches Envoy's v1.33.0 default; minimal fixture shape.
  - **(ii) Set `true` on both sides.** Pre-positions for FIN-sensitive use cases at the cost of a toggle that doesn't yet matter.
  - **(iii) Set `true` on one side only.** Divergent behavior under identical inputs; violates the "configs are initially identical" fixture principle.
- Decision: (i). `enable_half_close` is absent from both `envoy.yaml` and `envoy-rust.yaml` in fixture 0003. envoy-rust's `TcpProxy::serve_connection` is implemented to match: `tokio::io::copy` on both directions, EOF on either side propagates via drop.
- Rationale: matches Envoy v1.33.0's default tcp_proxy posture; `drive_tcp`'s client pattern doesn't need half-close propagation for a 1:1 echo round-trip; minimizing the fixture's YAML keeps reviewer diffing tight. The ADR-0006/0007 precedent — "narrow fix, leave the grammar for when it pays for itself" — applies to the YAML toggle too.
- Consequences: phase 02's TCP proxy is explicitly *not* a drop-in for every Envoy tcp_proxy deployment; use cases depending on half-close propagation are phase-later work. A future fixture with an asymmetric-close requirement (one side writes, then expects the other side's FIN to trigger a response) lands its own ADR flipping the toggle and extending `TcpProxy` with an explicit half-close propagation mode. Until then, `enable_half_close` is a known non-surface.

---

## 8. Artifacts this phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/02-tcp-proxy/PLAN.md`
- `docs/envoy-rust/phases/02-tcp-proxy/PROGRESS.md`
- `docs/envoy-rust/phases/02-tcp-proxy/REVIEW.md`
- `crates/envoy-cluster/Cargo.toml`
- `crates/envoy-cluster/src/lib.rs`
- `crates/envoy-cluster/src/cluster.rs`
- `crates/envoy-listener/Cargo.toml`
- `crates/envoy-listener/src/lib.rs`
- `crates/envoy-tcp/Cargo.toml`
- `crates/envoy-tcp/src/lib.rs`
- `tests/helpers/tcp-echo-server/Cargo.toml`
- `tests/helpers/tcp-echo-server/src/main.rs`
- `crates/envoy-bin/tests/tcp_proxy.rs`
- `tests/differential/src/backend.rs`
- `tests/differential/tests/tcp_proxy.rs`
- `tests/fixtures/0003-tcp-proxy/envoy.yaml`
- `tests/fixtures/0003-tcp-proxy/envoy-rust.yaml`
- `tests/fixtures/0003-tcp-proxy/inputs/payload.bin`
- `tests/fixtures/0003-tcp-proxy/expectations.yaml`
- `tests/fixtures/0003-tcp-proxy/README.md`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml`

Amended during execution:

- Root `Cargo.toml` — add `crates/envoy-cluster`, `crates/envoy-listener`, `crates/envoy-tcp`, `tests/helpers/tcp-echo-server` to `[workspace] members`.
- `crates/envoy-bin/Cargo.toml` — add `envoy-cluster`, `envoy-listener`, `envoy-tcp` path deps.
- `crates/envoy-bin/src/main.rs` — construct `ClusterManager`; dispatch listener setup between `echo::serve` (echo filter) and `Listener::serve`+`TcpProxy` (tcp_proxy filter).
- `crates/envoy-bin/src/admin.rs` — apply I4 tightening (read-slice bounded by `MAX_REQUEST_HEAD - buf.len()`).
- `crates/envoy-config/src/bootstrap.rs` — add `TypedConfig`, `TcpProxyConfig`, fleshed-out `Cluster`/`LoadAssignment`/`LocalityLbEndpoints`/`LbEndpoint`/`Endpoint`, extended validator rules, 16 new unit tests.
- `crates/envoy-config/src/lib.rs` — re-export new public types (`TypedConfig`, `TcpProxyConfig`, `ClusterType`, `LbPolicy`, etc.); extend `ConfigError` with `MissingTypedConfig`, `UnexpectedTypedConfig`, `UnknownCluster`, `LoadAssignmentNameMismatch`, `EmptyClusterEndpoints`.
- `tests/differential/Cargo.toml` — verify at plan time whether `tracing` is already a dev-dep; if not, add it for the backend helper's diagnostics. No new *non-D-3.2* deps.
- `tests/differential/src/lib.rs` — add 4 chunked-decoder unit tests (I3); add `TcpProxyBackend` module re-export; extend `render_yaml` per-driver substitution logic; extend `run_fixture` to spawn the backend when `{{BACKEND_PORT}}` appears in the template.
- `tests/differential/src/subject.rs` — retarget `TODO(phase-01)` comment (M1); doc-only.
- `tests/differential/src/upstream.rs` — extend testcontainers config to add `with_host("host.docker.internal", Host::HostGateway)` when a fixture uses a backend.
- `tests/fixtures/0001-tcp-echo/` — unchanged.
- `tests/fixtures/0002-static-admin-ready/` — unchanged.
- `docs/envoy-rust/DECISIONS.md` — ADR-0013, ADR-0014, ADR-0015 appended.
- `docs/envoy-rust/ROADMAP.md` — row 02 status → `done` in the final commit.
- `docs/envoy-rust/STATE.md` — active → `03-tls-tcp` (slug consistent with §8 of `BOOTSTRAP_PROMPT.md`), next-skill → `superpowers:brainstorming`.
- `deny.toml` — only if `cargo deny check` flags a new transitive license (expected: no — the helper crate's tracing-subscriber dep graph is already in scope via envoy-bin).

---

## 9. Final commit message format (for state 6 of the lifecycle)

```
phase 02: Listener + TCP proxy + static cluster + round-robin LB [ADR-0013, ADR-0014, ADR-0015]

Three new crates land the first real data-plane path: envoy-listener manages
bind/accept/drain; envoy-cluster owns static clusters with round-robin LB;
envoy-tcp implements the TCP proxy filter. A new tests/helpers/tcp-echo-server
helper crate provides the deterministic backend that both upstream Envoy and
envoy-rust dial through fixture 0003. Phase-01 starter items I3/I4/M1 land
alongside.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (byte-exact payload round-trip through
  tcp_proxy → STATIC cluster, one endpoint, to host-local tcp-echo-server).
Conformance: none.
```
