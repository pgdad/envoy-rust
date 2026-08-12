#![forbid(unsafe_code)]

use std::net::SocketAddr;

use anyhow::{Context, Result};
use tokio::net::TcpListener;

mod argv;
mod direct_response;
mod echo;
mod network_rbac;
mod runtime_stats;
mod tls_handler;

fn main() -> std::process::ExitCode {
    match argv::parse_argv(std::env::args()) {
        Ok(path) => {
            install_tracing();
            match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("building tokio runtime")
            {
                Ok(rt) => match rt.block_on(run(path)) {
                    Ok(()) => std::process::ExitCode::SUCCESS,
                    Err(err) => {
                        tracing::error!(error = ?err, "envoy-rust exited with error");
                        std::process::ExitCode::from(1)
                    }
                },
                Err(err) => {
                    eprintln!("{err:#}");
                    std::process::ExitCode::from(1)
                }
            }
        }
        Err(err) => {
            eprintln!("envoy-bin: {err}");
            std::process::ExitCode::from(2)
        }
    }
}

fn install_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};
    let filter =
        EnvFilter::try_from_env("ENVOY_RUST_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_target(false).init();
}

async fn run(config_path: std::path::PathBuf) -> Result<()> {
    let yaml = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config at {}", config_path.display()))?;
    let mut bootstrap = envoy_config::parse_bootstrap(&yaml)?;
    // 18 D3: read + parse + merge the CDS file (if configured) BEFORE any
    // consumer iterates clusters. All failures are fatal (ADR-0049 L4).
    envoy_config::load_dynamic_resources(&mut bootstrap)?;
    let bootstrap = std::sync::Arc::new(bootstrap);

    // 109.1: the boot runtime snapshot, built ONCE and Arc-shared into every
    // HCMConfig (route runtime_fraction gates read it; admin /runtime keeps
    // its own per-request rebuild, deliberately untouched).
    let runtime_snapshot = std::sync::Arc::new(
        envoy_config::runtime::RuntimeSnapshot::from_bootstrap(&bootstrap),
    );

    // Phase 08.1 D13a: capture the process-start `Instant` once at startup
    // so the admin `/server_info` renderer (Task 6) can compute uptime as
    // `Instant::now().duration_since(start_instant)`. Mirrors the pre-task
    // shape: a single value built once, cloned (here, copied) into the
    // admin handler at construction.
    let start_instant = std::time::Instant::now();

    // Phase 08.1 D13a: build the admin `command_line_options` map once at
    // startup per architecture lock-in #7. Currently `envoy-bin` only knows
    // about the `-c` / `--config-path` flag; the map carries a single entry
    // `{"config_path": Value::String(<-c value>)}`. Future flags (e.g.,
    // `--mode`, `--log-level`) extend this map.
    let mut command_line_options: std::collections::BTreeMap<String, serde_yaml::Value> =
        std::collections::BTreeMap::new();
    command_line_options.insert(
        "config_path".to_string(),
        serde_yaml::Value::String(config_path.display().to_string()),
    );

    // Per SPEC §6 signpost 4: rustls's aws-lc-rs default provider must be
    // installed once per process before any TLS-touching code runs. The
    // call returns `Err(_)` on second-or-later calls (no-op). Routed
    // through envoy-tls per SPEC §3 D1 / ADR-0019 so envoy-bin does not
    // hold a direct rustls dep.
    let _ = envoy_tls::install_default_crypto_provider();

    if let Some(node) = bootstrap.node.as_ref() {
        tracing::info!(
            node.id = %node.id,
            node.cluster = %node.cluster,
            "node registered",
        );
    }

    let token = tokio_util::sync::CancellationToken::new();
    let signal_token = token.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        signal_token.cancel();
    });

    let mut set: tokio::task::JoinSet<Result<()>> = tokio::task::JoinSet::new();

    // 06.1 D4: build the global StatsRegistry once at process startup.
    // envoy-bin owns the constructor; consumers (cluster_mgr, listener-walk,
    // HCMConfig::from_config) receive `Arc::clone(&registry)` per the
    // cross-sub-phase architectural rule "envoy-bin owns the global
    // Arc<StatsRegistry>" (parent SPEC §3 D4 + 06.1 PLAN Task 10 Step I).
    let registry: std::sync::Arc<envoy_stats::StatsRegistry> =
        std::sync::Arc::new(envoy_stats::StatsRegistry::new());

    // 19 D4: conditional listener_manager.lds.* registration (no-op when
    // lds_config is unconfigured — the §5.2 inertness invariant). Runs after
    // load_dynamic_resources (line 54) has populated dynamic_listeners, so
    // listener_added counts static + dynamic correctly (the L3 lesson).
    envoy_listener::register_lds_stats(&bootstrap, &registry)
        .context("registering listener_manager.lds stats")?;
    // 20 D4: conditional per-HCM http.<prefix>.rds.<name>.* registration (no-op
    // when no HCM uses rds — the §5.2 inertness invariant).
    envoy_listener::register_rds_stats(&bootstrap, &registry)
        .context("registering http.*.rds.* stats")?;
    // 108.2 D5: the nine runtime.* stats, registered UNCONDITIONALLY —
    // upstream emits all nine even with no `layered_runtime` block (SPEC §2).
    runtime_stats::register_runtime_stats(&bootstrap, &registry)
        .context("registering runtime.* stats")?;

    // 08.2 D13b: construct the shared DrainState ONCE at startup. Cloned
    // into the admin handler (writer; for the 3 POST endpoints + /server_info
    // state read + /ready drain-aware response) and into every data-plane
    // Listener::serve call (reader/observer per D12 — the tcp_proxy + HCM
    // paths). The echo path (fixture 0002 only) and the admin path itself
    // use TcpListener::bind directly and are naturally excluded from drain
    // observation per 08.2 PLAN architecture-decision lock-in #12.
    let drain: std::sync::Arc<envoy_listener::DrainState> =
        std::sync::Arc::new(envoy_listener::DrainState::new(&registry));

    // Build the cluster manager once. Empty `clusters` is permitted at the
    // envoy-config validator (admin-only configs); the manager is empty in
    // that case and `tcp_proxy` filters reference clusters by name, which
    // the validator already verified exist (`ConfigError::UnknownCluster`).
    // Phase 08.1 D13a: `bootstrap` is `Arc<Bootstrap>` (was `Bootstrap` before
    // Task 5). `envoy_cluster::from_bootstrap` takes `&Bootstrap`; `&bootstrap`
    // coerces via Arc's `Deref` impl in function-arg position. Field accesses
    // below (`bootstrap.node`, `bootstrap.static_resources.*`, `bootstrap.admin`)
    // similarly auto-deref.
    let cluster_mgr = std::sync::Arc::new(
        envoy_cluster::from_bootstrap(&bootstrap, std::sync::Arc::clone(&registry))
            .await
            .context("building cluster manager")?,
    );

    // 13.1 Task 4 (D4): build the shared `H1PoolManager` once after
    // `cluster_mgr` and before any HCMConfig consumer. One H1 pool per
    // H1-protocol cluster (default-enabled per lock-in #2); H2-protocol
    // clusters defer their pool to 13.2. Mirrors the 12.2
    // `envoy-health::Scheduler` external-injection precedent (lock-in #1);
    // threaded into every `HCMConfig::from_config` below as
    // `Some(Arc::clone(&pool_mgr))`. The idle-sweeper tasks owned by the
    // manager abort cleanly on `token` cancel.
    let pool_mgr = envoy_http1::H1PoolManager::for_bootstrap(
        &bootstrap,
        &cluster_mgr,
        std::sync::Arc::clone(&registry),
        token.clone(),
    )
    .context("building H1 pool manager")?;

    // 13.2 Task 2 (D6): build the shared `H2PoolManager` once after
    // `pool_mgr` (H1) and before any HCMConfig consumer. One H2 pool per
    // H2-protocol cluster (default-enabled per lock-in #3). Mirrors the
    // 13.1 H1 cycle-resolution pattern (lock-in #1); threaded into every
    // H2 `HCMConfig::wrap` below as `Some(Arc::clone(&h2_pool_mgr))`. The
    // idle-sweeper tasks owned by the manager abort cleanly on `token`
    // cancel.
    let h2_pool_mgr = envoy_http2::H2PoolManager::for_bootstrap(
        &bootstrap,
        &cluster_mgr,
        std::sync::Arc::clone(&registry),
        token.clone(),
    )
    .context("building H2 pool manager")?;

    // 12.2 (parent-12 D4): spawn active-HC probe tasks for every cluster
    // carrying `health_checks`. Cancellation wired to the existing signal
    // token so SIGTERM/SIGINT triggers clean shutdown of every probe task at
    // its next `tokio::select!` boundary. The scheduler holds JoinHandles
    // until `shutdown().await` on the runtime drain path below.
    let health_scheduler = envoy_health::Scheduler::spawn(
        &bootstrap,
        std::sync::Arc::clone(&cluster_mgr),
        std::sync::Arc::clone(&registry),
        token.clone(),
    )
    .context("building active-HC scheduler")?;

    // 14.2 D7 (lock-in #11): the fourth periodic-background primitive. Spawns one ejection
    // sweeper per cluster that configures outlier_detection; inert (zero sweepers) otherwise.
    // Mirrors the 12.2 health scheduler + 13.x pool managers: constructed after them, passed
    // `token.clone()`, and drained via `shutdown().await` on the runtime drain path below.
    // `for_bootstrap` takes `&ClusterManager`; `&cluster_mgr` (an `Arc<ClusterManager>`)
    // auto-derefs at the call, matching how the H1/H2 pool managers consume it above.
    let outlier_mgr = envoy_cluster::OutlierManager::for_bootstrap(&cluster_mgr, token.clone());

    // 03.2: per-cluster Arc<UpstreamTls> construction. Build once at startup
    // and reuse across all per-connection invocations of `handle`.
    let upstream_tls_by_cluster = build_upstream_tls_map(&bootstrap)?;

    // 26 Task 3: the 5th periodic-background primitive — the `RdsWatcher`.
    // Built by walking the listeners for HCMs with `rds` configured (the §5.2
    // inertness invariant: empty target list ⇒ zero watch tasks when no HCM
    // uses rds). Populated in the HCM dispatch arm below — it needs the
    // post-construction `Arc<HCMConfig>` (the Task-2 swappable route-table
    // handle) plus the rds file path/name — then spawned AFTER the
    // listener-serve block and drained via `shutdown().await` on the runtime
    // drain path, mirroring the 12.2 health scheduler / 14.2 outlier manager.
    let mut rds_targets: Vec<envoy_http1::WatchTarget> = Vec::new();

    // 67.1 D5: the chain is split into its NON-TERMINAL prefix and its TERMINAL
    // last filter. Before 67.1 this read `filters.first()` and ignored the rest,
    // safe ONLY because phase 66's terminal validation made every ≥2-filter chain
    // invalid. `envoy.filters.network.rbac` is the first non-terminal filter, so
    // that interlock is gone.
    //
    // ADR-0130 §2: an EMPTY `filters: []` chain is ACCEPTED by the config
    // validator — measured parity with upstream Envoy, which accepts and STARTS
    // on the same config (phase-67 SPEC R-7, the finding that closed M66-5).
    // envoy-rust used to PANIC here (`validator guarantees ≥1 filter`). It now
    // binds no data listener and warns; the admin listener, spawned independently
    // below, still serves.
    //
    // What upstream Envoy does with a CONNECTION to such a listener has not been
    // probed, so envoy-rust asserts nothing about it. Carried forward as CF-67-5.
    // Recorded in BEHAVIOR_CONTRACT.md as a divergence with no differential
    // observable — no fixture configures an empty chain.
    // M-8: envoy-rust serves only the FIRST listener, and only its first filter
    // chain. **Pre-existing — NOT introduced by 67.1**, which merely moved the
    // network-filter chain refactor inside this block. Noted here so a future
    // session does not read `.next()` / `.first()` as new, and so whichever phase
    // lands multi-listener support knows this is the site.
    if let Some(listener_cfg) = bootstrap.all_listeners().next() {
        let Some(chain) = listener_cfg.filter_chains.first() else {
            anyhow::bail!("listener {:?} has no filter_chains", listener_cfg.name);
        };
        if let Some((prefix, terminal)) = split_chain(chain) {
            let sock = &listener_cfg.address.socket_address;
            let bind_addr: SocketAddr = format!("{}:{}", sock.address, sock.port_value)
                .parse()
                .with_context(|| {
                    format!(
                        "parsing listener address {}:{}",
                        sock.address, sock.port_value
                    )
                })?;

            // Number of SO_REUSEPORT accept sockets to request when the listener has
            // `enable_reuse_port` (the default) — one per logical CPU, matching the
            // tokio runtime's default worker-thread count so each worker drives its
            // own kernel accept queue. `Listener::bind_with_concurrency` clamps this
            // to a single plain socket when reuse_port is off, the platform is not
            // Linux, or the value is 1 — so non-Linux/dev runs are unchanged.
            let listener_concurrency = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1);

            // 67.1 D5: the non-terminal prefix, built once at startup and shared by
            // every accepted connection. Its counters are `Arc<Counter>`s, so even
            // the thread-per-core path's N shards tick ONE counter set.
            let chain_filters = build_network_filter_chain(&prefix, &registry)?;

            match terminal.name.as_str() {
                envoy_config::ECHO_FILTER => {
                    bind_and_spawn_listener(
                        listener_cfg,
                        wrap_in_chain(chain_filters, std::sync::Arc::new(echo::EchoHandler)),
                        &registry,
                        listener_concurrency,
                        "echo",
                        bind_addr,
                        || tracing::info!(addr = %bind_addr, "envoy-rust listening (echo)"),
                        &token,
                        &drain,
                        &mut set,
                    )
                    .await?;
                }
                envoy_config::DIRECT_RESPONSE_FILTER => {
                    let Some(envoy_config::TypedConfig::DirectResponse(dr_cfg)) =
                        terminal.typed_config.as_ref()
                    else {
                        anyhow::bail!(
                            "validator guarantees a DirectResponse typed_config on {}",
                            envoy_config::DIRECT_RESPONSE_FILTER
                        );
                    };
                    // `response` omitted => empty payload (SPEC §0 R-0.7).
                    let payload: std::sync::Arc<[u8]> = dr_cfg
                        .response
                        .as_ref()
                        .map(|d| d.inline_string.as_bytes())
                        .unwrap_or(&[])
                        .into();
                    let payload_len = payload.len();
                    // ADR-0132 decision 2: `direct_response` BYPASSES the chain.
                    //
                    // Upstream Envoy runs every filter's `onNewConnection` at
                    // connection establishment — the TERMINAL filter's included —
                    // and defers only the RBAC *verdict* to the first downstream
                    // byte. `direct_response` writes its payload and closes at
                    // establishment, so `onData` never fires and the network
                    // `rbac` filter never evaluates: measured, all four counters
                    // stay 0 even under `action: DENY`, and the payload is
                    // delivered regardless.
                    //
                    // So no `wrap_in_chain` here. `chain_filters` was still built
                    // above, which is what REGISTERS the `<stat_prefix>.rbac.*`
                    // counters at 0 and keeps the stat tree matching upstream's.
                    // Dropping it here is the point: the filters never run.
                    //
                    // Witnessed by `direct_response_delivers_payload_to_a_client_that_sends_nothing`
                    // and `deny_does_not_suppress_the_direct_response_payload`.
                    drop(chain_filters);
                    bind_and_spawn_listener(
                    listener_cfg,
                    std::sync::Arc::new(direct_response::DirectResponseHandler::new(payload)),
                    &registry,
                    listener_concurrency,
                    "direct_response",
                    bind_addr,
                    || tracing::info!(addr = %bind_addr, payload_len, "envoy-rust listening (direct_response)"),
                    &token,
                    &drain,
                    &mut set,
                )
                .await?;
                }
                envoy_config::TCP_PROXY_FILTER => {
                    let Some(envoy_config::TypedConfig::TcpProxy(tp_cfg)) =
                        terminal.typed_config.as_ref()
                    else {
                        anyhow::bail!(
                            "filter '{}' missing typed_config; envoy-config validator should have rejected at parse time",
                            envoy_config::TCP_PROXY_FILTER,
                        );
                    };
                    let cluster = cluster_mgr
                        .get(&tp_cfg.cluster)
                        .expect("validator guarantees cluster present");
                    // 03.2: per-cluster upstream-TLS dispatch per SPEC §D5 step 2.
                    // If this cluster carried `transport_socket: UpstreamTlsContext`
                    // we built an `Arc<UpstreamTls>` at startup and select the
                    // TLS-upstream constructor; otherwise the plaintext-upstream
                    // constructor (03.1 path).
                    let proxy: std::sync::Arc<envoy_tcp::TcpProxy> =
                        match upstream_tls_by_cluster.get(&tp_cfg.cluster) {
                            Some(upstream_tls) => {
                                std::sync::Arc::new(envoy_tcp::TcpProxy::with_upstream_tls(
                                    cluster,
                                    tp_cfg,
                                    upstream_tls.clone(),
                                ))
                            }
                            None => std::sync::Arc::new(envoy_tcp::TcpProxy::new(cluster, tp_cfg)),
                        };

                    // 03.2: three-way TLS dispatch per SPEC §D5 step 1, factored
                    // through `build_downstream_tls_for_listener` (04.1 task 11)
                    // so the new HCM arm below shares the same per-listener
                    // logic. See the helper for the plaintext / single-cert /
                    // multi-cert branching.
                    let downstream_tls = build_downstream_tls_for_listener(listener_cfg)?;

                    let handler: std::sync::Arc<dyn envoy_listener::ConnectionHandler> =
                        match downstream_tls {
                            Some(tls) => std::sync::Arc::new(tls_handler::TlsAcceptingHandler {
                                tls,
                                inner: proxy,
                            }),
                            None => proxy,
                        };
                    // 67.1: the chain runs on the raw TcpStream, BEFORE the TLS
                    // handshake. For `any` (67.1) and the peer/local-address arms
                    // (67.2) the verdict is identical either way — TLS alters neither
                    // address.
                    let handler = wrap_in_chain(chain_filters, handler);

                    bind_and_spawn_listener(
                    listener_cfg,
                    handler,
                    &registry,
                    listener_concurrency,
                    "tcp_proxy",
                    bind_addr,
                    || tracing::info!(addr = %bind_addr, cluster = %tp_cfg.cluster, "envoy-rust listening (tcp_proxy)"),
                    &token,
                    &drain,
                    &mut set,
                )
                .await?;
                }
                envoy_config::HCM_FILTER => {
                    let Some(envoy_config::TypedConfig::HttpConnectionManager(hcm_cfg)) =
                        terminal.typed_config.as_ref()
                    else {
                        anyhow::bail!(
                            "filter '{}' missing typed_config; envoy-config validator should have rejected at parse time",
                            envoy_config::HCM_FILTER,
                        );
                    };

                    // Thread-per-core dispatch: when eligible, serve this HCM
                    // listener with one single-threaded runtime per worker, each
                    // owning its own SO_REUSEPORT accept socket and its own
                    // upstream connection pools — upstream Envoy's per-worker-
                    // dispatcher architecture. Every connection's downstream and
                    // upstream I/O is then registered with, and polled by, the
                    // same thread, so the cross-thread wakeups of the shared
                    // multi-threaded runtime (scheduler unparks, work stealing,
                    // pooled sockets owned by another worker's driver) disappear.
                    //
                    // Gated to: Linux + enable_reuse_port (SO_REUSEPORT load-
                    // balancing), >= 2 workers, H1 codec, no rds (per-worker
                    // route tables would not observe hot reloads), no downstream
                    // TLS (falls through to the existing detect-and-bail).
                    //
                    // Opt-in via `ENVOY_RUST_TPC=1`, NOT opt-out: each worker
                    // builds its own HCMConfig (fresh filter-chain instances), so
                    // any filter or resource limit whose state must be shared
                    // across the whole listener — the local_ratelimit token
                    // bucket, the max_connections circuit-breaker counter — is
                    // silently fragmented into N independent per-worker copies
                    // instead of one. Confirmed regressions: the differential
                    // `http_filter_local_rate_limit` fixture, the envoy-bin
                    // backstops `local_rate_limit_enforces_429_after_token_exhaustion`
                    // and `cx_overflow_yields_200_503_multiset_and_cx_open_both_edges`.
                    // Default off until per-worker state sharing is fixed.
                    let tpc_workers = if cfg!(target_os = "linux")
                        && listener_cfg.enable_reuse_port
                        && listener_concurrency > 1
                        && matches!(
                            hcm_cfg.codec_type,
                            envoy_config::CodecType::AUTO | envoy_config::CodecType::HTTP1
                        )
                        && hcm_cfg.rds.is_none()
                        && build_downstream_tls_for_listener(listener_cfg)?.is_none()
                        && std::env::var("ENVOY_RUST_TPC")
                            .map(|v| v == "1")
                            .unwrap_or(false)
                    {
                        listener_concurrency
                    } else {
                        0
                    };

                    if tpc_workers >= 2 {
                        // EXPERIMENTAL io_uring data plane (see envoy_http1::uring):
                        // compiled behind `--features uring`, engaged only with
                        // ENVOY_RUST_URING=1. Each worker thread runs a monoio
                        // io_uring runtime and binds its own SO_REUSEPORT socket;
                        // envoy-listener is bypassed for this listener.
                        #[cfg(all(feature = "uring", target_os = "linux"))]
                        let uring_enabled = std::env::var("ENVOY_RUST_URING")
                            .map(|v| v == "1")
                            .unwrap_or(false);
                        #[cfg(not(all(feature = "uring", target_os = "linux")))]
                        let uring_enabled = false;
                        #[cfg(not(all(feature = "uring", target_os = "linux")))]
                        if std::env::var("ENVOY_RUST_URING")
                            .map(|v| v == "1")
                            .unwrap_or(false)
                        {
                            tracing::warn!(
                                "ENVOY_RUST_URING=1 ignored: binary built without the 'uring' feature"
                            );
                        }

                        if uring_enabled {
                            #[cfg(all(feature = "uring", target_os = "linux"))]
                            {
                                for i in 0..tpc_workers {
                                    let worker_config = std::sync::Arc::new(
                                        envoy_http1::HCMConfig::from_config(
                                            hcm_cfg,
                                            std::sync::Arc::clone(&cluster_mgr),
                                            std::sync::Arc::clone(&registry),
                                            None, // the uring worker keeps its own per-worker pool
                                            std::sync::Arc::clone(&runtime_snapshot),
                                        )
                                        .await?,
                                    );
                                    let worker = envoy_http1::uring::UringWorker {
                                        addr: bind_addr,
                                        listener_name: listener_cfg.name.clone(),
                                        config: worker_config,
                                        registry: std::sync::Arc::clone(&registry),
                                        token: token.clone(),
                                    };
                                    let (done_tx, done_rx) =
                                        tokio::sync::oneshot::channel::<std::io::Result<()>>();
                                    std::thread::Builder::new()
                                        .name(format!("envoy-uring-{i}"))
                                        .spawn(move || {
                                            let _ = done_tx
                                                .send(envoy_http1::uring::run_worker(worker));
                                        })
                                        .with_context(|| {
                                            format!("spawning uring worker thread {i}")
                                        })?;
                                    set.spawn(async move {
                                    match done_rx.await {
                                        Ok(res) => res.map_err(anyhow::Error::from),
                                        Err(_) => Err(anyhow::anyhow!(
                                            "uring worker thread {i} exited without reporting a result"
                                        )),
                                    }
                                });
                                }
                                tracing::info!(
                                    addr = %bind_addr,
                                    workers = tpc_workers,
                                    stat_prefix = %hcm_cfg.stat_prefix,
                                    "envoy-rust listening (http_connection_manager, thread-per-core, io_uring)",
                                );
                            }
                        } else {
                            // Per-worker handler stack: each worker gets its OWN
                            // H1PoolManager (upstream connections stay on the thread
                            // that created them) and its own HCMConfig. All stats
                            // registrations are idempotent by name, so the N workers
                            // share one set of counters/gauges — per-worker pools,
                            // process-wide aggregated stats, exactly like Envoy.
                            let mut handlers: Vec<
                                std::sync::Arc<dyn envoy_listener::ConnectionHandler>,
                            > = Vec::with_capacity(tpc_workers);
                            for _ in 0..tpc_workers {
                                let worker_pool_mgr = envoy_http1::H1PoolManager::for_bootstrap(
                                    &bootstrap,
                                    &cluster_mgr,
                                    std::sync::Arc::clone(&registry),
                                    token.clone(),
                                )
                                .context("building per-worker H1 pool manager")?;
                                let worker_config = std::sync::Arc::new(
                                    envoy_http1::HCMConfig::from_config(
                                        hcm_cfg,
                                        std::sync::Arc::clone(&cluster_mgr),
                                        std::sync::Arc::clone(&registry),
                                        Some(worker_pool_mgr),
                                        std::sync::Arc::clone(&runtime_snapshot),
                                    )
                                    .await?,
                                );
                                // 67.1 D5: wrap each shard's handler. The chain's
                                // `Arc<dyn NetworkFilter>`s are cheap to clone and the
                                // counters inside are shared `Arc<Counter>`s, so N
                                // shards tick ONE counter set — which is correct.
                                handlers.push(wrap_in_chain(
                                    chain_filters.clone(),
                                    std::sync::Arc::new(envoy_http1::HCM {
                                        config: worker_config,
                                    }),
                                ));
                            }
                            let shards = envoy_listener::Listener::bind_per_worker(
                                listener_cfg,
                                handlers,
                                std::sync::Arc::clone(&registry),
                            )
                            .await
                            .with_context(|| {
                                format!("binding sharded HCM listener to {bind_addr}")
                            })?;
                            tracing::info!(
                                addr = %bind_addr,
                                workers = tpc_workers,
                                stat_prefix = %hcm_cfg.stat_prefix,
                                "envoy-rust listening (http_connection_manager, thread-per-core)",
                            );
                            for (i, shard) in shards.into_iter().enumerate() {
                                let shutdown = token.clone().cancelled_owned();
                                let drain_for_worker = std::sync::Arc::clone(&drain);
                                let (done_tx, done_rx) = tokio::sync::oneshot::channel::<
                                    Result<(), envoy_listener::ListenerError>,
                                >();
                                std::thread::Builder::new()
                                    .name(format!("envoy-worker-{i}"))
                                    .spawn(move || {
                                        let result =
                                            match tokio::runtime::Builder::new_current_thread()
                                                .enable_all()
                                                .build()
                                            {
                                                Ok(rt) => rt.block_on(
                                                    shard.serve(shutdown, drain_for_worker),
                                                ),
                                                Err(e) => {
                                                    Err(envoy_listener::ListenerError::Accept(e))
                                                }
                                            };
                                        let _ = done_tx.send(result);
                                    })
                                    .with_context(|| format!("spawning worker thread {i}"))?;
                                // Bridge the worker thread's exit back into the main
                                // JoinSet so shutdown/error handling below treats it
                                // like any other listener task. A dropped sender means
                                // the worker thread died without reporting (panic).
                                set.spawn(async move {
                                    match done_rx.await {
                                        Ok(res) => res.map_err(|e| anyhow::anyhow!(e)),
                                        Err(_) => Err(anyhow::anyhow!(
                                            "worker thread {i} exited without reporting a result"
                                        )),
                                    }
                                });
                            }
                            // The TPC path serves this listener; skip the shared-
                            // runtime construction below. (No rds by the gate above,
                            // so no watch-target wiring is skipped.)
                        }
                    } else {
                        let hcm_config = std::sync::Arc::new(
                            envoy_http1::HCMConfig::from_config(
                                hcm_cfg,
                                std::sync::Arc::clone(&cluster_mgr),
                                std::sync::Arc::clone(&registry),
                                Some(std::sync::Arc::clone(&pool_mgr)),
                                std::sync::Arc::clone(&runtime_snapshot),
                            )
                            .await?,
                        );

                        // 26 Task 3: if this HCM is rds-configured, register a watch
                        // target. `hcm_config` (the h1 `Arc<HCMConfig>`) is the
                        // swappable route-table owner — for the H2 path below it is
                        // wrapped as the `.inner` of `envoy_http2::HCMConfig`, so this
                        // SAME h1 handle is the one both dispatch paths read; the
                        // watcher's `store` must be it (NOT the H2 wrapper, which
                        // holds no swappable cell). Path/name come from the parsed
                        // `rds` block (`rds.config_source.path_config_source.path` +
                        // `rds.route_config_name`). §5.2: HCMs WITHOUT rds add nothing.
                        if let Some(rds) = hcm_cfg.rds.as_ref() {
                            // 26 Task 4 (Task 5 folded in): re-resolve the 5 `rds.*`
                            // counters the initial-load `register_rds_stats` already
                            // registered (line ~116). `register_counter` is idempotent
                            // by name, so this returns the SAME underlying handles —
                            // the watcher continues the series initial load seeded at
                            // `1/1/0/0/1`, ticking it per the §6.2 taxonomy on reload.
                            // Shared base-name helper: MUST stay byte-identical to the
                            // initial-load `register_rds_stats` (idempotent register-by-name
                            // returns the SAME handles); see `envoy_listener::rds_counter_base`.
                            let base = envoy_listener::rds_counter_base(
                                &hcm_cfg.stat_prefix,
                                &rds.route_config_name,
                            );
                            let mk = |suffix: &str| {
                                registry
                                    .register_counter(&format!("{base}.{suffix}"))
                                    .map_err(|e| anyhow::anyhow!(e))
                            };
                            let counters = envoy_http1::RdsCounters {
                                update_attempt: mk("update_attempt")?,
                                update_success: mk("update_success")?,
                                update_failure: mk("update_failure")?,
                                update_rejected: mk("update_rejected")?,
                                config_reload: mk("config_reload")?,
                            };
                            rds_targets.push(envoy_http1::WatchTarget {
                                path: std::path::PathBuf::from(
                                    &rds.config_source.path_config_source.path,
                                ),
                                route_config_name: rds.route_config_name.clone(),
                                store: std::sync::Arc::clone(&hcm_config),
                                counters,
                            });
                        }

                        // 05.2 NEW: H1-vs-H2 dispatch on hcm_cfg.codec_type.
                        // - AUTO / HTTP1 → envoy_http1::HCM (existing 04.x path)
                        // - HTTP2       → envoy_http2::HCM (new in 05.2)
                        // - HTTP3       → bail (validator rejected at parse time)
                        let hcm: std::sync::Arc<dyn envoy_listener::ConnectionHandler> =
                            match hcm_cfg.codec_type {
                                envoy_config::CodecType::AUTO | envoy_config::CodecType::HTTP1 => {
                                    std::sync::Arc::new(envoy_http1::HCM { config: hcm_config })
                                }
                                envoy_config::CodecType::HTTP2 => {
                                    // 13.2 Task 2 (D6, lock-in #2): wrap the H1
                                    // HCMConfig in the new `envoy_http2::HCMConfig`
                                    // struct, threading the H2 pool manager. The
                                    // H2 HCM's dispatch arm uses
                                    // `config.h2_pool_mgr` for proxy upstream
                                    // dispatch; H1 inner fields are accessed via
                                    // `config.inner.<field>`.
                                    std::sync::Arc::new(envoy_http2::HCM::new(std::sync::Arc::new(
                                        envoy_http2::HCMConfig::wrap(
                                            std::sync::Arc::clone(&hcm_config),
                                            Some(std::sync::Arc::clone(&h2_pool_mgr)),
                                        ),
                                    )))
                                }
                                envoy_config::CodecType::HTTP3 => {
                                    anyhow::bail!(
                                        "CodecType::HTTP3 should have been rejected by the envoy-config \
                             validator at parse time; this is a validator bug",
                                    );
                                }
                            };

                        // TLS-detect-and-bail: only meaningful for the H1 path.
                        // For H2 the validator already rejected TLS+HTTP2 at parse
                        // time (Http2OverTlsNotSupported) so this branch is
                        // unreachable for H2. The H1 branch retains the 04.x
                        // detect-and-bail: TlsAcceptingHandler is hard-coded to
                        // `Arc<TcpProxy>` (per the inherent-generic `handle::<S>`
                        // design in `tls_handler.rs`), so wrapping HCM in TLS
                        // requires generalizing the adapter, which is deliberately
                        // deferred to phase 05+ per SPEC §3 D4.
                        if matches!(
                            hcm_cfg.codec_type,
                            envoy_config::CodecType::AUTO | envoy_config::CodecType::HTTP1
                        ) && build_downstream_tls_for_listener(listener_cfg)?.is_some()
                        {
                            anyhow::bail!(
                                "HCM listener with downstream TLS is not supported in phase 04.x; \
                         TlsAcceptingHandler is currently TcpProxy-only and will be \
                         generalized in phase 05+ (SPEC §3 D4)",
                            );
                        }
                        // Defensive symmetric bail for H2+TLS. The envoy-config validator
                        // (Http2OverTlsNotSupported, Task 2) rejects this combination at
                        // parse time, so this branch is unreachable from any well-formed
                        // config. Keep the runtime check anyway so a validator regression
                        // surfaces as a clean config-load error rather than a silently-
                        // non-functional plaintext listener on a port the operator
                        // expected to be TLS-protected.
                        if matches!(hcm_cfg.codec_type, envoy_config::CodecType::HTTP2)
                            && build_downstream_tls_for_listener(listener_cfg)?.is_some()
                        {
                            anyhow::bail!(
                                "TLS+HTTP2 listener is unsupported in phase 05.2; the \
                         envoy-config validator's Http2OverTlsNotSupported should \
                         have rejected this combination at parse time"
                            );
                        }
                        // 67.1 D5: the chain runs on the raw TcpStream before any
                        // codec is selected; HCM is just another terminal handler.
                        let handler: std::sync::Arc<dyn envoy_listener::ConnectionHandler> =
                            wrap_in_chain(chain_filters, hcm);

                        bind_and_spawn_listener(
                            listener_cfg,
                            handler,
                            &registry,
                            listener_concurrency,
                            "HCM",
                            bind_addr,
                            || {
                                tracing::info!(
                                    addr = %bind_addr,
                                    stat_prefix = %hcm_cfg.stat_prefix,
                                    codec_type = ?hcm_cfg.codec_type,
                                    "envoy-rust listening (http_connection_manager)",
                                )
                            },
                            &token,
                            &drain,
                            &mut set,
                        )
                        .await?;
                    }
                }
                other => {
                    anyhow::bail!(
                        "unsupported terminal network filter '{other}'; envoy-config should have rejected at parse time"
                    );
                }
            }
        } else {
            // ADR-0130 §2 / SPEC R-7: `filters: []`. Upstream Envoy accepts this
            // config and starts; envoy-rust used to panic on it. Bind no data
            // listener, warn, and let the admin listener (spawned below) serve.
            // What upstream does with a CONNECTION to such a listener was never
            // probed, so envoy-rust asserts nothing about it — CF-67-5.
            tracing::warn!(
                listener = %listener_cfg.name,
                "filter chain is empty; binding no data listener (upstream Envoy accepts \
                 this config and starts — see CF-67-5)",
            );
        }
    }

    // 26 Task 3: spawn the `RdsWatcher` AFTER the listeners/HCMs are
    // constructed (the target list is now populated). Cancellation wired to
    // the existing signal token so SIGTERM/SIGINT terminates every watch loop
    // at its next `tokio::select!` boundary. Inert (zero watch tasks) when
    // `rds_targets` is empty — the §5.2 inertness invariant. `spawn` is
    // infallible (the skeleton registers no counters and its `reload` is a
    // no-op stub; the real reparse+revalidate+store is Task 4, the `rds.*`
    // counter ticks are Task 5). Drained via `shutdown().await` below.
    // 26 Task 6: capture the live, swappable route-table sources (keyed by
    // `rds.route_config_name`) BEFORE `RdsWatcher::spawn` consumes `rds_targets`.
    // These share the same `Arc<HCMConfig>` swap-owners the watcher holds, so
    // `/config_dump`'s RoutesConfigDump renders the HOT-RELOADED table. Built
    // here (not lazily) so the Vec outlives the move of `rds_targets`.
    let live_route_configs: Vec<(String, std::sync::Arc<envoy_http1::HCMConfig>)> = rds_targets
        .iter()
        .map(|t| (t.route_config_name.clone(), std::sync::Arc::clone(&t.store)))
        .collect();

    let rds_watcher = envoy_http1::RdsWatcher::spawn(rds_targets, token.clone());

    // 27 Task 4 (D3/D4, §6.2-LOCKED / ADR-0068): spawn the EDS endpoint-file
    // watcher beside the RDS watcher, over the SAME generic `XdsFileWatcher`
    // core. The target list is built IN-CRATE by `build_eds_watch_targets`,
    // which walks `cluster_mgr`, filters for PLAIN EDS clusters (EDS-with-a-
    // file-path AND no active HC / outlier detection — Decision-5), and bundles
    // each target's reload closure + the 5 retained `update_*` counter handles.
    // This sidesteps the envoy-bin→envoy-cluster encapsulation wall (envoy-bin
    // cannot reach `ClusterHandle.inner` nor the plainness fields). Inert (zero
    // watch tasks) when no plain EDS cluster exists — the §5.2 invariant.
    // Drained via `shutdown().await` below alongside the other primitives.
    let eds_targets = envoy_cluster::build_eds_watch_targets(&cluster_mgr);
    let eds_watcher = envoy_cluster::XdsFileWatcher::spawn(eds_targets, token.clone());

    if let Some(admin_cfg) = bootstrap.admin.as_ref() {
        let admin_config = std::sync::Arc::new(
            envoy_admin::AdminConfig::from_envoy_config(admin_cfg)
                .with_context(|| "building AdminConfig")?,
        );
        let lst = TcpListener::bind(admin_config.address)
            .await
            .with_context(|| format!("binding admin listener to {}", admin_config.address))?;
        let addr = lst.local_addr().unwrap_or(admin_config.address);
        tracing::info!(%addr, "envoy-rust listening (admin)");
        let admin_handler = std::sync::Arc::new(envoy_admin::AdminHandler::new(
            std::sync::Arc::clone(&admin_config),
            std::sync::Arc::clone(&registry),
            std::sync::Arc::clone(&bootstrap),
            std::sync::Arc::clone(&cluster_mgr),
            start_instant,
            command_line_options.clone(),
            std::sync::Arc::clone(&drain),
            // 26 Task 6: hand the live route-table sources to the admin handler
            // so `/config_dump` reflects hot reloads (not the startup snapshot).
            live_route_configs,
        ));
        let shutdown = token.clone();
        set.spawn(async move {
            envoy_admin::serve(
                lst,
                admin_handler,
                async move { shutdown.cancelled().await },
            )
            .await
            .map_err(anyhow::Error::from)
        });
    }

    // Collect the first error from the listener/admin task set without
    // short-circuiting the scheduler drain. We capture the first error and
    // propagate it AFTER `health_scheduler.shutdown().await` so the probe
    // tasks always drain cleanly on BOTH clean-exit and error-exit paths.
    let mut first_err: Option<anyhow::Error> = None;
    while let Some(res) = set.join_next().await {
        let outcome = res.context("task panicked").and_then(|inner| inner);
        if let Err(e) = outcome
            && first_err.is_none()
        {
            first_err = Some(e);
        }
    }
    // 12.2 (parent-12 D4): drain the active-HC scheduler on BOTH clean-exit
    // and error-exit paths. The signal token cancellation has already
    // propagated through `tokio::select!` exits in the per-(cluster, endpoint)
    // `probe_loop`s; `shutdown().await` joins every JoinHandle before
    // envoy-bin returns (clean tokio task drain).
    health_scheduler.shutdown().await;
    // 14.2 D7 (lock-in #11): drain the outlier-ejection sweepers alongside the health
    // scheduler on BOTH clean-exit and error-exit paths. The signal token cancellation has
    // already propagated through each sweeper's `tokio::select!`; `shutdown().await` cancels
    // its token clone + joins every sweeper task before `cluster_mgr`'s stats handles drop.
    outlier_mgr.shutdown().await;
    // 26 Task 3: drain the rds watcher alongside the health scheduler +
    // outlier manager on BOTH clean-exit and error-exit paths. The signal
    // token cancellation has already propagated through each watch loop's
    // `tokio::select!`; `shutdown().await` cancels its token clone + joins
    // every watch task before envoy-bin returns. Inert (immediate) when no
    // rds target was configured.
    rds_watcher.shutdown().await;
    // 27 Task 4: drain the EDS endpoint-file watcher alongside the RDS watcher on
    // BOTH clean-exit and error-exit paths. The signal token cancellation has
    // already propagated through each watch loop's `tokio::select!`;
    // `shutdown().await` cancels its token clone + joins every watch task before
    // envoy-bin returns. Inert (immediate) when no plain EDS cluster was
    // configured.
    eds_watcher.shutdown().await;
    if let Some(e) = first_err {
        return Err(e);
    }
    tracing::info!("envoy-rust exited cleanly");
    Ok(())
}

