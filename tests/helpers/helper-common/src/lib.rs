#![forbid(unsafe_code)]

//! `helper-common` — shared infrastructure for the test helper servers under
//! `tests/helpers/` (`tcp-echo-server`, `tls-echo-server`, `http1-echo-server`,
//! `http2-echo-server`).
//!
//! The helpers copy-pasted three pieces of scaffolding: the argv error enum +
//! `--help`/`--version`/`--port` parse skeleton, the tracing-subscriber init,
//! and the build-runtime/block_on/exit-code tail of `main`. This crate hosts
//! the shared shape; each binary keeps its OWN divergences byte-exactly:
//!
//! - per-binary flags (`--close-on-accept`, `--cert`/`--key`, `--body-marker`,
//!   `--close-before-response`) compose via the `extra` callback of
//!   [`parse_port_argv`];
//! - per-binary `Trailing` message text rides in the [`ArgvError::Trailing`]
//!   field;
//! - `http2-echo-server`'s tracing posture (default `"warn"`, stdout writer)
//!   vs the others' (default `"info"`, stderr writer) is parameterized in
//!   [`init_tracing`] — deliberately NOT normalized;
//! - the differing runtime-failure error prefixes are parameterized in
//!   [`run_blocking`].
//!
//! `health-aware-http1-backend` has a different posture (anyhow-based argv
//! errors, no signal handling) and deliberately does not consume this crate.
//! `tcp-echo-server` keeps its `#[tokio::main]` entrypoint (its runtime-build
//! failure path panics rather than printing; switching it to [`run_blocking`]
//! would change that surface).

use std::process::ExitCode;

use thiserror::Error;

/// argv parse failure modes.
///
/// `HelpRequested` and `VersionRequested` are "successful" user intents that
/// nevertheless short-circuit the parse — each helper's `main` translates
/// them to exit 0.
#[derive(Debug, Error, PartialEq)]
pub enum ArgvError {
    #[error("required flag {0} missing")]
    MissingFlag(&'static str),
    #[error("flag expects a value")]
    MissingValue,
    #[error("port value must be a u16")]
    InvalidPort,
    /// The field carries the per-binary "after ..." phrase (e.g.
    /// `"--port <u16>"` for the http helpers, `"--port <PORT>"` for
    /// tcp-echo-server, `"--key <PATH>"` for tls-echo-server) so every
    /// binary's original message is preserved byte-exactly.
    #[error("trailing arguments after {0}")]
    Trailing(&'static str),
    #[error("--help")]
    HelpRequested,
    #[error("--version")]
    VersionRequested,
}

/// The shared `--help`/`--version`/`--port` argv parse skeleton (argv[0]
/// excluded). Returns the parsed port on success.
///
/// `trailing_after` is the per-binary phrase embedded in
/// [`ArgvError::Trailing`] for unrecognized arguments.
///
/// Per-binary flags compose via `extra`: it is called with the full arg slice
/// and a mutable cursor positioned at an argument the skeleton does not
/// recognize. If the callback consumes the flag (and any value), it advances
/// the cursor past what it consumed and returns `Ok(true)`; `Ok(false)` means
/// "not mine", which the skeleton turns into `ArgvError::Trailing`.
pub fn parse_port_argv(
    args: &[String],
    trailing_after: &'static str,
    mut extra: impl FnMut(&[String], &mut usize) -> Result<bool, ArgvError>,
) -> Result<u16, ArgvError> {
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
            _ => {
                if !extra(args, &mut i)? {
                    return Err(ArgvError::Trailing(trailing_after));
                }
            }
        }
    }
    port.ok_or(ArgvError::MissingFlag("--port"))
}

/// Shared tracing-subscriber init: `RUST_LOG` (the default env) wins; else
/// `default_filter`. `to_stderr` selects the writer — `http2-echo-server`
/// keeps its historical stdout writer + `"warn"` default; the other helpers
/// use stderr + `"info"`. The divergence is parameterized, not normalized.
pub fn init_tracing(default_filter: &str, to_stderr: bool) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter));
    if to_stderr {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}

