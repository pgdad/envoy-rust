#![forbid(unsafe_code)]

//! Differential test harness for envoy-rust. Phase 01 surface: TCP echo + HTTP
//! admin GET.
//!
//! Contract: `run_fixture(fixture_dir)` starts upstream Envoy (via
//! testcontainers) and envoy-rust (via subprocess) against the fixture's paired
//! configs, then dispatches on `expectations.yaml`'s tagged `driver:` —
//! `tcp_echo` drives `inputs/payload.bin` via `drive_tcp`; `http_get` issues
//! a minimal `GET` via `drive_http_get`. Equivalence rules from `expectations`
//! are enforced by `assert_equivalence` (status-exact and/or byte-exact body).

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub mod subject;
pub mod upstream;

/// Contents of `<fixture>/expectations.yaml`. See SPEC §D5.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Expectations {
    pub driver: Driver,
    #[serde(default)]
    pub equivalence: Equivalence,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Driver {
    TcpEcho,
    HttpGet { path: String, host: String },
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Equivalence {
    #[serde(default)]
    pub response_status: Option<StatusRule>,
    #[serde(default)]
    pub response_body: Option<BodyRule>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StatusRule {
    Exact,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BodyRule {
    ByteExact,
}

pub fn load_expectations(path: &Path) -> Result<Expectations> {
    let yaml =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: Expectations =
        serde_yaml::from_str(&yaml).with_context(|| format!("parsing {}", path.display()))?;
    Ok(parsed)
}

/// Reserve a free TCP port on 127.0.0.1. Binds `:0`, reads the assigned port,
/// drops the listener, and returns the number.
///
/// TOCTOU: between the drop and the subsequent bind by envoy-rust, another
/// process on the host could grab this port. This is accepted for a
/// pre-production harness per SPEC §6 point 6. If CI flakes materialize, this
/// becomes its own split phase with a port-range reservation strategy.
pub fn reserve_port() -> Result<u16> {
    let listener =
        StdTcpListener::bind(("127.0.0.1", 0)).context("binding 127.0.0.1:0 to reserve a port")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Template-render a fixture YAML by substituting literal `{{KEY}}` tokens.
/// The `kvs` list is the set of tokens to replace; any `{{…}}` token not in
/// `kvs` is left untouched so a typo surfaces as a parser error rather than
/// silently rendering to the empty string.
pub fn render_yaml(template: &str, kvs: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in kvs {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

/// Write `content` to a new temp file in `dir` and return the path. The caller
/// is responsible for ensuring `dir` is already created.
pub fn write_temp(dir: &Path, name: &str, content: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    let mut f =
        std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
    f.write_all(content.as_bytes())?;
    Ok(path)
}

/// Poll `addr` with exponential backoff (starting at 50ms, doubling, capped at
/// 500ms) until a TCP connect succeeds or `budget` elapses. Returns `Err` on
/// timeout.
pub async fn wait_accept_ready(addr: std::net::SocketAddr, budget: Duration) -> Result<()> {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match tokio::net::TcpStream::connect(addr).await {
            Ok(_) => return Ok(()),
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(err) => bail!("{addr} not accept-ready within {budget:?}: {err}"),
        }
    }
}

/// Drive `payload` at `addr`: open TCP, write payload, read exactly
/// `payload.len()` bytes of echoed response, then confirm the peer writes
/// no further bytes before shutting down the write side and dropping the
/// stream. Returns the echoed bytes.
///
/// Why `read_exact(payload.len())` instead of half-close + `read_to_end`: see
/// `docs/envoy-rust/DECISIONS.md` ADR-0006. Upstream Envoy v1.33.0's default
/// `ConnectionImpl` (enable_half_close_=false) translates a client FIN into
/// `PostIoAction::Close` and calls `closeSocket(RemoteClose)` before the echo
/// filter's queued write is flushed, so a pre-read half-close causes the
/// response bytes to be dropped. Phase 00's only fixture (echo filter) has a
/// deterministic 1:1 byte-count contract, so `read_exact(payload.len())` is
/// both sufficient and matches upstream Envoy's own echo integration test
/// pattern. Graceful write-side shutdown still fires after the read so the
/// envoy-rust subject's echo loop exits on FIN rather than a peer reset.
///
/// Why the trailing-byte poll: see ADR-0007. A bare `read_exact(payload.len())`
/// silently ignores any bytes the peer writes after the echo, which would
/// narrow BEHAVIOR_CONTRACT row 2's "byte-exact" assertion to "first N bytes
/// match." After `read_exact`, we poll the socket with a short deadline
/// (100ms) and bail if the peer delivers more data before EOF or the
/// deadline — a peer that follows the echo-filter contract closes its
/// write side cleanly and we observe `Ok(0)` or a timeout.
pub async fn drive_tcp(addr: SocketAddr, payload: &[u8]) -> Result<Vec<u8>> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    stream.write_all(payload).await?;
    let mut out = vec![0u8; payload.len()];
    stream.read_exact(&mut out).await?;

    // ADR-0007: detect trailing bytes past the echoed payload. A compliant
    // peer either closes (Ok(0)) or stays silent until the deadline (timeout
    // Err). Any non-zero read is a contract violation.
    let mut tail = [0u8; 64];
    match tokio::time::timeout(Duration::from_millis(100), stream.read(&mut tail)).await {
        Ok(Ok(0)) | Err(_) => {}
        Ok(Ok(n)) => bail!("{addr} sent {n} trailing bytes after echo"),
        Ok(Err(e)) => bail!("{addr} read error after echo: {e}"),
    }

    stream.shutdown().await.ok();
    drop(stream);
    Ok(out)
}

/// Decoded HTTP/1.1 response. Headers are captured for debug tracing but play
/// no part in the phase-01 equivalence diff (ADR-0011).
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    #[allow(dead_code)]
    pub headers: Vec<(String, Vec<u8>)>,
}

/// Open a TCP connection to `addr`, issue a minimal `GET` for `path` with
/// `Host: host`, and parse the response. Supports `content-length`-framed and
/// `connection: close`-framed responses only; that is enough for phase 01's
/// admin surface (SPEC §6 signpost 9).
pub async fn drive_http_get(addr: SocketAddr, path: &str, host: &str) -> Result<HttpResponse> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Connection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await?;
    stream.flush().await.ok();

    let mut buf = Vec::with_capacity(2048);
    let mut scratch = [0u8; 2048];
    let head_end;
    loop {
        let n = stream.read(&mut scratch).await?;
        if n == 0 {
            bail!("{addr} closed before a response head was received");
        }
        buf.extend_from_slice(&scratch[..n]);

        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut resp = httparse::Response::new(&mut headers);
        match resp.parse(&buf) {
            Ok(httparse::Status::Complete(n)) => {
                head_end = n;
                let status = resp
                    .code
                    .ok_or_else(|| anyhow::anyhow!("missing response status code"))?;
                let mut captured_headers: Vec<(String, Vec<u8>)> = Vec::new();
                let mut content_length: Option<usize> = None;
                let mut connection_close = false;
                for h in resp.headers.iter() {
                    captured_headers.push((h.name.to_ascii_lowercase(), h.value.to_vec()));
                    if h.name.eq_ignore_ascii_case("content-length") {
                        let s = std::str::from_utf8(h.value)?.trim();
                        content_length = Some(s.parse()?);
                    } else if h.name.eq_ignore_ascii_case("connection") {
                        let s = std::str::from_utf8(h.value)?.trim();
                        if s.eq_ignore_ascii_case("close") {
                            connection_close = true;
                        }
                    }
                }

                // Drain the body.
                let body = match content_length {
                    Some(cl) => {
                        let mut body = Vec::with_capacity(cl);
                        let already = &buf[head_end..];
                        let take = already.len().min(cl);
                        body.extend_from_slice(&already[..take]);
                        if body.len() < cl {
                            let remaining = cl - body.len();
                            let mut rest = vec![0u8; remaining];
                            stream.read_exact(&mut rest).await?;
                            body.extend(rest);
                        }
                        body
                    }
                    None if connection_close => {
                        let mut body = Vec::new();
                        body.extend_from_slice(&buf[head_end..]);
                        stream.read_to_end(&mut body).await?;
                        body
                    }
                    None => bail!(
                        "{addr} response has neither `content-length` nor `connection: close`; \
                         drive_http_get does not support keep-alive in phase 01",
                    ),
                };

                return Ok(HttpResponse {
                    status,
                    body,
                    headers: captured_headers,
                });
            }
            Ok(httparse::Status::Partial) => continue,
            Err(e) => bail!("{addr} response parse error: {e}"),
        }
    }
}

/// End-to-end run of one fixture. Panics-on-failure paths unwind through Drop
/// guards so the container and envoy-rust subprocess are cleaned up even on
/// assertion failure.
pub async fn run_fixture(fixture_dir: &Path) -> Result<()> {
    let expectations = load_expectations(&fixture_dir.join("expectations.yaml"))?;

    // Reserve one port; use the driver-specific token to substitute into the
    // rendered configs. Upstream Envoy runs inside the container namespace and
    // listens on upstream::CONTAINER_PORT; envoy-rust listens on the host's
    // reserved port.
    let host_port = reserve_port()?;

    let tmp = tempfile::tempdir().context("creating fixture temp dir")?;
    let upstream_template = std::fs::read_to_string(fixture_dir.join("envoy.yaml"))
        .context("reading upstream envoy.yaml")?;
    let subject_template = std::fs::read_to_string(fixture_dir.join("envoy-rust.yaml"))
        .context("reading envoy-rust.yaml")?;

    let upstream_port_str = upstream::CONTAINER_PORT.to_string();
    let subject_port_str = host_port.to_string();
    let port_key = match &expectations.driver {
        Driver::TcpEcho => "PORT",
        Driver::HttpGet { .. } => "ADMIN_PORT",
    };
    let upstream_yaml = render_yaml(&upstream_template, &[(port_key, &upstream_port_str)]);
    let subject_yaml = render_yaml(&subject_template, &[(port_key, &subject_port_str)]);
    let upstream_path = write_temp(tmp.path(), "envoy.yaml", &upstream_yaml)?;
    let subject_path = write_temp(tmp.path(), "envoy-rust.yaml", &subject_yaml)?;

    let upstream = upstream::start(&upstream_path).await?;
    let mut subject = subject::start(&subject_path, host_port).await?;

    let upstream_addr: SocketAddr = format!("127.0.0.1:{}", upstream.host_port()).parse()?;
    let subject_addr: SocketAddr = format!("127.0.0.1:{}", subject.port()).parse()?;

    let budget = Duration::from_secs(10);
    wait_accept_ready(upstream_addr, budget)
        .await
        .context("upstream Envoy never became accept-ready")?;
    wait_accept_ready(subject_addr, budget)
        .await
        .context("envoy-rust never became accept-ready")?;

    match &expectations.driver {
        Driver::TcpEcho => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
            let upstream_out = drive_tcp(upstream_addr, &payload)
                .await
                .context("upstream envoy drive")?;
            let subject_out = drive_tcp(subject_addr, &payload)
                .await
                .context("envoy-rust drive")?;
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
            assert_equivalence(
                &expectations,
                /* upstream status */ None,
                /* subject status  */ None,
                &upstream_out,
                &subject_out,
            )?;
        }
        Driver::HttpGet { path, host } => {
            let upstream_resp = drive_http_get(upstream_addr, path, host)
                .await
                .context("upstream envoy http get")?;
            let subject_resp = drive_http_get(subject_addr, path, host)
                .await
                .context("envoy-rust http get")?;
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
            assert_equivalence(
                &expectations,
                Some(upstream_resp.status),
                Some(subject_resp.status),
                &upstream_resp.body,
                &subject_resp.body,
            )?;
        }
    }

    Ok(())
}

fn assert_equivalence(
    expectations: &Expectations,
    upstream_status: Option<u16>,
    subject_status: Option<u16>,
    upstream_body: &[u8],
    subject_body: &[u8],
) -> Result<()> {
    if matches!(
        expectations.equivalence.response_status,
        Some(StatusRule::Exact)
    ) {
        match (upstream_status, subject_status) {
            (Some(u), Some(s)) if u == s => {}
            (u, s) => bail!(
                "response status mismatch under `response_status: exact`\n  \
                 upstream: {u:?}\n  subject:  {s:?}"
            ),
        }
    }
    if matches!(
        expectations.equivalence.response_body,
        Some(BodyRule::ByteExact)
    ) && upstream_body != subject_body
    {
        bail!(
            "byte-exact body mismatch\n  upstream: {upstream_body:?}\n  subject:  {subject_body:?}",
        );
    }
    // Neither rule configured → silently pass + log a warning (SPEC §D5).
    if expectations.equivalence.response_status.is_none()
        && expectations.equivalence.response_body.is_none()
    {
        tracing::warn!(
            "fixture has neither response_status nor response_body equivalence rule — running as a smoke test"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expectations_parse_byte_exact() {
        let yaml = "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: byte_exact\n";
        let e: Expectations = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert!(matches!(e.driver, Driver::TcpEcho));
    }

    #[test]
    fn expectations_reject_unknown_rule() {
        let yaml = "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: sorta_equal\n";
        let r = serde_yaml::from_str::<Expectations>(yaml);
        assert!(r.is_err());
    }

    // Regression for REVIEW.md M3: `#[serde(deny_unknown_fields)]` must reject
    // a typo'd or unexpected top-level key rather than silently dropping it.
    #[test]
    fn expectations_reject_unknown_field() {
        let yaml =
            "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: byte_exact\nfoo: bar\n";
        let err = serde_yaml::from_str::<Expectations>(yaml)
            .expect_err("must reject unknown top-level field");
        let msg = err.to_string();
        assert!(msg.contains("unknown field"), "unexpected: {msg}");
    }

    // Regression for REVIEW.md M3 at the nested `Equivalence` level.
    #[test]
    fn equivalence_reject_unknown_field() {
        let yaml =
            "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: byte_exact\n  extra: true\n";
        let err = serde_yaml::from_str::<Expectations>(yaml)
            .expect_err("must reject unknown nested field");
        let msg = err.to_string();
        assert!(msg.contains("unknown field"), "unexpected: {msg}");
    }

    #[test]
    fn render_yaml_substitutes_all_port_tokens() {
        let t = "a: {{PORT}}\nb: {{PORT}}\n";
        assert_eq!(render_yaml(t, &[("PORT", "9000")]), "a: 9000\nb: 9000\n");
    }

    #[test]
    fn render_yaml_substitutes_admin_port_key() {
        let t = "address: 127.0.0.1\nport: {{ADMIN_PORT}}\n";
        assert_eq!(
            render_yaml(t, &[("ADMIN_PORT", "9901")]),
            "address: 127.0.0.1\nport: 9901\n"
        );
    }

    #[test]
    fn reserve_port_returns_nonzero() {
        let p = reserve_port().unwrap();
        assert!(p > 0);
    }

    #[tokio::test]
    async fn wait_accept_ready_succeeds_for_listening_socket() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        wait_accept_ready(addr, Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn wait_accept_ready_times_out_for_closed_socket() {
        let listener = StdTcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        drop(listener);
        let result = wait_accept_ready(addr, Duration::from_millis(200)).await;
        assert!(result.is_err());
    }

    // Mirrors upstream Envoy v1.33.0's echo filter semantics per ADR-0006: the
    // server accepts one connection, reads `payload.len()` bytes, echoes them
    // back, and closes WITHOUT ever honoring a client half-close. A harness
    // that half-closed before reading (the pre-ADR-0006 `drive_tcp`) would
    // race against this close and see an empty response.
    #[tokio::test]
    async fn drive_tcp_round_trips_without_half_close() {
        use tokio::io::AsyncReadExt as _;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let payload: &'static [u8] = b"hello, envoy-rust\n";
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; payload.len()];
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
            // Drop without waiting for a client FIN — this is what upstream
            // Envoy's echo path does once it has written the response.
            drop(stream);
        });

        let echoed = drive_tcp(addr, payload).await.unwrap();
        assert_eq!(echoed, payload);
        server.await.unwrap();
    }

    #[test]
    fn expectations_parse_tcp_echo_driver() {
        let yaml = r#"
driver:
  kind: tcp_echo
equivalence:
  response_body: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert_eq!(e.equivalence.response_status, None);
    }

    #[test]
    fn expectations_parse_http_get_driver() {
        let yaml = r#"
driver:
  kind: http_get
  path: /ready
  host: envoy-rust-phase-01
equivalence:
  response_status: exact
  response_body: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        match e.driver {
            Driver::HttpGet { path, host } => {
                assert_eq!(path, "/ready");
                assert_eq!(host, "envoy-rust-phase-01");
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
        assert_eq!(e.equivalence.response_status, Some(StatusRule::Exact));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
    }

    #[test]
    fn expectations_reject_unknown_driver_kind() {
        let yaml = r#"
driver:
  kind: quantum_bogon
equivalence:
  response_body: byte_exact
"#;
        let r: Result<Expectations, _> = serde_yaml::from_str(yaml);
        assert!(r.is_err(), "quantum_bogon must not parse: {r:?}");
    }

    // Regression for REVIEW.md I1 (ADR-0007): a server that writes
    // `payload.len()` bytes and then additional trailing bytes must cause
    // `drive_tcp` to fail the fixture. Before ADR-0007's trailing-byte check,
    // `drive_tcp` silently consumed only the first `payload.len()` bytes and
    // returned Ok, narrowing BEHAVIOR_CONTRACT row 2's "byte-exact" contract
    // to "first N bytes match."
    #[tokio::test]
    async fn drive_tcp_rejects_trailing_bytes_after_echo() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let payload: &'static [u8] = b"hello, envoy-rust\n";
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; payload.len()];
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
            // Write extra trailing bytes beyond the echoed payload. A pre-
            // ADR-0007 `drive_tcp` would not notice these.
            stream.write_all(b"EXTRA").await.unwrap();
            // Hold the stream open long enough that the harness's trailing-
            // byte poll deadline sees the bytes rather than an early EOF.
            tokio::time::sleep(Duration::from_millis(250)).await;
            drop(stream);
        });

        let err = drive_tcp(addr, payload)
            .await
            .expect_err("drive_tcp must fail when the peer writes trailing bytes");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("trailing bytes"),
            "unexpected error message: {msg}",
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn drive_http_get_round_trips() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // Read the request (we don't parse — just drain until CRLFCRLF).
            let mut buf = [0u8; 512];
            let mut read = Vec::new();
            loop {
                let n = stream.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                read.extend_from_slice(&buf[..n]);
                if read.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\nconnection: close\r\n\r\nLIVE\n",
                )
                .await
                .unwrap();
            drop(stream);
        });

        let resp = drive_http_get(addr, "/ready", "x").await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"LIVE\n");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn drive_http_get_handles_explicit_content_length() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let _ = tokio::io::copy(
                &mut tokio::io::empty(),
                &mut tokio::io::BufWriter::new(&mut s),
            )
            .await;
            s.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 4\r\n\r\nNOPE")
                .await
                .unwrap();
            // Hold open long enough for the client to read_exact the 4 bytes.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            drop(s);
        });

        let resp = drive_http_get(addr, "/x", "h").await.unwrap();
        assert_eq!(resp.status, 404);
        assert_eq!(resp.body, b"NOPE");
    }

    #[tokio::test]
    async fn drive_http_get_handles_connection_close_without_length() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            // Drain the incoming request so the receive buffer is empty before
            // we write and close. Without this, macOS sends RST instead of FIN
            // when dropping a TcpStream with unread data.
            let mut drain = [0u8; 512];
            loop {
                let n = s.read(&mut drain).await.unwrap();
                if n == 0 {
                    break;
                }
                if drain[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            s.write_all(b"HTTP/1.1 200 OK\r\nconnection: close\r\n\r\nhello-close")
                .await
                .unwrap();
            s.shutdown().await.ok();
            drop(s);
        });

        let resp = drive_http_get(addr, "/x", "h").await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"hello-close");
    }

    #[tokio::test]
    async fn drive_http_get_rejects_malformed_response() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            s.write_all(b"this is not a valid http response\r\n\r\n")
                .await
                .unwrap();
            drop(s);
        });

        let err = drive_http_get(addr, "/x", "h")
            .await
            .expect_err("malformed must fail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("parse") || msg.contains("invalid"),
            "got: {msg}"
        );
    }

    #[test]
    fn fixture_0001_expectations_parses_as_tcp_echo() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests/fixtures/0001-tcp-echo/expectations.yaml");
        let e = load_expectations(&path).expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert_eq!(e.equivalence.response_status, None);
    }
}
