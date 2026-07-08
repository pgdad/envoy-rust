#![forbid(unsafe_code)]

//! `http1-echo-server` — minimal HTTP/1.1 echo server for the
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
use helper_common::ArgvError;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio::time::timeout;

const DRAIN_BUDGET: Duration = Duration::from_secs(5);

/// Per-read deadline for header + body reads. Bounds straggler clients without
/// slowing the harness; same value as DRAIN_BUDGET because both are "stop
/// waiting eventually" budgets, not load-bearing tuning.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Parsed argv surface. (`--port <u16>` required; `--body-marker <s>` optional.)
#[derive(Debug, PartialEq)]
struct Args {
    port: u16,
    /// 27 Task 6 (D6 / §6.2-LOCKED V2): an optional per-instance body marker.
    /// When set, the echo body carries a leading `backend: <marker>\n` line so
    /// two otherwise-identical echo backends are distinguishable by their
    /// response body — the EDS-reload discriminating observable. `None` for all
    /// pre-phase-27 fixtures (the body shape is byte-identical to before).
    body_marker: Option<String>,
}

/// The per-binary phrase in the `ArgvError::Trailing` message.
const TRAILING_AFTER: &str = "--port <u16>";

/// Parses argv (excluding argv[0]). The `--help`/`--version`/`--port`
/// skeleton lives in `helper_common`; the closure handles this binary's
/// `--body-marker` flag.
fn parse_argv(args: &[String]) -> Result<Args, ArgvError> {
    let mut body_marker: Option<String> = None;
    let port = helper_common::parse_port_argv(args, TRAILING_AFTER, |args, i| {
        if args[*i] == "--body-marker" {
            let v = args.get(*i + 1).ok_or(ArgvError::MissingValue)?;
            body_marker = Some(v.clone());
            *i += 2;
            Ok(true)
        } else {
            Ok(false)
        }
    })?;
    Ok(Args { port, body_marker })
}

fn print_help() {
    println!(
        "http1-echo-server: HTTP/1.1 echo server helper for the envoy-rust differential harness.\n\
         \n\
         Usage:\n  http1-echo-server --port <u16> [--body-marker <s>]\n  \
         http1-echo-server --help\n  http1-echo-server --version"
    );
}

async fn run(args: Args) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", args.port)).await?;
    tracing::info!("http1-echo-server listening on 0.0.0.0:{}", args.port);

    // 27 Task 6: the per-instance body marker (if any). Shared with each spawned
    // connection task so the echo body identifies which backend served the
    // request (the EDS-reload discriminating observable).
    let body_marker = std::sync::Arc::new(args.body_marker);

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
                        join_set.spawn(handle_connection(stream, body_marker.clone()));
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

async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    body_marker: std::sync::Arc<Option<String>>,
) {
    // Read the request bytes (single request per connection;
    // no keep-alive — see SPEC §6 signpost 9).
    let mut buf = bytes::BytesMut::with_capacity(8192);
    loop {
        let mut chunk = [0u8; 4096];
        let n = match timeout(READ_TIMEOUT, stream.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => n,
            Ok(Err(_)) | Err(_) => return,
        };
        buf.extend_from_slice(&chunk[..n]);
        match envoy_http1::Http1Codec::parse_request(&buf) {
            Ok(Some(req)) => {
                let body_len = req
                    .headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.parse::<usize>().ok())
                    .unwrap_or(0);
                let headers_end = req.bytes_consumed;
                let mut body: Vec<u8> = Vec::with_capacity(body_len);
                if buf.len() > headers_end {
                    let take = (buf.len() - headers_end).min(body_len);
                    body.extend_from_slice(&buf[headers_end..headers_end + take]);
                }
                while body.len() < body_len {
                    let mut chunk = [0u8; 4096];
                    let n = match timeout(READ_TIMEOUT, stream.read(&mut chunk)).await {
                        Ok(Ok(0)) => return,
                        Ok(Ok(n)) => n,
                        Ok(Err(_)) | Err(_) => return,
                    };
                    let need = body_len - body.len();
                    body.extend_from_slice(&chunk[..n.min(need)]);
                }

                let echo = build_echo_body(&req, &body, body_marker.as_deref());
                let resp = envoy_http1::Response {
                    status: 200,
                    reason: None,
                    headers: vec![
                        ("content-type".to_string(), "text/plain".to_string()),
                        ("content-length".to_string(), echo.len().to_string()),
                        ("connection".to_string(), "close".to_string()),
                    ],
                    body: bytes::Bytes::from(echo),
                };
                let _ = envoy_http1::Http1Response::write_to(&resp, &mut stream).await;
                let _ = stream.shutdown().await;
                return;
            }
            Ok(None) => continue,
            Err(_) => return,
        }
    }
}

