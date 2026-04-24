#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::net::TcpListener;

mod echo;

#[allow(dead_code)]
mod admin;

#[derive(Debug, PartialEq, Eq)]
pub enum ArgvError {
    NoConfigFlag,
    UnknownFlag(String),
    MissingValue(String),
    Trailing(String),
}

impl std::fmt::Display for ArgvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoConfigFlag => write!(
                f,
                "expected exactly one of `-c <path>` or `--config-path <path>`",
            ),
            Self::UnknownFlag(flag) => write!(f, "unknown argument: {flag}"),
            Self::MissingValue(flag) => write!(f, "{flag} requires a path argument"),
            Self::Trailing(arg) => write!(f, "unexpected trailing argument: {arg}"),
        }
    }
}

impl std::error::Error for ArgvError {}

/// Phase 00 accepts exactly one flag: `-c <path>` or `--config-path <path>`.
/// `clap` is deliberately avoided (not on the D-3.2 permitted-foundations list).
/// When argv grows past a single path, land an ADR and revisit.
pub fn parse_argv<I, S>(args: I) -> Result<PathBuf, ArgvError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut iter = args.into_iter().map(Into::into);
    let _ = iter.next();
    let mut path: Option<PathBuf> = None;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-c" | "--config-path" => {
                let value = iter.next().ok_or(ArgvError::MissingValue(arg.clone()))?;
                if path.is_some() {
                    return Err(ArgvError::Trailing(value));
                }
                path = Some(PathBuf::from(value));
            }
            other => return Err(ArgvError::UnknownFlag(other.to_string())),
        }
    }
    path.ok_or(ArgvError::NoConfigFlag)
}

fn main() -> std::process::ExitCode {
    match parse_argv(std::env::args()) {
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
    let sock = &bootstrap.static_resources.listeners[0]
        .address
        .socket_address;
    let addr: SocketAddr = format!("{}:{}", sock.address, sock.port_value)
        .parse()
        .with_context(|| format!("parsing address {}:{}", sock.address, sock.port_value))?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding to {addr}"))?;
    tracing::info!(%addr, "envoy-rust listening");
    echo::serve(listener, shutdown_signal()).await?;
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

#[cfg(test)]
mod argv_tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        std::iter::once("envoy-bin")
            .chain(args.iter().copied())
            .map(ToOwned::to_owned)
            .collect()
    }

    #[test]
    fn accepts_short_flag() {
        let p = parse_argv(argv(&["-c", "/etc/envoy-rust.yaml"])).unwrap();
        assert_eq!(p, PathBuf::from("/etc/envoy-rust.yaml"));
    }

    #[test]
    fn accepts_long_flag() {
        let p = parse_argv(argv(&["--config-path", "/tmp/e.yaml"])).unwrap();
        assert_eq!(p, PathBuf::from("/tmp/e.yaml"));
    }

    #[test]
    fn rejects_missing_flag() {
        assert_eq!(parse_argv(argv(&[])), Err(ArgvError::NoConfigFlag));
    }

    #[test]
    fn rejects_missing_value() {
        assert_eq!(
            parse_argv(argv(&["-c"])),
            Err(ArgvError::MissingValue("-c".into())),
        );
    }

    #[test]
    fn rejects_unknown_flag() {
        assert_eq!(
            parse_argv(argv(&["--foo", "bar"])),
            Err(ArgvError::UnknownFlag("--foo".into())),
        );
    }

    #[test]
    fn rejects_duplicate_config_flag() {
        let err = parse_argv(argv(&["-c", "/a", "-c", "/b"])).unwrap_err();
        assert!(matches!(err, ArgvError::Trailing(_)), "got {err:?}");
    }
}
