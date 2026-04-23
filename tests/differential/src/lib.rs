#![forbid(unsafe_code)]

//! Differential test harness for envoy-rust. Phase 00 surface: TCP echo.
//!
//! Contract: `run_fixture(fixture_dir)` starts upstream Envoy (via
//! testcontainers) and envoy-rust (via subprocess) against the fixture's paired
//! configs, drives the fixture's `inputs/payload.bin` at both, and asserts the
//! responses are byte-exact equal per `expectations.yaml`.

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub mod subject;
pub mod upstream;

/// Contents of `<fixture>/expectations.yaml`.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Expectations {
    pub equivalence: Equivalence,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Equivalence {
    pub response_body: BodyRule,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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

/// Template-render a fixture YAML by substituting the literal `{{PORT}}` token.
pub fn render_yaml(template: &str, port: u16) -> String {
    template.replace("{{PORT}}", &port.to_string())
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

/// End-to-end run of one fixture. Panics-on-failure paths unwind through Drop
/// guards so the container and envoy-rust subprocess are cleaned up even on
/// assertion failure.
pub async fn run_fixture(fixture_dir: &Path) -> Result<()> {
    let expectations = load_expectations(&fixture_dir.join("expectations.yaml"))?;
    assert_eq!(
        expectations.equivalence.response_body,
        BodyRule::ByteExact,
        "phase 00 only understands response_body: byte_exact",
    );

    // Shared port number — upstream Envoy uses it inside the container's
    // namespace, envoy-rust binds it on the host.
    let host_port = reserve_port()?;

    // Render and materialize both configs in a temp directory.
    let tmp = tempfile::tempdir().context("creating fixture temp dir")?;
    let upstream_template = std::fs::read_to_string(fixture_dir.join("envoy.yaml"))
        .context("reading upstream envoy.yaml")?;
    let subject_template = std::fs::read_to_string(fixture_dir.join("envoy-rust.yaml"))
        .context("reading envoy-rust.yaml")?;
    let upstream_yaml = render_yaml(&upstream_template, upstream::CONTAINER_PORT);
    let subject_yaml = render_yaml(&subject_template, host_port);
    let upstream_path = write_temp(tmp.path(), "envoy.yaml", &upstream_yaml)?;
    let subject_path = write_temp(tmp.path(), "envoy-rust.yaml", &subject_yaml)?;

    // Start both proxies. Upstream first because it is slower to become ready.
    let upstream = upstream::start(&upstream_path).await?;
    let mut subject = subject::start(&subject_path, host_port).await?;

    let upstream_addr: SocketAddr = format!("127.0.0.1:{}", upstream.host_port()).parse()?;
    let subject_addr: SocketAddr = format!("127.0.0.1:{}", subject.port()).parse()?;

    // 10s accept-ready budget per SPEC §D4 step 4.
    let budget = Duration::from_secs(10);
    wait_accept_ready(upstream_addr, budget)
        .await
        .context("upstream Envoy never became accept-ready")?;
    wait_accept_ready(subject_addr, budget)
        .await
        .context("envoy-rust never became accept-ready")?;

    // Drive identical bytes at both and compare.
    let payload =
        std::fs::read(fixture_dir.join("inputs/payload.bin")).context("reading payload.bin")?;
    let upstream_out = drive_tcp(upstream_addr, &payload)
        .await
        .context("upstream envoy drive")?;
    let subject_out = drive_tcp(subject_addr, &payload)
        .await
        .context("envoy-rust drive")?;

    // Graceful subject shutdown so Drop doesn't SIGKILL unnecessarily.
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);

    if upstream_out != subject_out {
        bail!(
            "byte-exact body mismatch\n  upstream: {upstream_out:?}\n  subject:  {subject_out:?}",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expectations_parse_byte_exact() {
        let yaml = "equivalence:\n  response_body: byte_exact\n";
        let e: Expectations = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(e.equivalence.response_body, BodyRule::ByteExact);
    }

    #[test]
    fn expectations_reject_unknown_rule() {
        let yaml = "equivalence:\n  response_body: sorta_equal\n";
        let r = serde_yaml::from_str::<Expectations>(yaml);
        assert!(r.is_err());
    }

    // Regression for REVIEW.md M3: `#[serde(deny_unknown_fields)]` must reject
    // a typo'd or unexpected top-level key rather than silently dropping it.
    #[test]
    fn expectations_reject_unknown_field() {
        let yaml = "equivalence:\n  response_body: byte_exact\nfoo: bar\n";
        let err = serde_yaml::from_str::<Expectations>(yaml)
            .expect_err("must reject unknown top-level field");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field"),
            "unexpected error message: {msg}",
        );
    }

    // Regression for REVIEW.md M3 at the nested `Equivalence` level.
    #[test]
    fn equivalence_reject_unknown_field() {
        let yaml = "equivalence:\n  response_body: byte_exact\n  extra: true\n";
        let err = serde_yaml::from_str::<Expectations>(yaml)
            .expect_err("must reject unknown nested field");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field"),
            "unexpected error message: {msg}",
        );
    }

    #[test]
    fn render_yaml_substitutes_all_port_tokens() {
        let t = "a: {{PORT}}\nb: {{PORT}}\n";
        assert_eq!(render_yaml(t, 9000), "a: 9000\nb: 9000\n");
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
}