/// 03.2: per-cluster `Arc<UpstreamTls>` map, keyed by cluster name. The
/// validator already rejected DownstreamTlsContext on a cluster's
/// transport_socket (MismatchedTransportSocketDirection { side: "cluster" }),
/// so the Downstream(_) match arm below is unreachable in practice but
/// kept defensively (parity with the listener-side fallthrough).
fn build_upstream_tls_map(
    bootstrap: &envoy_config::Bootstrap,
) -> Result<std::collections::HashMap<String, std::sync::Arc<envoy_tls::UpstreamTls>>> {
    let mut upstream_tls_by_cluster: std::collections::HashMap<
        String,
        std::sync::Arc<envoy_tls::UpstreamTls>,
    > = std::collections::HashMap::new();
    for cluster in bootstrap.all_clusters() {
        let Some(socket) = cluster.transport_socket.as_ref() else {
            continue;
        };
        match &socket.typed_config {
            envoy_config::TransportSocketTypedConfig::Upstream(ctx) => {
                let upstream_tls =
                    std::sync::Arc::new(envoy_tls::UpstreamTls::from_context(ctx).with_context(
                        || format!("building UpstreamTls for cluster {:?}", cluster.name),
                    )?);
                upstream_tls_by_cluster.insert(cluster.name.clone(), upstream_tls);
            }
            envoy_config::TransportSocketTypedConfig::Downstream(_) => {
                anyhow::bail!(
                    "cluster {:?} has DownstreamTlsContext (validator should have rejected)",
                    cluster.name
                );
            }
        }
    }
    Ok(upstream_tls_by_cluster)
}

