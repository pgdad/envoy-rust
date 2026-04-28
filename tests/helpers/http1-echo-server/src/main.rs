#![forbid(unsafe_code)]

//! `http1-echo-server` — minimal localhost-only HTTP/1.1 echo server for the
//! envoy-rust differential harness. Sibling of `tcp-echo-server` (phase 02.1)
//! and `tls-echo-server` (phase 03.2). Plaintext only — no TLS.
//!
//! The deterministic-echo response body shape is LOAD-BEARING for differential
//! equivalence (per SPEC §3 D3): the helper produces a `200 OK` response with
//! `Content-Type: text/plain` and a body of:
//!
//! ```text
//! method: <METHOD>
//! path: <PATH>
//! headers:
//!   <name1>: <value1>     (alphabetically sorted by lowercase name)
//!   <name2>: <value2>
//!   ...
//! body: <BODY>
//! ```
//!
//! Both proxies forward the same request to the same helper; the alphabetic
//! header sort eliminates ordering divergences from differential body comparison.

use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use thiserror::Error;

const DRAIN_BUDGET: Duration = Duration::from_secs(5);

/// Parsed argv surface. (`--port <u16>` only; no TLS keys.)
#[derive(Debug, PartialEq)]
struct Args {
    port: u16,
}

#[derive(Debug, Error, PartialEq)]
enum ArgvError {
    #[error("required flag {0} missing")]
    MissingFlag(&'static str),
    #[error("flag expects a value")]
    MissingValue,
    #[error("port value must be a u16")]
    InvalidPort,
    #[error("trailing arguments after --port <u16>")]
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
    while i < args.len() {
        match args[i].as_str() {
            "--help" => return Err(ArgvError::HelpRequested),
            "--version" => return Err(ArgvError::VersionRequested),
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
    })
}

fn print_help() {
    println!(
        "http1-echo-server: HTTP/1.1 echo server helper for the envoy-rust differential harness.\n\
         \n\
         Usage:\n  http1-echo-server --port <u16>\n  \
         http1-echo-server --help\n  http1-echo-server --version"
    );
}

async fn run(_args: Args) -> Result<()> {
    // Task 12 lands the accept loop.
    let _ = DRAIN_BUDGET;
    Ok(())
}

fn main() -> ExitCode {
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
            print_help();
            return ExitCode::from(0);
        }
        Err(ArgvError::VersionRequested) => {
            println!("http1-echo-server {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::from(0);
        }
        Err(e) => {
            eprintln!("argv error: {e}");
            return ExitCode::from(2);
        }
    };

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to build tokio runtime: {e}");
            return ExitCode::from(1);
        }
    };

    match rt.block_on(run(args)) {
        Ok(()) => ExitCode::from(0),
        Err(e) => {
            eprintln!("runtime error: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn argv_parses_full_invocation() {
        let args = parse_argv(&argv(&["--port", "10042"])).expect("parse");
        assert_eq!(args.port, 10042);
    }

    #[test]
    fn argv_rejects_missing_port() {
        // No --port arg → MissingFlag("--port").
        let result = parse_argv(&argv(&[]));
        assert_eq!(result, Err(ArgvError::MissingFlag("--port")));
    }

    #[test]
    fn argv_rejects_invalid_port() {
        let result = parse_argv(&argv(&["--port", "not-a-number"]));
        assert_eq!(result, Err(ArgvError::InvalidPort));
    }

    #[test]
    fn argv_shows_help() {
        assert_eq!(
            parse_argv(&argv(&["--help"])),
            Err(ArgvError::HelpRequested)
        );
    }
}