/// The blocking main/runtime/exit-code tail shared by `tls-echo-server`,
/// `http1-echo-server`, and `http2-echo-server`: build a multi-thread tokio
/// runtime, `block_on(fut)`, translate the outcome to an exit code.
///
/// The two prefixes preserve each binary's historical stderr text byte-exactly:
///
/// - tls/http1: `run_blocking(fut, "", "runtime error: ")` emits
///   `failed to build tokio runtime: {e}` / `runtime error: {e}`;
/// - http2: `run_blocking(fut, "error: ", "error: ")` emits
///   `error: failed to build tokio runtime: {e}` / `error: {e}`.
///
/// Exit codes: 0 on `Ok(())`, 1 on either failure (`ExitCode::FAILURE` and
/// `ExitCode::from(1)` are the same value).
pub fn run_blocking<E: std::fmt::Display>(
    fut: impl std::future::Future<Output = Result<(), E>>,
    build_err_prefix: &str,
    run_err_prefix: &str,
) -> ExitCode {
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("{build_err_prefix}failed to build tokio runtime: {e}");
            return ExitCode::from(1);
        }
    };
    match rt.block_on(fut) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{run_err_prefix}{e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| s.to_string()).collect()
    }

    /// An `extra` callback for binaries with no extra flags.
    fn no_extra(_args: &[String], _i: &mut usize) -> Result<bool, ArgvError> {
        Ok(false)
    }

    #[test]
    fn parses_port() {
        let port = parse_port_argv(&argv(&["--port", "10042"]), "--port <u16>", no_extra)
            .expect("parse ok");
        assert_eq!(port, 10042);
    }

    #[test]
    fn rejects_missing_port_flag() {
        let err = parse_port_argv(&argv(&[]), "--port <u16>", no_extra).expect_err("empty argv");
        assert_eq!(err, ArgvError::MissingFlag("--port"));
    }

    #[test]
    fn rejects_dangling_port_value() {
        let err =
            parse_port_argv(&argv(&["--port"]), "--port <u16>", no_extra).expect_err("dangling");
        assert_eq!(err, ArgvError::MissingValue);
    }

    #[test]
    fn rejects_non_numeric_port() {
        let err = parse_port_argv(&argv(&["--port", "abc"]), "--port <u16>", no_extra)
            .expect_err("non-numeric");
        assert_eq!(err, ArgvError::InvalidPort);
    }

    #[test]
    fn trailing_carries_the_per_binary_phrase() {
        let err = parse_port_argv(&argv(&["--port", "1", "--junk"]), "--key <PATH>", no_extra)
            .expect_err("trailing");
        assert_eq!(err, ArgvError::Trailing("--key <PATH>"));
        assert_eq!(err.to_string(), "trailing arguments after --key <PATH>");
    }

    #[test]
    fn help_and_version_short_circuit() {
        assert_eq!(
            parse_port_argv(&argv(&["--help"]), "--port <u16>", no_extra),
            Err(ArgvError::HelpRequested)
        );
        assert_eq!(
            parse_port_argv(&argv(&["--version"]), "--port <u16>", no_extra),
            Err(ArgvError::VersionRequested)
        );
    }

    #[test]
    fn extra_flags_compose() {
        // A boolean flag and a value-taking flag, as the real binaries use.
        let mut boolean = false;
        let mut value: Option<String> = None;
        let port = parse_port_argv(
            &argv(&["--bool", "--port", "7000", "--value", "x"]),
            "--port <u16>",
            |args, i| match args[*i].as_str() {
                "--bool" => {
                    boolean = true;
                    *i += 1;
                    Ok(true)
                }
                "--value" => {
                    let v = args.get(*i + 1).ok_or(ArgvError::MissingValue)?;
                    value = Some(v.clone());
                    *i += 2;
                    Ok(true)
                }
                _ => Ok(false),
            },
        )
        .expect("parse ok");
        assert_eq!(port, 7000);
        assert!(boolean);
        assert_eq!(value.as_deref(), Some("x"));
    }

    #[test]
    fn error_messages_are_byte_stable() {
        // These strings are part of each helper binary's CLI surface.
        assert_eq!(
            ArgvError::MissingFlag("--port").to_string(),
            "required flag --port missing"
        );
        assert_eq!(ArgvError::MissingValue.to_string(), "flag expects a value");
        assert_eq!(
            ArgvError::InvalidPort.to_string(),
            "port value must be a u16"
        );
        assert_eq!(ArgvError::HelpRequested.to_string(), "--help");
        assert_eq!(ArgvError::VersionRequested.to_string(), "--version");
    }
}