/// Shared bind→log→spawn tail of the tcp_proxy and (shared-runtime) HCM
/// dispatch arms: bind the listener with SO_REUSEPORT concurrency, emit the
/// per-arm "listening" log line, and spawn `listener.serve(...)` onto the
/// main JoinSet wired to the signal token + drain state.
///
/// `kind` is the per-arm label in the bind error context ("tcp_proxy" /
/// "HCM"); `log_listening` is a per-arm closure so each arm's
/// `tracing::info!` call — with its own fields — stays byte-identical to the
/// pre-extraction messages.
/// 67.1 D5: split a network filter chain into its NON-TERMINAL prefix and its
/// TERMINAL last filter.
///
/// Before 67.1, `main` read `filters.first()` and ignored the rest — safe ONLY
/// because phase 66's terminal validation made every ≥2-filter chain invalid.
/// `envoy.filters.network.rbac` is the first NON-terminal filter, so that
/// interlock is gone: the terminal filter is the LAST one, and everything before
/// it is a filter to run per-connection.
///
/// `envoy-config`'s `NetworkFilterChainNotTerminated` rule (67.1 D2) guarantees
/// a NON-EMPTY chain ends in a terminal filter. An EMPTY `filters: []` chain is
/// ACCEPTED (SPEC R-7, upstream parity, closes M66-5) and has no terminal filter
/// at all — hence the `Option`. Returning `None` is NOT an error; see the caller.
fn split_chain(
    chain: &envoy_config::FilterChain,
) -> Option<(
    Vec<&envoy_config::NetworkFilter>,
    &envoy_config::NetworkFilter,
)> {
    let (terminal, prefix) = chain.filters.split_last()?;
    Some((prefix.iter().collect(), terminal))
}