/// Build the deterministic echo body per SPEC §3 D3:
///
/// ```text
/// method: <METHOD>
/// path: <PATH>
/// headers:
///   <name1>: <value1>     (alphabetically sorted by lowercase name)
///   ...
/// body: <BODY>
/// ```
///
/// The alphabetic header sort is LOAD-BEARING: both proxies forward the
/// request to the SAME helper, but Envoy may emit headers in a different
/// order than envoy-rust. Sorting by lowercase name eliminates this
/// source of divergence so byte-exact body equality holds across both
/// proxies' downstream responses (which are then proxied back to the
/// harness verbatim per the router proxy arm in Task 9).
fn build_echo_body(req: &envoy_http1::Request, body: &[u8], body_marker: Option<&str>) -> Vec<u8> {
    let mut out = String::new();
    // 27 Task 6 (D6 / §6.2-LOCKED V2): when a per-instance marker is set, emit
    // it as a leading `backend: <marker>\n` line so a `GET /probe` response
    // identifies WHICH backend served it (the EDS-reload discriminating
    // observable). `None` ⇒ no line — the body is byte-identical to the pre-27
    // SPEC §3 D3 shape, so all existing fixtures' differential equality holds.
    if let Some(marker) = body_marker {
        out.push_str("backend: ");
        out.push_str(marker);
        out.push('\n');
    }
    out.push_str("method: ");
    out.push_str(&req.method);
    out.push('\n');
    out.push_str("path: ");
    out.push_str(&req.path);
    out.push('\n');
    out.push_str("headers:\n");
    let mut sorted_headers: Vec<(String, String)> = req
        .headers
        .iter()
        .map(|(n, v)| (n.to_ascii_lowercase(), v.clone()))
        .collect();
    sorted_headers.sort_by(|a, b| a.0.cmp(&b.0));
    for (n, v) in &sorted_headers {
        out.push_str("  ");
        out.push_str(n);
        out.push_str(": ");
        out.push_str(v);
        out.push('\n');
    }
    out.push_str("body: ");
    // UTF-8 if possible; else replace each byte with `?`.
    match std::str::from_utf8(body) {
        Ok(s) => out.push_str(s),
        Err(_) => {
            for _ in body {
                out.push('?');
            }
        }
    }
    out.push('\n');
    out.into_bytes()
}

fn main() -> ExitCode {
    helper_common::init_tracing("info", true);

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

    helper_common::run_blocking(run(args), "", "runtime error: ")
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
        // No `--body-marker` ⇒ marker is None (the default-shape echo body,
        // unchanged for all pre-phase-27 fixtures).
        assert_eq!(args.body_marker, None);
    }

    #[test]
    fn argv_parses_body_marker() {
        // 27 Task 6 (D6 / §6.2-LOCKED V2): a per-instance body marker makes two
        // otherwise-identical echo backends distinguishable by their response
        // body (the EDS-reload discriminating observable — `[backend_1]` →
        // `[backend_2]` is a real endpoint swap only if the two backends differ).
        let args =
            parse_argv(&argv(&["--port", "10042", "--body-marker", "backend_1"])).expect("parse");
        assert_eq!(args.port, 10042);
        assert_eq!(args.body_marker.as_deref(), Some("backend_1"));
    }

    #[test]
    fn body_marker_prepends_a_marker_line() {
        // The marker is emitted as a leading `backend: <marker>\n` line so a
        // `GET /probe` response identifies WHICH backend served it. The rest of
        // the SPEC §3 D3 deterministic shape is preserved verbatim after it.
        let req = envoy_http1::Request {
            method: "GET".to_string(),
            path: "/probe".to_string(),
            version: envoy_http1::HttpVersion::Http11,
            headers: vec![("host".to_string(), "x.test".to_string())],
            bytes_consumed: 0,
            body: None,
        };
        let with = build_echo_body(&req, b"", Some("backend_2"));
        let body = String::from_utf8(with).unwrap();
        assert!(
            body.starts_with("backend: backend_2\n"),
            "marker must be the leading line: {body}"
        );
        assert!(
            body.contains("method: GET\npath: /probe\n"),
            "the deterministic D3 shape follows the marker line: {body}"
        );
        // No marker ⇒ no `backend:` line (byte-identical to the pre-27 body).
        let without = build_echo_body(&req, b"", None);
        let body0 = String::from_utf8(without).unwrap();
        assert!(
            !body0.contains("backend:"),
            "no `--body-marker` ⇒ no `backend:` line: {body0}"
        );
        assert!(body0.starts_with("method: GET\n"), "pre-27 shape: {body0}");
    }

    #[test]
    fn argv_rejects_missing_port() {
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

    #[tokio::test(flavor = "multi_thread")]
    async fn accepts_and_echoes_request() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Reserve a port (race-y but matches helper conventions).
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let p = l.local_addr().unwrap().port();
            drop(l);
            p
        };

        // Spawn the runtime in a background task.
        let server_handle = tokio::spawn(async move {
            let _ = run(Args {
                port,
                body_marker: None,
            })
            .await;
        });

        // Wait for the listener.
        for _ in 0..50 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Open a TCP connection and write an HTTP/1.1 GET. Use Connection: close
        // so the server closes after the response (matches fixture 0008's wire).
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        s.write_all(b"GET / HTTP/1.1\r\nHost: x.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();

        // Read the full response.
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf);

        // Assert the response shape. The body is deterministic per SPEC §3 D3.
        assert!(
            response.starts_with("HTTP/1.1 200 OK\r\n"),
            "status: {response}"
        );
        assert!(
            response.contains("content-type: text/plain\r\n"),
            "ct: {response}"
        );
        // The body has the SPEC §3 D3 shape:
        //   method: GET\npath: /\nheaders:\n  content-length: 0\n  host: x.test\nbody: \n
        let expected_body =
            "method: GET\npath: /\nheaders:\n  content-length: 0\n  host: x.test\nbody: \n";
        assert!(
            response.ends_with(expected_body),
            "body shape:\nactual:\n{response}\nexpected suffix:\n{expected_body}"
        );

        server_handle.abort();
    }
}
