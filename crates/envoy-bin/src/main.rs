#![forbid(unsafe_code)]

use std::net::SocketAddr;

use anyhow::{Context, Result};
use tokio::net::TcpListener;

mod argv;
mod echo;
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
    // and reuse across all per-connection invocations of `handle`. The
    // validator already rejected DownstreamTlsContext on a cluster's
    // transport_socket (MismatchedTransportSocketDirection { side: "cluster" }),
    // so the Downstream(_) match arm below is unreachable in practice but
    // kept defensively (parity with the listener-side fallthrough).
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

    // 26 Task 3: the 5th periodic-background primitive — the `RdsWatcher`.
    // Built by walking the listeners for HCMs with `rds` configured (the §5.2
    // inertness invariant: empty target list ⇒ zero watch tasks when no HCM
    // uses rds). Populated in the HCM dispatch arm below — it needs the
    // post-construction `Arc<HCMConfig>` (the Task-2 swappable route-table
    // handle) plus the rds file path/name — then spawned AFTER the
    // listener-serve block and drained via `shutdown().await` on the runtime
    // drain path, mirroring the 12.2 health scheduler / 14.2 outlier manager.
    let mut rds_targets: Vec<envoy_http1::WatchTarget> = Vec::new();

    if let Some(listener_cfg) = bootstrap.all_listeners().next() {
        // The validator guarantees `filter_chains.len() ≥ 1` and at least one
        // filter; we read the single first filter (phase 02.2 supports one
        // filter per chain). Phase 07's filter chain framework will iterate.
        let filter = listener_cfg
            .filter_chains
            .first()
            .and_then(|c| c.filters.first())
            .expect("validator guarantees ≥1 filter");

        let sock = &listener_cfg.address.socket_address;
        let bind_addr: SocketAddr = format!("{}:{}", sock.address, sock.port_value)
            .parse()
            .with_context(|| {
                format!(
                    "parsing listener address {}:{}",
                    sock.address, sock.port_value
                )
            })?;

        match filter.name.as_str() {
            envoy_config::ECHO_FILTER => {
                let lst = TcpListener::bind(bind_addr)
                    .await
                    .with_context(|| format!("binding echo listener to {bind_addr}"))?;
                tracing::info!(addr = %bind_addr, "envoy-rust listening (echo)");
                let shutdown = token.clone();
                set.spawn(async move {
                    echo::serve(lst, async move { shutdown.cancelled().await }).await
                });
            }
            envoy_config::TCP_PROXY_FILTER => {
                let Some(envoy_config::TypedConfig::TcpProxy(tp_cfg)) =
                    filter.typed_config.as_ref()
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

                let listener = envoy_listener::Listener::bind(
                    listener_cfg,
                    handler,
                    std::sync::Arc::clone(&registry),
                )
                .await
                .with_context(|| format!("binding tcp_proxy listener to {bind_addr}"))?;
                tracing::info!(addr = %bind_addr, cluster = %tp_cfg.cluster, "envoy-rust listening (tcp_proxy)");
                let shutdown = token.clone();
                let drain_for_listener = std::sync::Arc::clone(&drain);
                set.spawn(async move {
                    listener
                        .serve(
                            async move { shutdown.cancelled().await },
                            drain_for_listener,
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                });
            }
            envoy_config::HCM_FILTER => {
                let Some(envoy_config::TypedConfig::HttpConnectionManager(hcm_cfg)) =
                    filter.typed_config.as_ref()
                else {
                    anyhow::bail!(
                        "filter '{}' missing typed_config; envoy-config validator should have rejected at parse time",
                        envoy_config::HCM_FILTER,
                    );
                };

                let hcm_config = std::sync::Arc::new(
                    envoy_http1::HCMConfig::from_config(
                        hcm_cfg,
                        std::sync::Arc::clone(&cluster_mgr),
                        std::sync::Arc::clone(&registry),
                        Some(std::sync::Arc::clone(&pool_mgr)),
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
                        path: std::path::PathBuf::from(&rds.config_source.path_config_source.path),
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
                let handler: std::sync::Arc<dyn envoy_listener::ConnectionHandler> = hcm;

                let listener = envoy_listener::Listener::bind(
                    listener_cfg,
                    handler,
                    std::sync::Arc::clone(&registry),
                )
                .await
                .with_context(|| format!("binding HCM listener to {bind_addr}"))?;
                tracing::info!(
                    addr = %bind_addr,
                    stat_prefix = %hcm_cfg.stat_prefix,
                    codec_type = ?hcm_cfg.codec_type,
                    "envoy-rust listening (http_connection_manager)",
                );
                let shutdown = token.clone();
                let drain_for_listener = std::sync::Arc::clone(&drain);
                set.spawn(async move {
                    listener
                        .serve(
                            async move { shutdown.cancelled().await },
                            drain_for_listener,
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                });
            }
            other => {
                anyhow::bail!(
                    "filter '{other}' is not dispatchable; envoy-config should have rejected at parse time"
                );
            }
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
    if let Some(e) = first_err {
        return Err(e);
    }
    tracing::info!("envoy-rust exited cleanly");
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