/// 67.1 D5: construct one `envoy_listener::NetworkFilter` per non-terminal
/// filter, in configured order, registering its stats.
///
/// `envoy.filters.network.rbac` is the only non-terminal filter envoy-rust
/// supports today. The fallback arm is unreachable: the config validator's
/// per-filter match rejects every unknown name with
/// `ConfigError::UnsupportedFilter`, and every OTHER known name is terminal (so
/// it would be the chain's last filter and never appear in the prefix).
///
/// NOTE the two `NetworkFilter`s: `envoy_config::NetworkFilter` is the config
/// STRUCT; `envoy_listener::NetworkFilter` is the runtime TRAIT. Both appear
/// here. Always fully qualify.
fn build_network_filter_chain(
    prefix: &[&envoy_config::NetworkFilter],
    registry: &envoy_stats::StatsRegistry,
) -> Result<Vec<std::sync::Arc<dyn envoy_listener::NetworkFilter>>> {
    let mut out: Vec<std::sync::Arc<dyn envoy_listener::NetworkFilter>> =
        Vec::with_capacity(prefix.len());
    for filter in prefix {
        match filter.name.as_str() {
            envoy_config::NETWORK_RBAC_FILTER => {
                let Some(envoy_config::TypedConfig::NetworkRbac(cfg)) =
                    filter.typed_config.as_ref()
                else {
                    anyhow::bail!(
                        "validator guarantees a NetworkRbac typed_config on {}",
                        envoy_config::NETWORK_RBAC_FILTER
                    );
                };
                out.push(std::sync::Arc::new(
                    network_rbac::NetworkRbacFilter::new(cfg, registry)
                        .with_context(|| format!("registering stats for {}", cfg.stat_prefix))?,
                ));
            }
            other => anyhow::bail!(
                "non-terminal network filter '{other}' is not supported; \
                 the envoy-config validator should have rejected it at parse time",
            ),
        }
    }
    Ok(out)
}

