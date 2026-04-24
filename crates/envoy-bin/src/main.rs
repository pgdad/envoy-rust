#![forbid(unsafe_code)]

use std::net::SocketAddr;

use anyhow::{Context, Result};
use tokio::net::TcpListener;

mod admin;
mod argv;
mod echo;

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

    if let Some(listener_cfg) = bootstrap.static_resources.listeners.first() {
        let sock = &listener_cfg.address.socket_address;
        let addr: SocketAddr = format!("{}:{}", sock.address, sock.port_value)
            .parse()
            .with_context(|| format!("parsing address {}:{}", sock.address, sock.port_value))?;
        let lst = TcpListener::bind(addr)
            .await
            .with_context(|| format!("binding echo listener to {addr}"))?;
        tracing::info!(%addr, "envoy-rust listening (echo)");
        let shutdown = token.clone();
        set.spawn(async move { echo::serve(lst, async move { shutdown.cancelled().await }).await });
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
