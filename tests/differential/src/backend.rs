//! `TcpProxyBackend` — spawns the workspace's `tcp-echo-server` binary as a
//! host subprocess on a reserved 127.0.0.1 port, used by fixture 0003-tcp-proxy
//! as the upstream backend that both proxies dial. See SPEC §3 D4 and SPEC
//! §6 signpost 8: cross-package `CARGO_BIN_EXE_*` is unavailable, so we
//! compute the path as `<workspace>/target/<profile>/tcp-echo-server`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::{reserve_port, wait_accept_ready};

/// A running `tcp-echo-server` host subprocess. Drop sends SIGKILL via
/// tokio's `start_kill` and waits up to 2s for the child to exit (matches
/// `tests/differential/src/subject.rs`'s SIGKILL posture per phase-01 M1's
/// open-ended `nix`-deferral).
pub struct TcpProxyBackend {
    port: u16,
    child: Option<tokio::process::Child>,
}

impl TcpProxyBackend {
    /// Reserve an ephemeral 127.0.0.1 port, locate the workspace's
    /// `tcp-echo-server` binary, spawn it with `--port <port>`, and wait
    /// until the listener accepts a TCP connection. Total readiness budget:
    /// 1s (matches `wait_accept_ready`'s exponential backoff defaults; see
    /// SPEC §6 signpost 8).
    pub async fn spawn() -> Result<Self> {
        let port = reserve_port().context("reserving backend port")?;
        let bin = locate_tcp_echo_server().context("locating tcp-echo-server binary")?;
        let child = tokio::process::Command::new(&bin)
            .arg("--port")
            .arg(port.to_string())
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning {} --port {port}", bin.display()))?;

        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
        wait_accept_ready(addr, Duration::from_secs(1))
            .await
            .with_context(|| format!("tcp-echo-server never became accept-ready on {addr}"))?;

        Ok(Self {
            port,
            child: Some(child),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Hostname the upstream Envoy container uses to reach this backend.
    /// See ADR-0015. Always `host.docker.internal`; envoy-rust on the host
    /// reaches the same backend at `127.0.0.1`.
    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for TcpProxyBackend {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // SIGKILL via tokio's start_kill. Same posture as
            // tests/differential/src/subject.rs.
            let _ = child.start_kill();
            // Best-effort exit wait. Using `try_wait` in a 2s polling loop
            // because `Drop` cannot await; the spawned task pattern would
            // require a runtime handle we don't have here.
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => return,
                }
            }
        }
    }
}

/// `TlsEchoBackend` — spawns the workspace's `tls-echo-server` binary as a
/// subprocess and tears it down on Drop. Sibling of `TcpProxyBackend`. Used
/// by fixture 0005's TLS-upstream backend.
///
/// Drop posture: SIGKILL via tokio's `start_kill` + 50ms-poll/2s-deadline
/// fallback. Mirrors `TcpProxyBackend` (phase-02.2 REVIEW M1 inherited).
///
/// `_server_cert` / `_server_key` are alive-keepers — the spawn lifetime
/// borrows the paths into the child, but the child only holds them until it
/// reads the PEMs at startup; nonetheless keeping owned copies here makes the
/// caller's lifetime story trivially correct (the same `TlsTestPki` `TempDir`
/// outlives the backend by the binding-order discipline in `run_fixture`).
pub struct TlsEchoBackend {
    port: u16,
    child: Option<tokio::process::Child>,
    _server_cert: PathBuf,
    _server_key: PathBuf,
}

impl TlsEchoBackend {
    /// Reserve an ephemeral 127.0.0.1 port, locate the workspace's
    /// `tls-echo-server` binary, spawn it with `--port <port> --cert <path>
    /// --key <path>`, and wait until the listener accepts a TCP connection.
    /// Total readiness budget: 5s (TLS server has more startup work than
    /// plaintext — installing the crypto provider, building `ServerConfig`).
    pub async fn spawn(server_cert: &Path, server_key: &Path) -> Result<Self> {
        let port = reserve_port().context("reserving tls backend port")?;
        let bin = locate_tls_echo_server().context("locating tls-echo-server binary")?;
        let child = tokio::process::Command::new(&bin)
            .arg("--port")
            .arg(port.to_string())
            .arg("--cert")
            .arg(server_cert)
            .arg("--key")
            .arg(server_key)
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning {} --port {port}", bin.display()))?;

        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
        wait_accept_ready(addr, Duration::from_secs(5))
            .await
            .with_context(|| format!("tls-echo-server never became accept-ready on {addr}"))?;

        Ok(Self {
            port,
            child: Some(child),
            _server_cert: server_cert.to_path_buf(),
            _server_key: server_key.to_path_buf(),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Hostname the upstream Envoy container uses to reach this backend.
    /// See ADR-0015. Always `host.docker.internal`; envoy-rust on the host
    /// reaches the same backend at `127.0.0.1`.
    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for TlsEchoBackend {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => return,
                }
            }
        }
    }
}