/// 67.1 D5: wrap `inner` in the chain's non-terminal filters, if any. An empty
/// prefix returns `inner` untouched, so a lone-terminal-filter chain pays no
/// per-connection cost (no `peer_addr()`/`local_addr()` syscalls).
fn wrap_in_chain(
    filters: Vec<std::sync::Arc<dyn envoy_listener::NetworkFilter>>,
    inner: std::sync::Arc<dyn envoy_listener::ConnectionHandler>,
) -> std::sync::Arc<dyn envoy_listener::ConnectionHandler> {
    if filters.is_empty() {
        inner
    } else {
        std::sync::Arc::new(envoy_listener::ChainHandler::new(filters, inner))
    }
}

#[allow(clippy::too_many_arguments)] // mechanical extraction of a duplicated block; all args were in-scope locals
async fn bind_and_spawn_listener(
    listener_cfg: &envoy_config::Listener,
    handler: std::sync::Arc<dyn envoy_listener::ConnectionHandler>,
    registry: &std::sync::Arc<envoy_stats::StatsRegistry>,
    listener_concurrency: usize,
    kind: &str,
    bind_addr: SocketAddr,
    log_listening: impl FnOnce(),
    token: &tokio_util::sync::CancellationToken,
    drain: &std::sync::Arc<envoy_listener::DrainState>,
    set: &mut tokio::task::JoinSet<Result<()>>,
) -> Result<()> {
    let listener = envoy_listener::Listener::bind_with_concurrency(
        listener_cfg,
        handler,
        std::sync::Arc::clone(registry),
        listener_concurrency,
    )
    .await
    .with_context(|| format!("binding {kind} listener to {bind_addr}"))?;
    log_listening();
    let shutdown = token.clone();
    let drain_for_listener = std::sync::Arc::clone(drain);
    set.spawn(async move {
        listener
            .serve(
                async move { shutdown.cancelled().await },
                drain_for_listener,
            )
            .await
            .map_err(|e| anyhow::anyhow!(e))
    });
    Ok(())
}

