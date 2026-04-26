#![forbid(unsafe_code)]

use std::net::SocketAddr;

use anyhow::{Context, Result};
use tokio::net::TcpListener;

mod admin;
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
    let bootstrap = envoy_config::parse_bootstrap(&yaml)?;

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

    // Build the cluster manager once. Empty `clusters` is permitted at the
    // envoy-config validator (admin-only configs); the manager is empty in
    // that case and `tcp_proxy` filters reference clusters by name, which
    // the validator already verified exist (`ConfigError::UnknownCluster`).
    let cluster_mgr = std::sync::Arc::new(
        envoy_cluster::from_bootstrap(&bootstrap).context("building cluster manager")?,
    );

    if let Some(listener_cfg) = bootstrap.static_resources.listeners.first() {
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
                let proxy = std::sync::Arc::new(envoy_tcp::TcpProxy::new(cluster, tp_cfg));

                // Per SPEC §3 D5: pre-pass the listener's first filter chain
                // for a downstream `transport_socket`. If present, wrap the
                // inner `Arc<TcpProxy>` in a `TlsAcceptingHandler`. The
                // validator already rejected the wrong direction
                // (UpstreamTlsContext on a listener) and the wrong name
                // (anything not `envoy.transport_sockets.tls`), so the
                // `Upstream(...)` arm and the `name != TLS_TRANSPORT_SOCKET`
                // case are unreachable here.
                let chain = listener_cfg
                    .filter_chains
                    .first()
                    .expect("validator guarantees ≥1 filter chain");
                let handler: std::sync::Arc<dyn envoy_listener::ConnectionHandler> = if let Some(
                    ts,
                ) =
                    chain.transport_socket.as_ref()
                {
                    let envoy_config::TransportSocketTypedConfig::Downstream(ctx) =
                        &ts.typed_config
                    else {
                        anyhow::bail!(
                            "validator should have rejected upstream transport_socket on listener",
                        );
                    };
                    let downstream_tls = std::sync::Arc::new(
                        envoy_tls::DownstreamTls::from_context(ctx)
                            .context("building DownstreamTls from listener transport_socket")?,
                    );
                    std::sync::Arc::new(tls_handler::TlsAcceptingHandler {
                        tls: downstream_tls,
                        inner: proxy,
                    })
                } else {
                    proxy
                };

                let listener = envoy_listener::Listener::bind(listener_cfg, handler)
                    .await
                    .with_context(|| format!("binding tcp_proxy listener to {bind_addr}"))?;
                tracing::info!(addr = %bind_addr, cluster = %tp_cfg.cluster, "envoy-rust listening (tcp_proxy)");
                let shutdown = token.clone();
                set.spawn(async move {
                    listener
                        .serve(async move { shutdown.cancelled().await })
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

    if let Some(admin_cfg) = bootstrap.admin.as_ref() {
        let sock = &admin_cfg.address.socket_address;
        let addr: SocketAddr = format!("{}:{}", sock.address, sock.port_value)
            .parse()
            .with_context(|| {
                format!("parsing admin address {}:{}", sock.address, sock.port_value)
            })?;
        let lst = TcpListener::bind(addr)
            .await
            .with_context(|| format!("binding admin listener to {addr}"))?;
        tracing::info!(%addr, "envoy-rust listening (admin)");
        let shutdown = token.clone();
        set.spawn(
            async move { admin::serve(lst, async move { shutdown.cancelled().await }).await },
        );
    }

    while let Some(res) = set.join_next().await {
        res.context("task panicked")??;
    }
    tracing::info!("envoy-rust exited cleanly");
    Ok(())
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