/// `Http1EchoBackend` — spawns the workspace's `http1-echo-server` binary as a
/// host subprocess on a reserved 127.0.0.1 port. Sibling of `TcpProxyBackend`
/// (phase 02.2) and `TlsEchoBackend` (phase 03.2). Used by fixture
/// 0008-http1-router-upstream as the upstream HTTP/1.1 backend that both
/// proxies dial.
///
/// Drop posture: SIGKILL via tokio's `start_kill` + 50ms-poll/2s-deadline
/// fallback (mirrors TcpProxyBackend / TlsEchoBackend; phase-02.2 REVIEW M1
/// inherited).
pub struct Http1EchoBackend {
    port: u16,
    child: Option<tokio::process::Child>,
}

impl Http1EchoBackend {
    /// Reserve an ephemeral 127.0.0.1 port, locate the workspace's
    /// `http1-echo-server` binary, spawn it with `--port <port>`, and wait
    /// until the listener accepts a TCP connection. Total readiness budget:
    /// 1s (matches TcpProxyBackend's exponential backoff defaults).
    pub async fn spawn() -> Result<Self> {
        let port = reserve_port().context("reserving http1 backend port")?;
        let bin = locate_http1_echo_server().context("locating http1-echo-server binary")?;
        let child = tokio::process::Command::new(&bin)
            .arg("--port")
            .arg(port.to_string())
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning {} --port {port}", bin.display()))?;

        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
        wait_accept_ready(addr, Duration::from_secs(1))
            .await
            .with_context(|| format!("http1-echo-server never became accept-ready on {addr}"))?;

        Ok(Self {
            port,
            child: Some(child),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Hostname the upstream Envoy container uses to reach this backend.
    /// See ADR-0015. Always `host.docker.internal`; envoy-rust on the host
    /// reaches the same backend at `127.0.0.1`.
    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for Http1EchoBackend {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => return,
                }
            }
        }
    }
}

/// Locate the workspace's `http1-echo-server` binary. Mirrors
/// `locate_tcp_echo_server` and `locate_tls_echo_server`. `pub(crate)` so the
/// `lib.rs::tests` cross-module dispatch test can probe binary availability.
pub(crate) fn locate_http1_echo_server() -> Result<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest
        .parent()
        .and_then(|p| p.parent())
        .context("walking up from CARGO_MANIFEST_DIR to workspace root")?;
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let mut bin = target_dir.join(profile).join("http1-echo-server");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    if !bin.exists() {
        bail!(
            "http1-echo-server not found at {}; run `cargo build -p http1-echo-server` or `cargo test --workspace`",
            bin.display(),
        );
    }
    Ok(bin)
}

/// Locate the workspace's `tls-echo-server` binary. Mirrors
/// `locate_tcp_echo_server`. Returns `Err` if the binary is not at the
/// expected path (e.g., not built yet, or workspace layout changed).
fn locate_tls_echo_server() -> Result<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest
        .parent()
        .and_then(|p| p.parent())
        .context("walking up from CARGO_MANIFEST_DIR to workspace root")?;
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let mut bin = target_dir.join(profile).join("tls-echo-server");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    if !bin.exists() {
        bail!(
            "tls-echo-server not found at {}; run `cargo build -p tls-echo-server` or `cargo test --workspace`",
            bin.display(),
        );
    }
    Ok(bin)
}