/// 03.2 three-way downstream-TLS construction, factored at 04.1 task 11 so
/// both the TcpProxy and HCM dispatch arms share one implementation.
///
///   plaintext   -> `Ok(None)` (no chain has `transport_socket`).
///   single-cert -> `DownstreamTls::from_context` (03.1 path; fixtures 0001-0004).
///   multi-cert  -> `DownstreamTls::from_listener` (03.2 path; fixture 0006).
///
/// The validator already rejects the wrong direction (`UpstreamTlsContext`
/// on a listener) and the wrong name (anything not
/// `envoy.transport_sockets.tls`), so the `Upstream(...)` arm below is
/// unreachable in practice but kept defensively.
fn build_downstream_tls_for_listener(
    listener_cfg: &envoy_config::Listener,
) -> Result<Option<std::sync::Arc<envoy_tls::DownstreamTls>>> {
    let any_chain_has_tls = listener_cfg
        .filter_chains
        .iter()
        .any(|c| c.transport_socket.is_some());
    let any_chain_has_server_names = listener_cfg.filter_chains.iter().any(|c| {
        c.filter_chain_match
            .as_ref()
            .map(|m| !m.server_names.is_empty())
            .unwrap_or(false)
    });

    if !any_chain_has_tls {
        Ok(None)
    } else if listener_cfg.filter_chains.len() == 1 && !any_chain_has_server_names {
        let chain = &listener_cfg.filter_chains[0];
        let socket = chain
            .transport_socket
            .as_ref()
            .expect("any_chain_has_tls implies Some on the single chain");
        let envoy_config::TransportSocketTypedConfig::Downstream(ctx) = &socket.typed_config else {
            anyhow::bail!("validator should have rejected upstream transport_socket on listener",);
        };
        Ok(Some(std::sync::Arc::new(
            envoy_tls::DownstreamTls::from_context(ctx)
                .context("building DownstreamTls from single-chain context")?,
        )))
    } else {
        Ok(Some(std::sync::Arc::new(
            envoy_tls::DownstreamTls::from_listener(listener_cfg)
                .context("building DownstreamTls from multi-chain listener")?,
        )))
    }
}

