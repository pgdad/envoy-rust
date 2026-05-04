#![forbid(unsafe_code)]

//! `http2-echo-server` — minimal HTTP/2 cleartext (H2C) echo server for the
//! envoy-rust differential harness. Sibling of `tcp-echo-server` (phase 02.1),
//! `tls-echo-server` (phase 03.2), and `http1-echo-server` (phase 04.3).
//! Plaintext H2C only — no TLS.
//!
//! Per parent-05 SPEC §6 signpost 7: the helper consumes `envoy_http2` (NOT
//! `h2` directly for the handshake). The accept loop's per-stream surface
//! still reaches `h2::server::Connection` types (via the wrapper's return
//! value); this carve-out is the documented helper-side direct-surface
//! parallel to `tests/differential`'s `drive_http2` consumption.
//!
//! The deterministic-echo response body shape is LOAD-BEARING for differential
//! equivalence (per SPEC §3 D5): the helper produces a `200 OK` response with
//! `content-type: text/plain` and a body of:
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
//! Both proxies forward the same logical request; the alphabetic header sort
//! eliminates ordering divergences from differential body comparison. Mirrors
//! `http1-echo-server`'s body shape exactly so cross-protocol fixtures (if a
//! later phase ships them) remain comparable.

use std::process::ExitCode;

use anyhow::Result;
use bytes::Bytes;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::task::JoinSet;

/// Parsed argv surface. Mirrors `http1-echo-server`'s `Args` shape verbatim.
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

/// Argv parser. Identical shape to `http1-echo-server::parse_argv` per parent
/// §6 signpost 7's "mirror the established helper posture verbatim".
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
        "http2-echo-server: HTTP/2 cleartext echo server helper for the envoy-rust differential harness.\n\
         \n\
         Usage:\n  http2-echo-server --port <u16>\n  \
         http2-echo-server --help\n  http2-echo-server --version"
    );
}

fn print_version() {
    println!("http2-echo-server {}", env!("CARGO_PKG_VERSION"));
}

async fn run(args: Args) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", args.port)).await?;
    tracing::info!("http2-echo-server listening on 0.0.0.0:{}", args.port);

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
                        join_set.spawn(handle_connection(stream));
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "accept failed; continuing");
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_connection(tcp: tokio::net::TcpStream) {
    let mut conn = match envoy_http2::codec::server_handshake(tcp).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "h2 handshake failed");
            return;
        }
    };
    while let Some(stream_result) = conn.accept().await {
        let (req, mut send_response) = match stream_result {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "h2 stream accept failed");
                return;
            }
        };
        tokio::spawn(async move {
            // Drain the request body bytes (small body assumption).
            let (parts, mut body) = req.into_parts();
            let mut body_bytes = bytes::BytesMut::new();
            while let Some(chunk_result) = body.data().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(error = %e, "h2 body read failed");
                        return;
                    }
                };
                body_bytes.extend_from_slice(&chunk);
                let _ = body.flow_control().release_capacity(chunk.len());
            }
            let response_body = make_response_body(&parts, &body_bytes);
            let response = http::Response::builder()
                .status(200)
                .header("content-type", "text/plain")
                .body(())
                .unwrap();
            let mut send_stream = match send_response.send_response(response, false) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "send_response failed");
                    return;
                }
            };
            if let Err(e) = send_stream.send_data(Bytes::from(response_body), true) {
                tracing::warn!(error = %e, "send_data failed");
            }
        });
    }
}

/// Build the deterministic-echo body. The body shape MUST match
/// `http1-echo-server::make_response`'s body shape exactly so cross-protocol
/// fixtures (if any) remain comparable. The alphabetic header sort is
/// LOAD-BEARING for differential equivalence (both proxies forward the same
/// logical request; the helper's sorted-header response is the byte-exact
/// baseline).
fn make_response_body(parts: &http::request::Parts, body_bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(256 + body_bytes.len());
    out.extend_from_slice(b"method: ");
    out.extend_from_slice(parts.method.as_str().as_bytes());
    out.push(b'\n');
    out.extend_from_slice(b"path: ");
    out.extend_from_slice(parts.uri.path().as_bytes());
    out.push(b'\n');
    out.extend_from_slice(b"headers:\n");
    let mut sorted_headers: Vec<(String, Vec<u8>)> = parts
        .headers
        .iter()
        .map(|(n, v)| (n.as_str().to_lowercase(), v.as_bytes().to_vec()))
        .collect();
    // Add the H2 pseudo-headers explicitly so the body shape includes them
    // (h2 codec strips them from the user-facing HeaderMap).
    sorted_headers.push((
        ":authority".to_string(),
        parts
            .uri
            .authority()
            .map(|a| a.as_str().as_bytes().to_vec())
            .unwrap_or_default(),
    ));
    sorted_headers.push((
        ":method".to_string(),
        parts.method.as_str().as_bytes().to_vec(),
    ));
    sorted_headers.push((":path".to_string(), parts.uri.path().as_bytes().to_vec()));
    sorted_headers.push((
        ":scheme".to_string(),
        parts.uri.scheme_str().unwrap_or("http").as_bytes().to_vec(),
    ));
    sorted_headers.sort_by(|a, b| a.0.cmp(&b.0));
    for (n, v) in &sorted_headers {
        out.extend_from_slice(b"  ");
        out.extend_from_slice(n.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(v);
        out.push(b'\n');
    }
    out.extend_from_slice(b"body: ");
    out.extend_from_slice(body_bytes);
    out
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_argv(&argv) {
        Ok(a) => a,
        Err(ArgvError::HelpRequested) => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Err(ArgvError::VersionRequested) => {
            print_version();
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}");
            print_help();
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to build tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_argv_accepts_port() {
        let args = parse_argv(&["--port".to_string(), "7000".to_string()]).unwrap();
        assert_eq!(args, Args { port: 7000 });
    }

    #[test]
    fn parse_argv_rejects_missing_port() {
        let err = parse_argv(&[]).unwrap_err();
        assert_eq!(err, ArgvError::MissingFlag("--port"));
    }

    #[test]
    fn parse_argv_help_returns_help_requested() {
        let err = parse_argv(&["--help".to_string()]).unwrap_err();
        assert_eq!(err, ArgvError::HelpRequested);
    }

    #[test]
    fn parse_argv_version_returns_version_requested() {
        let err = parse_argv(&["--version".to_string()]).unwrap_err();
        assert_eq!(err, ArgvError::VersionRequested);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn echo_round_trip_against_in_test_h2_client() {
        // Spawn the helper on an ephemeral 127.0.0.1 port; open an h2 client
        // connection; send GET /test with Host: testharness; assert the
        // response body shape.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _server_task = tokio::spawn(async move {
            if let Ok((tcp, _)) = listener.accept().await {
                handle_connection(tcp).await;
            }
        });
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://testharness/test")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        let (_parts, mut body) = resp.into_parts();
        let mut body_bytes = bytes::BytesMut::new();
        while let Some(chunk_result) = body.data().await {
            let chunk = chunk_result.unwrap();
            body_bytes.extend_from_slice(&chunk);
            let _ = body.flow_control().release_capacity(chunk.len());
        }
        let s = std::str::from_utf8(&body_bytes).unwrap();
        assert!(s.starts_with("method: GET\n"), "body shape: {s}");
        assert!(s.contains("path: /test\n"), "body shape: {s}");
        assert!(s.contains(":authority: testharness\n"), "body shape: {s}");
        assert!(s.contains(":scheme: http\n"), "body shape: {s}");
    }
}