/// Locate the workspace's `tcp-echo-server` binary. Cargo's
/// `CARGO_BIN_EXE_<name>` is only set for tests in the same package as the
/// binary; we're in the cross-package `differential` crate, so we compute
/// the path by convention. See SPEC §6 signpost 8.
fn locate_tcp_echo_server() -> Result<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    // tests/differential → repo root is two parents up.
    let workspace_root = manifest
        .parent()
        .and_then(|p| p.parent())
        .context("walking up from CARGO_MANIFEST_DIR to workspace root")?;
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let mut bin = target_dir.join(profile).join("tcp-echo-server");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    if !bin.exists() {
        bail!(
            "tcp-echo-server not found at {}; run `cargo build -p tcp-echo-server` or `cargo test --workspace`",
            bin.display(),
        );
    }
    Ok(bin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    #[tokio::test(flavor = "multi_thread")]
    async fn tcp_proxy_backend_spawns_and_echoes() {
        // Skip if the helper binary isn't built — running
        // `cargo test -p differential` in isolation can hit this; the
        // workspace gate (`cargo test --workspace`) builds all binaries.
        if locate_tcp_echo_server().is_err() {
            eprintln!("skipping: tcp-echo-server not built");
            return;
        }
        let backend = TcpProxyBackend::spawn().await.expect("spawn ok");
        let port = backend.port();
        assert!(port > 0);
        assert_eq!(backend.container_host(), "host.docker.internal");

        let mut s = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        let payload = b"backend round-trip";
        s.write_all(payload).await.expect("write");
        let mut buf = vec![0u8; payload.len()];
        s.read_exact(&mut buf).await.expect("read");
        assert_eq!(buf, payload);
        drop(s);
        drop(backend);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tcp_proxy_backend_drop_terminates_child() {
        if locate_tcp_echo_server().is_err() {
            eprintln!("skipping: tcp-echo-server not built");
            return;
        }
        let backend = TcpProxyBackend::spawn().await.expect("spawn ok");
        let port = backend.port();
        // Sanity: the listener is up.
        let s = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect pre-drop");
        drop(s);

        // Drop the backend; the subprocess should exit. After drop, a fresh
        // connect attempt should fail (no listener).
        drop(backend);

        // Allow up to 3s for the child to exit + the kernel to release the port.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            match TcpStream::connect(("127.0.0.1", port)).await {
                Ok(_) if std::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Ok(_) => panic!("tcp-echo-server still listening on {port} 3s after Drop",),
                Err(_) => return,
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tls_echo_backend_spawns_and_echoes() {
        use std::sync::Arc;

        if locate_tls_echo_server().is_err() {
            eprintln!(
                "skipping tls_echo_backend_spawns_and_echoes — tls-echo-server not built; run `cargo test --workspace`"
            );
            return;
        }
        // tls-echo-server installs the aws-lc-rs default crypto provider on
        // startup; the *client* side here also needs one for ClientConfig.
        // Idempotent — ignore the already-installed result.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let pki = crate::tls::TlsTestPki::generate().expect("pki");
        let backend = TlsEchoBackend::spawn(&pki.server_cert, &pki.server_key)
            .await
            .expect("spawn tls-echo-server");

        let mut roots = rustls::RootCertStore::empty();
        let ca_pem = std::fs::read(&pki.ca_pem_path).unwrap();
        let mut slice = ca_pem.as_slice();
        for cert in rustls_pemfile::certs(&mut slice) {
            roots.add(cert.unwrap()).unwrap();
        }
        let client_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));

        let stream = TcpStream::connect(("127.0.0.1", backend.port()))
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
        drop(backend);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tls_echo_backend_drop_terminates_child() {
        if locate_tls_echo_server().is_err() {
            eprintln!(
                "skipping tls_echo_backend_drop_terminates_child — tls-echo-server not built"
            );
            return;
        }
        let pki = crate::tls::TlsTestPki::generate().expect("pki");
        let backend = TlsEchoBackend::spawn(&pki.server_cert, &pki.server_key)
            .await
            .expect("spawn tls-echo-server");
        let port = backend.port();

        drop(backend);

        tokio::time::sleep(Duration::from_millis(200)).await;
        let result = std::net::TcpStream::connect(("127.0.0.1", port));
        assert!(
            result.is_err(),
            "expected port {port} to be released after Drop"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn http1_echo_backend_spawns_and_echoes() {
        // Skip if the helper binary isn't built (cargo test --workspace builds
        // it; cargo test -p differential alone may not).
        if locate_http1_echo_server().is_err() {
            eprintln!(
                "skipping http1_echo_backend_spawns_and_echoes — http1-echo-server not built; run `cargo test --workspace`"
            );
            return;
        }

        let backend = Http1EchoBackend::spawn().await.expect("spawn ok");
        let port = backend.port();
        assert!(port > 0);
        assert_eq!(backend.container_host(), "host.docker.internal");

        let mut s = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        s.write_all(b"GET / HTTP/1.1\r\nHost: x.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write");
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.expect("read");
        let response = String::from_utf8_lossy(&buf);
        assert!(
            response.contains("HTTP/1.1 200 OK\r\n"),
            "status: {response}"
        );
        assert!(
            response.ends_with(
                "method: GET\npath: /\nheaders:\n  content-length: 0\n  host: x.test\nbody: \n"
            ),
            "deterministic echo body: {response}"
        );

        drop(backend);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn http1_echo_backend_drop_terminates_child() {
        if locate_http1_echo_server().is_err() {
            eprintln!(
                "skipping http1_echo_backend_drop_terminates_child — http1-echo-server not built"
            );
            return;
        }
        let backend = Http1EchoBackend::spawn().await.expect("spawn ok");
        let port = backend.port();

        drop(backend);

        tokio::time::sleep(Duration::from_millis(200)).await;
        let result = std::net::TcpStream::connect(("127.0.0.1", port));
        assert!(
            result.is_err(),
            "expected port {port} to be released after Drop"
        );
    }

    #[test]
    fn locate_http1_echo_server_returns_existing_path() {
        // Smoke test for the locator's path-construction logic, not a
        // build-prereq check — skip silently if the binary isn't built.
        match locate_http1_echo_server() {
            Ok(path) => {
                assert!(
                    path.exists(),
                    "locator returned {path:?} but file doesn't exist"
                );
                assert!(
                    path.ends_with("http1-echo-server") || path.ends_with("http1-echo-server.exe")
                );
            }
            Err(_) => {
                eprintln!(
                    "skipping locate_http1_echo_server_returns_existing_path — binary not built"
                );
            }
        }
    }
}
