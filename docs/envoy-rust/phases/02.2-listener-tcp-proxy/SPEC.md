# Phase 02.2 — Listener + TCP proxy filter + fixture 0003 + remaining rollovers

- **Phase id:** `02.2`
- **Parent phase:** `02-tcp-proxy` (split per ADR-0013)
- **Title:** Listener + TCP proxy filter + fixture 0003 + remaining rollovers
- **Depends on:** `02.1` (config schema + cluster manager + echo-server helper). Sibling sub-phase 02.1 MUST be `done` (its ROADMAP row flipped to `done` in 02.1's final commit) before 02.2 enters `in-progress`.
- **Differential surface when done:** `tests/fixtures/0003-tcp-proxy/` green against upstream `envoyproxy/envoy:v1.33.0` (byte-exact payload round-trip through a TCP-proxy → in-tree `tcp-echo-server` backend). Pre-existing fixtures `0001-tcp-echo` and `0002-static-admin-ready` remain green.
- **Seeded by:** `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (parent, committed at SHA `50349da`) §§D2, D4, D5, D7, D8, D9 (items I4 and M1 only), D11; split decision at ADR-0013.

This SPEC is the design contract for sub-phase 02.2. The next session — after 02.1 has landed its final commit and STATE.md has advanced to `02.2-listener-tcp-proxy` — converts this into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents and 02.1's final state (via `git log` and the landed `envoy-cluster` / `tcp-echo-server` / `envoy-config` surface) must be able to execute it without consulting the parent `02-tcp-proxy/SPEC.md`.

---

## 1. Goal and acceptance signal

**Goal.** Land the listener (`envoy-listener`) and the TCP proxy filter (`envoy-tcp`) — the two new library crates 02.1 deliberately deferred — wire them into `envoy-bin`, and ship the end-to-end differential fixture `tests/fixtures/0003-tcp-proxy/` that proves a realistic Envoy static bootstrap with a single `envoy.filters.network.tcp_proxy` filter routing to a static `STATIC` cluster with `lb_policy: ROUND_ROBIN` binds a listener, accepts downstream TCP connections, picks an upstream endpoint, dials it plaintext, and bidirectionally copies bytes until either side closes. Extend the differential harness (`TcpProxyBackend`, `render_yaml` expansion, upstream `with_host`) to support host-local backends reachable from both the upstream-Envoy container and the envoy-rust host subprocess. Close phase-01 REVIEW §9 starter items **I4** (admin 8 KiB header cap tightening) and **M1** (stale `TODO(phase-01)` retarget in `tests/differential/src/subject.rs`).

This sub-phase stands up the first real data-plane path in the project: all prior phases (and 02.1) have been scaffolding, config parsing, and admin `/ready`.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 02.2's feature surface (= the full parent-phase-02 acceptance surface):

- (a) the new differential fixture `tests/fixtures/0003-tcp-proxy/` is green;
- (b) the pre-existing differential fixtures `tests/fixtures/0001-tcp-echo/` and `tests/fixtures/0002-static-admin-ready/` remain green;
- (c) no conformance suites run this sub-phase (first one — `h2spec` — attaches in phase 05);
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`-max_total_time=30`) against the corpus extended in 02.1 (no new seeds in 02.2);
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- (f) `REVIEW.md` for this sub-phase is approved.

**Scope shape (inherited from parent-phase brainstorm).** Of the five forks resolved during the parent-phase brainstorm, the four that bind on 02.2 are:

1. **Round-robin LB differential scope — minimum.** The new differential fixture exercises a single upstream endpoint; round-robin correctness over N≥3 endpoints is proved by unit tests at the `envoy-cluster` boundary (landed in 02.1). No differential assertion on distribution, which would run into Envoy's per-worker RR sharding and require a softened-equivalence ADR.
2. **Upstream backend — in-tree Rust helper crate.** The `tests/helpers/tcp-echo-server/` crate (landed in 02.1) is the backend both proxies dial. No third-party image, no second Envoy container.
3. **Crate layout — two remaining new crates.** `envoy-listener` and `envoy-tcp` land here, one per primitive, matching the §4 layout of `BOOTSTRAP_PROMPT.md`.
4. **Phase-01 rollovers — I4 and M1 fold in here** (I3 already landed in 02.1).

The `typed_config`-deserialization fork landed in 02.1 under ADR-0014; 02.2 uses the resulting `TcpProxyConfig` from `envoy-config` as a consumer.

---

## 2. Behavior-contract scope for sub-phase 02.2

Sub-phase 02.2 exercises only **row 2** of the `BEHAVIOR_CONTRACT.md` §7.2 equivalence matrix:

- **Response body — Byte-exact for deterministic handlers.** The `drive_tcp` helper (landed in phase 00 under ADR-0006/0007, extended by 01) writes `payload` bytes downstream, reads exactly `payload.len()` bytes back, asserts byte-equality, and closes. The echo backend (`tcp-echo-server` from 02.1) guarantees the 1:1 contract regardless of whether the bytes pass through Envoy's echo *filter* (fixture 0001) or through `tcp_proxy → tcp-echo-server` (fixture 0003).

No other dimension is engaged. No response status (TCP, no HTTP). No access logs (phase 06). No stats (phase 06). No headers (phase 04 for HTTP; TCP has none). No xDS (§9 family).

**No `BEHAVIOR_CONTRACT.md` edits in 02.2.** The currently-empty subsections (`Header allow-list`, `Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances`) remain empty.

---

## 3. Deliverables

### D1 — New library crate `crates/envoy-listener/`

Added to root `[workspace] members`. Owns listener binding, accept loops, and graceful drain.

- `crates/envoy-listener/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Deps: `envoy-config = { path = "../envoy-config" }`, `tokio` (with features `rt`, `net`, `macros`, `time`, `sync`), `thiserror`, `tracing`. Dev-deps: `tokio` additionally gains `rt-multi-thread`, `io-util` for tests.

- `crates/envoy-listener/src/lib.rs` starts with `#![forbid(unsafe_code)]`. Public surface:

    ```rust
    pub struct Listener { /* TcpListener + Arc<dyn ConnectionHandler> */ }

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

    pub type BoxFuture<'a, T> =
        std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

    pub trait ConnectionHandler: Send + Sync + 'static {
        fn handle(
            &self,
            downstream: tokio::net::TcpStream,
        ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>;
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

  `BoxFuture` is defined in-crate to avoid pulling `futures` (not on the D-3.2 permitted-foundations list). The `ConnectionHandler::handle` return type uses `Box<dyn std::error::Error + Send + Sync>` rather than `anyhow::Error` because D-3.2 bars library crates from depending on `anyhow`. The binary crate (`envoy-bin`) can still convert to `anyhow::Error` at its boundary.

- `Listener::bind` resolves `cfg.address.socket_address` to a `SocketAddr` (reusing phase-01's `resolve_socket` helper pattern), calls `tokio::net::TcpListener::bind`, stores the bound listener + `Arc<dyn ConnectionHandler>`. Returns `ListenerError::Bind` with the pre-resolved `addr` so errors point at the intended bind target.

- `Listener::serve` uses the documented `tokio::select!` + `JoinSet<Result<()>>` shape (mirror of `admin::serve` / `echo::serve` from phases 00–01):

    ```rust
    const DRAIN_BUDGET: Duration = Duration::from_secs(5);
    let mut join_set: JoinSet<Result<(), Box<dyn std::error::Error + Send + Sync>>> = JoinSet::new();
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

- Unit tests in `crates/envoy-listener/src/lib.rs::tests` (six tests):
  - `bind_returns_socket_address` — ephemeral port, assert `local_addr()` matches the bound port.
  - `serves_accepts_and_dispatches_to_handler` — trivial `EchoHandler` for test; open a TCP connection, send bytes, receive them back.
  - `serves_honors_shutdown_signal` — spawn `serve`, send shutdown, assert return within `DRAIN_BUDGET + ε`.
  - `serves_drains_in_flight_connection_within_budget` — hold one in-flight connection alive, fire shutdown, assert return within `DRAIN_BUDGET + ε`.
  - `serves_aborts_stragglers_past_drain_budget` — the in-flight connection intentionally stalls past 5 s; assert `Err(ListenerError::DrainTimeout)`.
  - `bind_fails_cleanly_on_address_in_use` — bind twice to the same port; second call returns `ListenerError::Bind`.

### D2 — New library crate `crates/envoy-tcp/`

Added to root `[workspace] members`. Owns the TCP proxy filter logic.

- `crates/envoy-tcp/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Deps: `envoy-cluster = { path = "../envoy-cluster" }`, `envoy-config = { path = "../envoy-config" }`, `envoy-listener = { path = "../envoy-listener" }`, `tokio` (features `rt`, `net`, `io-util`, `macros`), `thiserror`, `tracing`. Dev-deps: `tokio` adds `rt-multi-thread`.

- `crates/envoy-tcp/src/lib.rs` starts with `#![forbid(unsafe_code)]`. Public surface:

    ```rust
    pub struct TcpProxy {
        cluster: envoy_cluster::ClusterHandle,
        cluster_name: String,
    }

    impl TcpProxy {
        pub fn new(
            cluster: envoy_cluster::ClusterHandle,
            cfg: &envoy_config::TcpProxyConfig,
        ) -> Self {
            Self { cluster, cluster_name: cfg.cluster.clone() }
        }
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
  1. `let addr = self.cluster.pick_endpoint().ok_or_else(|| TcpProxyError::NoHealthyEndpoint { cluster: self.cluster_name.clone() })?;`
  2. `let upstream = tokio::net::TcpStream::connect(addr).await.map_err(|source| TcpProxyError::UpstreamConnect { addr, source })?;`
  3. `let (mut dr, mut dw) = downstream.into_split(); let (mut ur, mut uw) = upstream.into_split();`
  4. `let d2u = tokio::io::copy(&mut dr, &mut uw); let u2d = tokio::io::copy(&mut ur, &mut dw); tokio::try_join!(d2u, u2d).map_err(|source| TcpProxyError::CopyFailed { source })?;`
  5. `tracing::debug!(%addr, "tcp proxy connection complete");`
  6. `Ok(())`.

  Half-close is **not** set (`enable_half_close: false`, Envoy's v1.33.0 default). Per ADR-0016, the `drive_tcp` 1:1 echo pattern does not depend on FIN propagation; a fixture that needs it lands its own ADR flipping the toggle.

- Per-connection errors (from `handle`) return `Err(Box<dyn std::error::Error + Send + Sync>)` after `TcpProxyError::into()` conversion (which `thiserror`'s generated `std::error::Error` impl enables). The listener's accept loop (§D1) logs them at `warn!` and drops the connection; the listener remains up.

- Unit tests in `crates/envoy-tcp/src/lib.rs::tests` (four tests):
  - `proxies_payload_end_to_end` — spawn an in-process `tokio` echo server on an ephemeral port; build a single-endpoint cluster + `TcpProxy`; `handle` a connected `TcpStream`; assert `payload.len()` bytes round-trip byte-exact.
  - `proxies_closes_downstream_on_upstream_close` — upstream half-closes; assert downstream sees the already-echoed bytes + EOF.
  - `proxies_closes_upstream_on_downstream_close` — downstream drops; assert upstream gets FIN and closes cleanly.
  - `proxies_returns_err_on_upstream_connect_refused` — cluster points at `127.0.0.1:1` (kernel-refused port); assert `Err` wraps `TcpProxyError::UpstreamConnect`.

### D3 — Binary crate wiring — `envoy-bin`

`crates/envoy-bin/src/main.rs::run` is updated to:

1. Parse bootstrap (unchanged).
2. Construct `envoy_cluster::from_bootstrap(&bootstrap)?`; wrap in `Arc<ClusterManager>`.
3. For each listener (0 or 1), inspect the single filter:
   - `envoy.filters.network.echo` → `echo::serve` path, unchanged from phase 01.
   - `envoy.filters.network.tcp_proxy` → `let cluster = cluster_mgr.get(&tcp_proxy_cfg.cluster).expect("validator ensured present");` (parser already rejected missing clusters in 02.1's validator — `ConfigError::UnknownCluster`). Build `TcpProxy::new(cluster, &tcp_proxy_cfg)`. Build `envoy_listener::Listener::bind(&listener_cfg, Arc::new(tcp_proxy)).await?`. Spawn `listener.serve(shutdown.cancelled())` onto the main `JoinSet` alongside the admin listener.
4. Admin listener, if configured, unchanged.

`crates/envoy-bin/Cargo.toml` adds: `envoy-cluster = { path = "../envoy-cluster" }`, `envoy-listener = { path = "../envoy-listener" }`, `envoy-tcp = { path = "../envoy-tcp" }`. No new transitive crates outside D-3.2 foundations.

A new integration test `crates/envoy-bin/tests/tcp_proxy.rs` (backstop to the differential fixture): spawn `envoy-bin` as a subprocess with a tcp-proxy config pointing at an in-process echo server (reserve the backend port in the test, run a tokio echo server in a spawned task — do *not* use the `tcp-echo-server` helper binary here; this is a Rust-native test, and spawning an external helper process is only needed for the Docker-side differential harness). Open a TCP connection to the listener, write a payload, `read_exact`, assert equality. Mirrors the shape of phase 01's `admin_only.rs`.

### D4 — Differential harness extensions

`tests/differential/src/lib.rs` and neighboring modules extend to support host-local backends.

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

    impl Drop for TcpProxyBackend { /* Child::start_kill() (SIGKILL on Unix via tokio::process — same posture as tests/differential/src/subject.rs) + try_wait loop with 2s timeout */ }
    ```

  `spawn` locates the `tcp-echo-server` binary at test time. Cargo's `CARGO_BIN_EXE_<name>` env var is only populated for tests in the *same* package that owns the binary, so the differential crate cannot use it for a cross-package helper (02.1 noted this in its signposts). The harness computes the path as `env!("CARGO_MANIFEST_DIR") + "/../../target/<profile>/tcp-echo-server"` with `profile = if cfg!(debug_assertions) { "debug" } else { "release" }`. `cargo test --workspace` builds all workspace binaries before running tests, so the binary is guaranteed present. Readiness is polled by attempting `tokio::net::TcpStream::connect(("127.0.0.1", port))` in a short backoff loop (≤1 s total), matching the phase-00 `wait_accept_ready` pattern.

- **`render_yaml` per-driver key expansion.** The existing `render_yaml(template, kvs)` helper (landed in phase 01 as part of the tagged `Driver` grammar) is generic over `kvs`. The per-driver key map grows:
  - `Driver::TcpEcho` with a template that does *not* contain `{{BACKEND_PORT}}` (fixture 0001) → substitute `{{PORT}}` only.
  - `Driver::TcpEcho` with a template that *does* contain `{{BACKEND_PORT}}` (fixture 0003) → substitute `{{PORT}}`, `{{BACKEND_PORT}}`, `{{BACKEND_HOST}}`. `{{BACKEND_HOST}}` is `"host.docker.internal"` for `envoy.yaml` (container side, per ADR-0015) and `"127.0.0.1"` for `envoy-rust.yaml` (host subprocess side).
  - `Driver::HttpGet { .. }` → substitute `{{ADMIN_PORT}}` (unchanged from phase 01).

  Detection is mechanical (string-contains on the template body before substitution). No new `Driver` variant is introduced — the harness's round-trip pattern (write payload, read-exact, compare) is identical whether bytes go through Envoy's echo filter or through tcp_proxy → echo backend.

- **`run_fixture` dispatch.** Before starting the two proxies, check whether either template contains `{{BACKEND_PORT}}`. If yes, spawn a `TcpProxyBackend`, fill that port into the substitution map, proceed; tear down the backend via `Drop` when `run_fixture` returns (either side).

- **Upstream container `host.docker.internal` setup.** `tests/differential/src/upstream.rs` adds `with_host("host.docker.internal", Host::HostGateway)` to the upstream-Envoy testcontainers image when the fixture uses a backend (= template contains `{{BACKEND_HOST}}`). Pattern comes from the `testcontainers` `with_host` surface; no direct cost beyond the existing testcontainers API already vetted by ADR-0005.

- **New harness unit tests** in `tests/differential/src/lib.rs::tests` and `tests/differential/src/backend.rs::tests` (five tests):
  - `tcp_proxy_backend_spawns_and_echoes` — spawn backend, connect, round-trip a payload, assert equal. Proves the helper in isolation before any fixture exercises it.
  - `tcp_proxy_backend_drop_terminates_child` — spawn, drop, assert child process exited.
  - `fixture_0003_expectations_parses_as_tcp_echo` — structural load of `tests/fixtures/0003-tcp-proxy/expectations.yaml`, mirroring the 0001 migration regression from phase 01.
  - `render_yaml_substitutes_backend_keys_for_envoy_side` — unit test on the substitution map (envoy side → `host.docker.internal`).
  - `render_yaml_substitutes_backend_keys_for_envoy_rust_side` — unit test on the substitution map (envoy-rust side → `127.0.0.1`).

- **Integration test** `tests/differential/tests/tcp_proxy.rs` — Docker-gated, same `#[ignore]`-unless-`DOCKER=1` pattern as `admin_ready.rs` (phase 01). Calls `run_fixture("0003-tcp-proxy")`.

### D5 — Fixture `tests/fixtures/0003-tcp-proxy/`

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

  (Upstream Envoy requires `admin.port_value`; `0` asks the kernel for an ephemeral port. If v1.33.0 rejects `0` here, the fixture supplies a templated `{{ENVOY_ADMIN_PORT}}` and the harness reserves an extra host port. Plan execution resolves this against the real container, not at planning time. If reached, land it under a new ADR during execution per D-3.5.)

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

  Neither divergence requires a per-fixture ADR. Both are harness mechanics covered by ADR-0015 (cross-container host reachability).

- `inputs/payload.bin` — copy of phase-00's payload bytes (a deterministic non-empty blob). Exact contents do not matter beyond ≥1 byte and not being all-zero (which could mask copy bugs).

- `expectations.yaml`:

    ```yaml
    driver:
      kind: tcp_echo
    equivalence:
      response_body: byte_exact
    ```

- `README.md` — one paragraph:
    > This fixture drives an arbitrary byte payload through a listener configured with `envoy.filters.network.tcp_proxy` → static cluster `backend` (one endpoint) → a host-local `tcp-echo-server` helper process. Both upstream Envoy and envoy-rust dial the same backend. The `driver.kind: tcp_echo` value refers to the harness's round-trip pattern (write payload, read-exact, compare), not to Envoy's echo *filter* — reusing the same `TcpEcho` driver across fixtures 0001 and 0003 proves that the harness is data-plane-agnostic. Cross-container host reachability is covered by ADR-0015; the `{{BACKEND_HOST}}` divergence between `envoy.yaml` and `envoy-rust.yaml` is its only non-harness divergence. Half-close posture is Envoy's v1.33.0 default (`enable_half_close: false`), covered by ADR-0016.

### D6 — Phase-01 rollovers folded in (I4 and M1)

Per ADR-0013's split, phase-01 REVIEW §9 items I4 and M1 land in 02.2. (I3 landed in 02.1.)

- **I4 — admin 8 KiB header cap tightening.** In `crates/envoy-bin/src/admin.rs:158–170`:
  - Replace `let n = stream.read(&mut scratch).await?;` with `let remaining = MAX_REQUEST_HEAD - buf.len(); let n = stream.read(&mut scratch[..remaining]).await?;` so the buffer cannot exceed `MAX_REQUEST_HEAD` by even one byte.
  - Update `rejects_oversized_request_headers` (admin.rs:~302) to write exactly `MAX_REQUEST_HEAD + 1` bytes and assert the 431 response.
  - Add `accepts_requests_exactly_at_cap` — `MAX_REQUEST_HEAD` bytes with a valid terminating CRLF-CRLF (e.g., a maximally-padded header block that still parses) → assert the 404 (unknown path) response fires, not 431.

- **M1 — stale `TODO(phase-01)` retarget.** In `tests/differential/src/subject.rs:25–32`, update the comment body from "deferred to phase 01 under its own ADR" to: the phase-00 I3 SIGKILL→SIGTERM switch awaits a future phase that takes the `nix` crate (or equivalent POSIX-signal surface) under its own ADR. Name no specific target phase — phase 01 and phase 02 (across 02.1 and 02.2) both chose not to take `nix`, so the deferral is open-ended. Doc-only change; no functional behavior shift.

### D7 — CI workflow

`.github/workflows/ci.yml` changes: none. The existing `build` job runs `cargo test --workspace`, which picks up the new `envoy-listener` and `envoy-tcp` crates automatically. The existing `fuzz` job exercises the 02.1-extended corpus via the same `cargo fuzz run parse_bootstrap -- -max_total_time=30` invocation. The `tcp-echo-server` binary is already a workspace member (landed in 02.1), so `cargo build --workspace --all-targets` compiles it and exposes `CARGO_BIN_EXE_tcp-echo-server` to same-package tests (envoy-rust doesn't use this env var for the differential harness; see §D4 for the cross-package path-lookup scheme).

The Docker-gated integration test `tests/differential/tests/tcp_proxy.rs` runs under the same gating pattern as `admin_ready.rs`.

### D8 — ADRs to land during execution

Two ADRs, appended to `docs/envoy-rust/DECISIONS.md` in order. Numbering reflects the ADR-0013 split: parent-SPEC §7's ADR-0014 becomes this ADR-0015; parent-SPEC §7's ADR-0015 becomes this ADR-0016. See §7 of this SPEC for the ADR texts.

Additional ADRs may be required during execution per D-3.5 if:

- Upstream Envoy v1.33.0's admin schema rejects `port_value: 0` on fixture 0003 (land an ADR documenting the workaround: extra templated `{{ENVOY_ADMIN_PORT}}`). Likely ADR-0017 if it trips.
- `cargo deny check` flips red on any new transitive surface from `envoy-listener`'s `tokio` feature set additions (unlikely — `tokio`'s feature matrix is already transitively present; verify during execution).
- Platform detection forces a second backend-host address (if `ubuntu-latest` CI's Docker install turns out to refuse `host-gateway`, fall back to `172.17.0.1` with an ADR; verify during execution).

---

## 4. Non-goals (deferred to later phases)

- TLS on listener (downstream) or cluster (upstream) — phase 03.
- `envoy.filters.network.echo`'s deprecation — stays supported for fixture 0001.
- `idle_timeout`, `max_connect_duration`, `tunneling_config`, `access_log` on tcp_proxy — phase 06 and later.
- `enable_half_close` configurability on tcp_proxy — deferred to a phase with a fixture that depends on FIN propagation (ADR-0016).
- Multiple listeners per process — `listeners.len() ∈ {0, 1}` cap stays.
- Cluster health checking, outlier detection, circuit breakers — §9 upstream-robustness family.
- `type: LOGICAL_DNS`, `type: STRICT_DNS`, `type: EDS` — phase 02 accepts only `STATIC` (validator landed in 02.1).
- `lb_policy: LEAST_REQUEST`, `RANDOM`, `RING_HASH`, `MAGLEV`, subset LB, locality-weighted LB, priority LB, panic thresholds — §9 load-balancing family (validator landed in 02.1).
- Listener filters (`listener_filters`), filter chain matchers (`filter_chain_matcher`), transport_socket, `per_connection_buffer_limit_bytes`, `connection_balance_config` — out of phase-02 surface.
- Filter chain framework / extension registry (trait registry, per-route config, iteration protocol) — phase 07.
- Stats subsystem, access logs, Prometheus — phase 06.
- Admin endpoints beyond phase 01's `/ready` — phase 08.
- Distribution-equivalence assertion on round-robin — parent-brainstorm Q1 decision: unit-test-only (unit tests landed in 02.1).
- `envoy-protos` crate + `prost` / `prost-build` + proto-tree vendoring — xDS family (§9) per ADR-0014 (landed in 02.1).
- `envoy-filter/` crate — phase 07.
- Long-budget nightly fuzz CRON — a future, scheduled phase.

---

## 5. Splitting guidance for the planner

Estimated scope:

| Surface | Net LoC (impl + tests) |
|---|---|
| envoy-listener crate (bind/serve/drain + 6 tests) | ~200 + ~150 |
| envoy-tcp crate (TcpProxy + ConnectionHandler impl + 4 tests) | ~120 + ~150 |
| envoy-bin wiring + `crates/envoy-bin/tests/tcp_proxy.rs` integration test | ~80 + ~80 |
| Harness extensions (`TcpProxyBackend`, `render_yaml`, `run_fixture` dispatch, upstream `with_host`, 5 unit tests) | ~150 + ~100 |
| Fixture 0003 (envoy.yaml, envoy-rust.yaml, inputs/payload.bin, expectations.yaml, README.md) | ~60 |
| Phase-01 rollovers I4 (admin cap tightening + 1 new test) + M1 (doc retarget) | ~10 + ~20 |
| ADRs 0015 / 0016 (docs) | ~0 |
| **Total** | **~1120 LoC; ~14 tasks** |

Both `BOOTSTRAP_PROMPT.md` §6 gates (> ~25 tasks OR > ~1500 LoC) hold comfortably at ~14 tasks / ~1120 LoC. **Do not split 02.2 further**. If the plan as actually written crosses either gate mid-write, invoke `superpowers:systematic-debugging` before attempting a nested split — nested splits of a split sub-phase were not anticipated at the parent-phase brainstorm and deserve a fresh root-cause analysis (scope creep vs. planner overdecomposition).

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the planner resolves them in-plan rather than mid-execution.

1. **Task ordering: `envoy-listener` lands before `envoy-tcp`.** `envoy-tcp::TcpProxy` depends on `envoy_listener::ConnectionHandler` and `envoy_listener::BoxFuture`. `envoy-tcp` in turn lands before `envoy-bin` wiring because `main::run` constructs a `TcpProxy`. Harness extensions (D4) land before the fixture (D5) because the fixture's integration test depends on `TcpProxyBackend` + `render_yaml` backend-key substitution + `run_fixture` dispatch.

2. **`envoy-listener`'s handler trait boxes the future.** Rust async-trait-without-crate idiom: declare `trait ConnectionHandler` with `fn handle(&self, ...) -> BoxFuture<'static, Result<...>>` instead of `async fn handle(...)`. This keeps the trait object-safe (required because `Listener::serve` stores `Arc<dyn ConnectionHandler>`) without pulling `async-trait` — which isn't on the D-3.2 permitted-foundations list. If a future phase (07) builds a richer extension registry, it may land `async-trait` under a dedicated ADR; phase 02.2 sidesteps with hand-boxed futures.

3. **`ConnectionHandler`'s error type is a trait object, not `anyhow::Error`.** Library crates cannot depend on `anyhow` per D-3.2. The trait returns `Result<(), Box<dyn std::error::Error + Send + Sync>>`, which `envoy-tcp::TcpProxy` produces by `.into()`-converting its `TcpProxyError` (it implements `std::error::Error` via `thiserror`). The binary crate (`envoy-bin`) is the only place `anyhow` is imported.

4. **Upstream-container host reachability via `host-gateway` requires a recent-enough Docker daemon.** On `ubuntu-latest` GitHub Actions runners, Docker CE ≥ 20.10 is standard (the feature landed there). Local-dev Docker Desktop (macOS/Windows/Linux) always supports it. If a developer is on an ancient Docker (<20.10), `with_host("host.docker.internal", Host::HostGateway)` either errors at container start or resolves to nothing. The fallback — `172.17.0.1` (default bridge gateway) — is an ADR-able Plan B; verify at execution time, land the fallback ADR only if it trips.

5. **`tokio::io::copy` propagates EOF but not every error symmetry.** If downstream → upstream succeeds while upstream → downstream errors, `try_join!` returns the upstream-side error and drops the downstream-side future. `Drop` on the write halves closes their sockets, which RSTs the open direction. That's acceptable — it aligns with Envoy's behavior of closing both sides on an asymmetric error. Review should flag any future phase that promotes `tokio::io::copy` to a custom loop without preserving this property.

6. **`enable_half_close: false` is Envoy's v1.33.0 default.** Not writing the key in the fixture YAML is equivalent to writing `false`. Don't "defensively" include it — the fixture should be minimal. See ADR-0016.

7. **`{{BACKEND_PORT}}` and `{{PORT}}` must be distinct.** The harness reserves two separate ephemeral ports for fixture 0003; the templates MUST NOT reuse `{{PORT}}` for the backend port. Review should flag any future fixture that collapses them.

8. **`tcp-echo-server` binary lookup from the differential crate.** `env!("CARGO_MANIFEST_DIR") + "/../../target/<profile>/tcp-echo-server"` is the canonical path because `CARGO_BIN_EXE_tcp-echo-server` is only set for tests in the `tcp-echo-server` package itself. `cargo test --workspace` always builds all workspace binaries before running tests, so existence is guaranteed. If the path lookup fails, it's a workspace-membership regression — not a 02.2 bug.

9. **Fixture 0003's `admin` block on envoy-rust side.** Intentionally absent. envoy-rust's admin endpoint lives in the phase-01 fixture 0002; fixture 0003 does not need it. Including `admin` would grow the differential surface into headers that ADR-0011 defers to phase 04.

10. **Payload choice for `inputs/payload.bin`.** Reuse fixture 0001's payload exactly to minimize cognitive load. The exact bytes don't matter — 1:1 echo semantics are what's tested.

11. **`testcontainers`'s `with_host` API signature.** At the `testcontainers = "0.23"` version pinned by ADR-0005, `with_host` takes `(name: &str, value: Host)` where `Host::HostGateway` is a named enum variant. The planner verifies this against the actual crate version in `Cargo.lock` during execution — if the API has shifted, adjust accordingly; not an ADR surface because we're not extending the exemption. (The `testcontainers` exemption in `deny.toml` already covers its transitive graph.)

12. **I4 unit-test delta is two tests, not one.** Tightening the read-slice bound changes behavior at the exact-cap boundary: the pre-tightening code accepted up to `MAX_REQUEST_HEAD + scratch_size` bytes in practice, masking a cap-boundary bug. The updated `rejects_oversized_request_headers` proves rejection at `MAX_REQUEST_HEAD + 1`; a new `accepts_requests_exactly_at_cap` proves acceptance at `MAX_REQUEST_HEAD`. Both are necessary — dropping either leaves the boundary unverified.

---

## 7. ADRs expected from this sub-phase

Two ADRs land during 02.2 execution, in `docs/envoy-rust/DECISIONS.md`, in order. Numbering reflects the ADR-0013 split: parent-SPEC §7's ADR-0014 (host-docker + host-gateway) becomes ADR-0015; parent-SPEC §7's ADR-0015 (`enable_half_close: false` default) becomes ADR-0016.

### ADR-0015 — Cross-container host reachability via `host.docker.internal` + `host-gateway`

- Context: Fixture 0003's upstream backend is an in-tree host-running `tcp-echo-server` process (landed in sub-phase 02.1). The upstream Envoy container (via testcontainers) and the envoy-rust host subprocess must both reach this backend. Container → host networking is platform-dependent: `host.docker.internal` resolves natively on Docker Desktop; Linux bridge networks require `--add-host=host.docker.internal:host-gateway`.
- Options considered:
  - **(i) Always-on `host.docker.internal` with `host-gateway` injected via testcontainers' `with_host`.** Standardizes on one name across dev/CI. `testcontainers` supports this natively.
  - **(ii) Runtime platform detection (`/.dockerenv`, `uname -r`, `docker info`) with `172.17.0.1` as a Linux-bridge fallback.** Two code paths; brittle against Docker config drift.
  - **(iii) Run the backend inside a Docker container on a shared network.** Loses the "backend is a host process" property; pulls container-network-management into every fixture's setup.
- Decision: (i). `with_host("host.docker.internal", Host::HostGateway)` on the upstream-Envoy container; `envoy.yaml` references `host.docker.internal:{{BACKEND_PORT}}`; `envoy-rust.yaml` references `127.0.0.1:{{BACKEND_PORT}}`.
- Rationale: one code path across macOS dev, Linux dev, and `ubuntu-latest` CI. `testcontainers` already handles the Docker-side plumbing.
- Consequences: every future fixture with a host-local backend follows the same pattern. If a later phase needs a backend inside a Docker network (e.g., multi-proxy topologies), that phase lands a separate testcontainers-networking ADR. If `ubuntu-latest`'s Docker turns out to refuse `host-gateway` (very unlikely; the feature has been GA since Docker CE 20.10), the fallback is `172.17.0.1` under a follow-up ADR — not a silent code change.

### ADR-0016 — Phase 02 TCP proxy runs with Envoy's default `enable_half_close: false`

- Context: ADR-0006/0007 documented the upstream-Envoy half-close-drops-pending-writes subtlety for the echo filter and the subsequent `drive_tcp` trailing-byte poll. `envoy.filters.network.tcp_proxy` has a YAML-visible `enable_half_close: true` toggle (unlike the echo filter, which has none). Fixture 0003's client pattern — `drive_tcp`: write → `read_exact(N)` → drop — does not depend on FIN propagation.
- Options considered:
  - **(i) Leave the default `false` on both `envoy.yaml` and the envoy-rust config.** Matches Envoy's v1.33.0 default; minimal fixture shape.
  - **(ii) Set `true` on both sides.** Pre-positions for FIN-sensitive use cases at the cost of a toggle that doesn't yet matter.
  - **(iii) Set `true` on one side only.** Divergent behavior under identical inputs; violates the "configs are initially identical" fixture principle.
- Decision: (i). `enable_half_close` is absent from both `envoy.yaml` and `envoy-rust.yaml` in fixture 0003. envoy-rust's `TcpProxy::handle` is implemented to match: `tokio::io::copy` on both directions, EOF on either side propagates via drop.
- Rationale: matches Envoy v1.33.0's default tcp_proxy posture; `drive_tcp`'s client pattern doesn't need half-close propagation for a 1:1 echo round-trip; minimizing the fixture's YAML keeps reviewer diffing tight. The ADR-0006/0007 precedent — "narrow fix, leave the grammar for when it pays for itself" — applies to the YAML toggle too.
- Consequences: phase 02.2's TCP proxy is explicitly *not* a drop-in for every Envoy tcp_proxy deployment; use cases depending on half-close propagation are phase-later work. A future fixture with an asymmetric-close requirement (one side writes, then expects the other side's FIN to trigger a response) lands its own ADR flipping the toggle and extending `TcpProxy` with an explicit half-close propagation mode. Until then, `enable_half_close` is a known non-surface.

---

## 8. Artifacts this sub-phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PLAN.md`
- `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PROGRESS.md`
- `docs/envoy-rust/phases/02.2-listener-tcp-proxy/REVIEW.md`
- `crates/envoy-listener/Cargo.toml`
- `crates/envoy-listener/src/lib.rs`
- `crates/envoy-tcp/Cargo.toml`
- `crates/envoy-tcp/src/lib.rs`
- `crates/envoy-bin/tests/tcp_proxy.rs`
- `tests/differential/src/backend.rs`
- `tests/differential/tests/tcp_proxy.rs`
- `tests/fixtures/0003-tcp-proxy/envoy.yaml`
- `tests/fixtures/0003-tcp-proxy/envoy-rust.yaml`
- `tests/fixtures/0003-tcp-proxy/inputs/payload.bin`
- `tests/fixtures/0003-tcp-proxy/expectations.yaml`
- `tests/fixtures/0003-tcp-proxy/README.md`

Amended during execution:

- Root `Cargo.toml` — add `crates/envoy-listener` and `crates/envoy-tcp` to `[workspace] members`. (`crates/envoy-cluster` and `tests/helpers/tcp-echo-server` are already there from 02.1.)
- `crates/envoy-bin/Cargo.toml` — add `envoy-cluster`, `envoy-listener`, `envoy-tcp` path deps.
- `crates/envoy-bin/src/main.rs` — construct `ClusterManager`; dispatch listener setup between `echo::serve` (echo filter) and `Listener::serve` + `TcpProxy` (tcp_proxy filter).
- `crates/envoy-bin/src/admin.rs` — apply I4 tightening (read-slice bounded by `MAX_REQUEST_HEAD - buf.len()`).
- `tests/differential/Cargo.toml` — verify at plan time whether `tracing` is already a dev-dep; if not, add it for the backend helper's diagnostics. No new *non-D-3.2* deps.
- `tests/differential/src/lib.rs` — add `TcpProxyBackend` module re-export; extend `render_yaml` per-driver substitution logic; extend `run_fixture` to spawn the backend when `{{BACKEND_PORT}}` appears in the template. Add 3 new unit tests (`fixture_0003_expectations_parses_as_tcp_echo`, `render_yaml_substitutes_backend_keys_for_envoy_side`, `render_yaml_substitutes_backend_keys_for_envoy_rust_side`).
- `tests/differential/src/subject.rs` — retarget `TODO(phase-01)` comment (M1); doc-only.
- `tests/differential/src/upstream.rs` — extend testcontainers config to add `with_host("host.docker.internal", Host::HostGateway)` when a fixture uses a backend.
- `tests/fixtures/0001-tcp-echo/` — unchanged.
- `tests/fixtures/0002-static-admin-ready/` — unchanged.
- `docs/envoy-rust/DECISIONS.md` — ADR-0015 and ADR-0016 appended.
- `docs/envoy-rust/ROADMAP.md` — row 02.2 `status` → `done` in the final commit; *at the same commit* row 02 (parent) `status` → `done` (per the ROADMAP schema: "The parent flips to `done` only after all sub-phases are `done`.") — since 02.1 will already be `done` at 02.2 start, landing 02.2 `done` completes the parent. Update both rows in the same commit.
- `docs/envoy-rust/STATE.md` — active → `03-tls-tcp` (slug consistent with §8 of `BOOTSTRAP_PROMPT.md`), next-skill → `superpowers:brainstorming` (phase 03 state 0/1), state detection: phase 03 directory does not exist yet.
- `deny.toml` — only if `cargo deny check` flags a new transitive license from `envoy-listener`'s or `envoy-tcp`'s `tokio` feature set additions (expected: no — already in scope via `envoy-bin`).

Not touched in 02.2 (belong to 02.1 or are frozen):

- `crates/envoy-cluster/` — finalized in 02.1.
- `crates/envoy-config/src/bootstrap.rs`, `crates/envoy-config/src/lib.rs` — finalized in 02.1.
- `tests/helpers/tcp-echo-server/` — finalized in 02.1.
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_*.yaml` — finalized in 02.1.
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `50349da`.
- `docs/envoy-rust/phases/02.1-config-cluster/` — landed and finalized before 02.2 begins.

---

## 9. Final commit message format (for state 6 of the 02.2 lifecycle)

```
phase 02.2: Listener + TCP proxy filter + fixture 0003 [ADR-0015, ADR-0016]

Two new crates land the first real data-plane path: envoy-listener manages
bind/accept/drain with a shutdown-gated JoinSet; envoy-tcp implements the
TCP proxy filter via tokio::io::copy. envoy-bin wires a ClusterManager +
echo/tcp_proxy dispatch. Differential harness extends with TcpProxyBackend,
render_yaml backend-key substitution, run_fixture dispatch, and upstream
with_host("host.docker.internal", HostGateway). Fixture 0003-tcp-proxy lands
green end-to-end. Phase-01 REVIEW §9 starter items I4 (admin cap tightening)
and M1 (stale TODO retarget) close alongside. Parent phase 02 row flips
done in the same commit.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (byte-exact payload round-trip through
  tcp_proxy → STATIC cluster, one endpoint, to host-local tcp-echo-server).
Conformance: none.
```
