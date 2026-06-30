#![forbid(unsafe_code)]

//! `tcp-echo-server` — a minimal echo server for the envoy-rust
//! differential harness. See SPEC §D3 of phase 02.1.

use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio::time::timeout;

const DRAIN_BUDGET: Duration = Duration::from_secs(5);

/// Parsed argv surface.
#[derive(Debug, PartialEq)]
struct Args {
    port: u16,
    close_on_accept: bool,
}

/// argv parse failure modes.
///
/// `HelpRequested` and `VersionRequested` are "successful" user intents that
/// nevertheless short-circuit the parse — `main` translates them to exit 0.
#[derive(Debug, Error, PartialEq)]
enum ArgvError {
    #[error("required flag {0} missing")]
    MissingFlag(&'static str),
    #[error("flag expects a value")]
    MissingValue,
    #[error("port value must be a u16")]
    InvalidPort,
    #[error("trailing arguments after --port <PORT>")]
    Trailing,
    #[error("--help")]
    HelpRequested,
    #[error("--version")]
    VersionRequested,
}

/// Parses argv (excluding argv[0]).
fn parse_argv(args: &[String]) -> Result<Args, ArgvError> {
    let mut i = 0;
    let mut port: Option<u16> = None;
    let mut close_on_accept = false;
    while i < args.len() {
        match args[i].as_str() {
            "--help" => return Err(ArgvError::HelpRequested),
            "--version" => return Err(ArgvError::VersionRequested),
            "--close-on-accept" => {
                close_on_accept = true;
                i += 1;
            }
            "--port" => {
                let v = args.get(i + 1).ok_or(ArgvError::MissingValue)?;
                port = Some(v.parse().map_err(|_| ArgvError::InvalidPort)?);
                i += 2;
            }
            _ => return Err(ArgvError::Trailing),
        }
    }
    Ok(Args {
        port: port.ok_or(ArgvError::MissingFlag("--port"))?,
        close_on_accept,
    })
}

const USAGE: &str = "tcp-echo-server --port <PORT> [--close-on-accept]";
const VERSION: &str = concat!("tcp-echo-server ", env!("CARGO_PKG_VERSION"));

/// Accept loop on an already-bound listener. Returns when `shutdown` resolves
/// *and* the drain completes (or `DRAIN_BUDGET` expires, whichever first).
async fn run_on(
    listener: TcpListener,
    shutdown: impl std::future::Future<Output = ()>,
    close_on_accept: bool,
) -> Result<()> {
    let mut conns: JoinSet<()> = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("shutdown signal received; draining");
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((mut stream, peer)) => {
                        tracing::debug!(?peer, "accepted");
                        conns.spawn(async move {
                            if close_on_accept {
                                // Phase 53 (ADR-0110): accept-then-close upstream.
                                // The handshake has completed (post-connect); do ONE
                                // best-effort read to drain whatever the client sent
                                // (the request), THEN drop the stream — a graceful FIN
                                // with NO response. The read-before-close guarantees
                                // BOTH proxies classify this as a POST-connect reset
                                // (UC), never a pre-connect connect-failure (UF).
                                use tokio::io::AsyncReadExt;
                                let mut buf = [0u8; 1024];
                                let _ = stream.read(&mut buf).await;
                                drop(stream);
                            } else {
                                let (mut r, mut w) = stream.split();
                                let _ = tokio::io::copy(&mut r, &mut w).await;
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "accept error; sleeping 10ms");
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            }
        }
    }

    // Drain with a budget. `JoinSet::join_next` returns `None` when empty.
    let drained = timeout(DRAIN_BUDGET, async {
        while conns.join_next().await.is_some() {}
    })
    .await;
    if drained.is_err() {
        tracing::warn!(
            budget_ms = DRAIN_BUDGET.as_millis() as u64,
            "drain budget exceeded; aborting stragglers"
        );
        conns.abort_all();
        // Let aborted tasks finish unwinding; ignore result.
        while conns.join_next().await.is_some() {}
    }
    Ok(())
}

/// Full runtime entrypoint: bind → `run_on` with ctrl_c as shutdown.
async fn run(port: u16, close_on_accept: bool) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "tcp-echo-server listening on 0.0.0.0:{port}");
    run_on(
        listener,
        async {
            let _ = tokio::signal::ctrl_c().await;
        },
        close_on_accept,
    )
    .await
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_argv(&argv) {
        Ok(a) => a,
        Err(ArgvError::HelpRequested) => {
            eprintln!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(ArgvError::VersionRequested) => {
            eprintln!("{VERSION}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    match run(args.port, args.close_on_accept).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:?}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::oneshot;

    fn argv(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn argv_parses_port() {
        let got = parse_argv(&argv(&["--port", "10042"])).expect("ok");
        assert_eq!(
            got,
            Args {
                port: 10042,
                close_on_accept: false
            }
        );
    }

    #[test]
    fn argv_parses_close_on_accept() {
        let got = parse_argv(&argv(&["--port", "10042", "--close-on-accept"])).expect("ok");
        assert_eq!(
            got,
            Args {
                port: 10042,
                close_on_accept: true
            }
        );
    }

    #[test]
    fn argv_rejects_missing_port_flag() {
        let err = parse_argv(&argv(&[])).expect_err("empty argv");
        assert_eq!(err, ArgvError::MissingFlag("--port"));
    }

    #[test]
    fn argv_rejects_missing_value() {
        let err = parse_argv(&argv(&["--port"])).expect_err("dangling --port");
        assert_eq!(err, ArgvError::MissingValue);
    }

    #[test]
    fn argv_rejects_non_numeric_port() {
        let err = parse_argv(&argv(&["--port", "abc"])).expect_err("non-numeric");
        assert_eq!(err, ArgvError::InvalidPort);
    }

    #[test]
    fn argv_rejects_trailing_argument() {
        let err = parse_argv(&argv(&["--port", "10042", "--junk"])).expect_err("trailing");
        assert_eq!(err, ArgvError::Trailing);
    }

    #[test]
    fn argv_shows_help() {
        let err = parse_argv(&argv(&["--help"])).expect_err("help");
        assert_eq!(err, ArgvError::HelpRequested);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn echoes_round_trip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            run_on(
                listener,
                async move {
                    let _ = rx.await;
                },
                false,
            )
            .await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect");
        let payload: [u8; 32] = core::array::from_fn(|i| i as u8);
        client.write_all(&payload).await.expect("write");
        let mut buf = [0u8; 32];
        client.read_exact(&mut buf).await.expect("read_exact");
        assert_eq!(buf, payload);
        drop(client);

        tx.send(()).expect("signal shutdown");
        server.await.expect("join").expect("server Ok");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drain_exits_within_budget() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let (tx, rx) = oneshot::channel::<()>();
        let start = std::time::Instant::now();
        let server = tokio::spawn(async move {
            run_on(
                listener,
                async move {
                    let _ = rx.await;
                },
                false,
            )
            .await
        });

        // Open a stalled connection — connect, write one byte, read the echoed
        // byte, then STOP reading while keeping the stream alive. The server's
        // spawned task is parked in its copy loop waiting on more bytes.
        let mut client = TcpStream::connect(addr).await.expect("connect");
        client.write_all(&[42]).await.expect("write");
        let mut one = [0u8; 1];
        client.read_exact(&mut one).await.expect("read");

        // Fire shutdown; drop the client *after* to avoid triggering a clean
        // FIN that would let the server's copy loop complete on its own.
        tx.send(()).expect("signal shutdown");
        tokio::time::timeout(DRAIN_BUDGET + Duration::from_millis(500), server)
            .await
            .expect("server task resolved within DRAIN_BUDGET + ε")
            .expect("join")
            .expect("server Ok");
        let elapsed = start.elapsed();
        drop(client); // keep client alive until the assertion above

        assert!(
            elapsed <= DRAIN_BUDGET + Duration::from_millis(1_000),
            "drain took {elapsed:?}; expected ≤ {:?}",
            DRAIN_BUDGET + Duration::from_millis(1_000),
        );
    }
}
