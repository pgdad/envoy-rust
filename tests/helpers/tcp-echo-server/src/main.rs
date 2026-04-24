#![forbid(unsafe_code)]

//! `tcp-echo-server` — a minimal localhost-only echo server for the envoy-rust
//! differential harness. See SPEC §D3 of phase 02.1.

use thiserror::Error;

/// Parsed argv surface.
#[allow(dead_code)] // used in Task 10's main()
#[derive(Debug, PartialEq)]
struct Args {
    port: u16,
}

/// argv parse failure modes.
///
/// `HelpRequested` and `VersionRequested` are "successful" user intents that
/// nevertheless short-circuit the parse — `main` translates them to exit 0.
#[allow(dead_code)] // used in Task 10's main()
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
#[allow(dead_code)] // used in Task 10's main()
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

fn main() {
    // Populated in Task 10.
    unimplemented!("tcp-echo-server runtime lands in Task 10");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn argv_parses_port() {
        let got = parse_argv(&argv(&["--port", "10042"])).expect("ok");
        assert_eq!(got, Args { port: 10042 });
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
}
