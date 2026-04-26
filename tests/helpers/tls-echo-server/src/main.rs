#![forbid(unsafe_code)]

//! `tls-echo-server` — a minimal localhost-only TLS echo server for the
//! envoy-rust differential harness. Sibling of `tcp-echo-server` (phase 02.1)
//! with rustls server-side termination on top. Single-cert ResolvesServerCert
//! (no SNI multiplexing — this helper is single-purpose). See SPEC §D7 of
//! phase 03.2.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
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
    cert: PathBuf,
    key: PathBuf,
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
    #[error("trailing arguments after --key <PATH>")]
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
    let mut cert: Option<PathBuf> = None;
    let mut key: Option<PathBuf> = None;
    while i < args.len() {
        match args[i].as_str() {
            "--help" => return Err(ArgvError::HelpRequested),
            "--version" => return Err(ArgvError::VersionRequested),
            "--port" => {
                let v = args.get(i + 1).ok_or(ArgvError::MissingValue)?;
                port = Some(v.parse().map_err(|_| ArgvError::InvalidPort)?);
                i += 2;
            }
            "--cert" => {
                let v = args.get(i + 1).ok_or(ArgvError::MissingValue)?;
                cert = Some(PathBuf::from(v));
                i += 2;
            }
            "--key" => {
                let v = args.get(i + 1).ok_or(ArgvError::MissingValue)?;
                key = Some(PathBuf::from(v));
                i += 2;
            }
            _ => return Err(ArgvError::Trailing),
        }
    }
    Ok(Args {
        port: port.ok_or(ArgvError::MissingFlag("--port"))?,
        cert: cert.ok_or(ArgvError::MissingFlag("--cert"))?,
        key: key.ok_or(ArgvError::MissingFlag("--key"))?,
    })
}

/// Runtime entrypoint: load PEMs, build server config, accept loop.
async fn run(args: Args) -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let cert_pem = std::fs::read(&args.cert)?;
    let key_pem = std::fs::read(&args.key)?;

    let cert_chain: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut std::io::BufReader::new(cert_pem.as_slice()))
            .collect::<Result<_, _>>()
            .map_err(|e| anyhow::anyhow!("parsing cert PEM: {e}"))?;
    let private_key_pkcs8 =
        rustls_pemfile::pkcs8_private_keys(&mut std::io::BufReader::new(key_pem.as_slice()))
            .next()
            .ok_or_else(|| anyhow::anyhow!("no PKCS#8 private key in {}", args.key.display()))?
            .map_err(|e| anyhow::anyhow!("parsing key PEM: {e}"))?;
    let private_key = rustls::pki_types::PrivateKeyDer::Pkcs8(private_key_pkcs8);

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .map_err(|e| anyhow::anyhow!("building server config: {e}"))?;
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind(("127.0.0.1", args.port)).await?;
    tracing::info!("tls-echo-server listening on 127.0.0.1:{}", args.port);

    let mut join_set: JoinSet<()> = JoinSet::new();
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("shutdown signal received");
                break;
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _)) => {
                        let acceptor = acceptor.clone();
                        join_set.spawn(async move {
                            match acceptor.accept(stream).await {
                                Ok(mut tls) => {
                                    let (mut r, mut w) = tokio::io::split(&mut tls);
                                    let _ = tokio::io::copy(&mut r, &mut w).await;
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "tls handshake failed; dropping");
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "accept failed; continuing");
                    }
                }
            }
        }
    }

    drop(listener);
    let drain = timeout(DRAIN_BUDGET, async {
        while join_set.join_next().await.is_some() {}
    });
    let _ = drain.await;
    join_set.abort_all();
    while join_set.join_next().await.is_some() {}

    Ok(())
}

fn print_help() {
    println!(
        "tls-echo-server: TLS echo server helper for the envoy-rust differential harness.\n\
         \n\
         Usage:\n  tls-echo-server --port <u16> --cert <path> --key <path>\n  \
         tls-echo-server --help\n  tls-echo-server --version"
    );
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
            println!("tls-echo-server {}", env!("CARGO_PKG_VERSION"));
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
        let args = parse_argv(&argv(&[
            "--port",
            "10042",
            "--cert",
            "/tmp/c.pem",
            "--key",
            "/tmp/k.pem",
        ]))
        .expect("parse");
        assert_eq!(args.port, 10042);
        assert_eq!(args.cert, PathBuf::from("/tmp/c.pem"));
        assert_eq!(args.key, PathBuf::from("/tmp/k.pem"));
    }

    #[test]
    fn argv_rejects_missing_cert() {
        let result = parse_argv(&argv(&["--port", "10042", "--key", "/tmp/k.pem"]));
        assert_eq!(result, Err(ArgvError::MissingFlag("--cert")));
    }

    #[test]
    fn argv_rejects_missing_key() {
        let result = parse_argv(&argv(&["--port", "10042", "--cert", "/tmp/c.pem"]));
        assert_eq!(result, Err(ArgvError::MissingFlag("--key")));
    }

    #[test]
    fn argv_shows_help() {
        assert_eq!(
            parse_argv(&argv(&["--help"])),
            Err(ArgvError::HelpRequested)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn accepts_and_echoes_via_tls() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        // rcgen-built CA + leaf with SAN "envoy-rust.test".
        let tmpdir = tempfile::tempdir().expect("tempdir");
        let ca_kp = rcgen::KeyPair::generate().expect("ca kp");
        let mut ca_params = rcgen::CertificateParams::new(vec![]).expect("ca params");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca self-signed");

        let leaf_kp = rcgen::KeyPair::generate().expect("leaf kp");
        let leaf_params =
            rcgen::CertificateParams::new(vec!["envoy-rust.test".into()]).expect("leaf params");
        let leaf_cert = leaf_params
            .signed_by(&leaf_kp, &ca_cert, &ca_kp)
            .expect("leaf signed");

        let cert_path = tmpdir.path().join("server.crt");
        let key_path = tmpdir.path().join("server.key");
        std::fs::write(&cert_path, leaf_cert.pem()).unwrap();
        std::fs::write(&key_path, leaf_kp.serialize_pem()).unwrap();

        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let p = l.local_addr().unwrap().port();
            drop(l);
            p
        };

        // Spawn the runtime in a background task.
        let args = Args {
            port,
            cert: cert_path.clone(),
            key: key_path.clone(),
        };
        let server_handle = tokio::spawn(async move {
            let _ = run(args).await;
        });

        // Wait for the listener.
        for _ in 0..50 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Build a TLS client trusting the test CA. Use rcgen's DER directly to
        // avoid round-tripping through PEM (mirrors Task 7's clean pattern).
        let mut roots = rustls::RootCertStore::empty();
        let ca_der: rustls::pki_types::CertificateDer<'static> = ca_cert.der().clone().into_owned();
        roots.add(ca_der).unwrap();
        let client_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
        let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let server_name = rustls::pki_types::ServerName::try_from("envoy-rust.test").unwrap();
        let mut tls = connector
            .connect(server_name, stream)
            .await
            .expect("handshake");

        let payload = b"hello, tls-echo-server\n";
        tls.write_all(payload).await.unwrap();
        let mut response = vec![0u8; payload.len()];
        tls.read_exact(&mut response).await.unwrap();
        assert_eq!(response, payload);

        drop(tls);
        server_handle.abort();
    }
}