async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM");
    let mut intr = signal(SignalKind::interrupt()).expect("install SIGINT");
    tokio::select! {
        _ = term.recv() => tracing::info!("SIGTERM received"),
        _ = intr.recv() => tracing::info!("SIGINT received"),
    }
}

#[cfg(test)]
mod chain_tests {
    use super::*;

    fn chain_from(yaml: &str) -> envoy_config::FilterChain {
        serde_yaml::from_str(yaml).expect("FilterChain parses")
    }

    const RBAC_ECHO: &str = "filters:\n  - name: envoy.filters.network.rbac\n    typed_config:\n      \"@type\": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC\n      stat_prefix: sp\n  - name: envoy.filters.network.echo\n";

    /// 67.1 D5: `[rbac, echo]` splits into one non-terminal filter + a terminal
    /// `echo`. The terminal filter is the LAST one — never `filters.first()`.
    #[test]
    fn splits_chain_into_non_terminal_prefix_and_terminal_last() {
        let chain = chain_from(RBAC_ECHO);
        let (prefix, terminal) = split_chain(&chain).expect("non-empty chain has a terminal");
        assert_eq!(prefix.len(), 1);
        assert_eq!(prefix[0].name, envoy_config::NETWORK_RBAC_FILTER);
        assert_eq!(terminal.name, envoy_config::ECHO_FILTER);
    }

    /// 67.1 D5: a lone terminal filter yields an EMPTY prefix — the pre-67.1 shape.
    #[test]
    fn lone_terminal_filter_yields_empty_prefix() {
        let chain = chain_from("filters:\n  - name: envoy.filters.network.echo\n");
        let (prefix, terminal) = split_chain(&chain).expect("terminal present");
        assert!(prefix.is_empty());
        assert_eq!(terminal.name, envoy_config::ECHO_FILTER);
    }

    /// 67.1 D5 / SPEC R-7 / ADR-0130 §2: an EMPTY chain has no terminal filter.
    /// `split_chain` returns None; the caller must NOT panic. envoy-rust used to
    /// crash here (`main.rs:220`, `validator guarantees ≥1 filter`) on a config
    /// upstream Envoy ACCEPTS and STARTS.
    #[test]
    fn empty_chain_has_no_terminal_and_does_not_panic() {
        let chain = chain_from("filters: []\n");
        assert!(split_chain(&chain).is_none());
    }

    /// 67.1 D5: `build_network_filter_chain` constructs one `NetworkFilter` per
    /// non-terminal filter and registers its counters.
    #[test]
    fn builds_a_network_rbac_filter_from_the_prefix() {
        let chain = chain_from(
            "filters:\n  - name: envoy.filters.network.rbac\n    typed_config:\n      \"@type\": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC\n      stat_prefix: built\n      rules:\n        action: DENY\n        policies:\n          p0:\n            permissions: [{ any: true }]\n            principals: [{ any: true }]\n  - name: envoy.filters.network.echo\n",
        );
        let registry = envoy_stats::StatsRegistry::new();
        let (prefix, _) = split_chain(&chain).unwrap();
        let filters = build_network_filter_chain(&prefix, &registry).expect("builds");
        assert_eq!(filters.len(), 1);
        // Counters registered at construction, at 0.
        assert_eq!(
            registry
                .register_counter("built.rbac.denied")
                .unwrap()
                .value(),
            0
        );
        assert_eq!(
            registry
                .register_counter("built.rbac.shadow_allowed")
                .unwrap()
                .value(),
            0
        );
    }

    /// 67.1 D5: an empty prefix returns `inner` untouched — a lone-terminal-filter
    /// chain pays no per-connection cost (no peer_addr()/local_addr() syscalls).
    #[test]
    fn wrap_in_chain_with_no_filters_returns_inner_unchanged() {
        let inner: std::sync::Arc<dyn envoy_listener::ConnectionHandler> =
            std::sync::Arc::new(echo::EchoHandler);
        let same = wrap_in_chain(vec![], std::sync::Arc::clone(&inner));
        assert!(
            std::sync::Arc::ptr_eq(&inner, &same),
            "empty prefix must not allocate a ChainHandler"
        );
    }
}
