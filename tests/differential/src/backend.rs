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
        Self::spawn_inner(None).await
    }

    /// 27 Task 6 (D6 / §6.2-LOCKED V2): spawn an echo backend with a
    /// per-instance `--body-marker <marker>`. Two backends spawned with
    /// DISTINCT markers (`backend_1`/`backend_2`) are distinguishable by their
    /// response body's leading `backend: <marker>\n` line — the EDS-reload
    /// discriminating observable (the `[backend_1]` → `[backend_2]` endpoint
    /// swap is only a real swap if the two backends differ). `spawn()` is the
    /// no-marker shim (the pre-27 byte-identical echo shape, unchanged for all
    /// existing fixtures).
    pub async fn spawn_with_marker(marker: &str) -> Result<Self> {
        Self::spawn_inner(Some(marker)).await
    }

    async fn spawn_inner(marker: Option<&str>) -> Result<Self> {
        let port = reserve_port().context("reserving http1 backend port")?;
        let bin = locate_http1_echo_server().context("locating http1-echo-server binary")?;
        let mut cmd = tokio::process::Command::new(&bin);
        cmd.arg("--port").arg(port.to_string());
        if let Some(marker) = marker {
            cmd.arg("--body-marker").arg(marker);
        }
        let child = cmd
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning {} --port {port}", bin.display()))?;

        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
        // Backstop 30s readiness deadline (was 1s) — CI cold-build budget per
        // the 12.2 Task 8 follow-up b1cb25c precedent (HealthAwareHttp1Backend
        // 3s → 30s bump for the same flake class; CI run 26361106477 RED on
        // backend::tests::http1_echo_backend_spawns_and_echoes with the
        // identical "never became accept-ready" signature).
        wait_accept_ready(addr, Duration::from_secs(30))
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

/// 12.2 D7.1 (06.3 REVIEW I2 down-payment): synthetic health-aware HTTP/1.1
/// backend. Serves 200 on `/` and 503 on `/healthz` by default — the
/// discriminating signal for the active-HC differential fixture. Runs on
/// the host bridge network via testcontainers' `cargo run`-equivalent
/// pattern (the existing helper-binary lifecycle in this module).
pub struct HealthAwareHttp1Backend {
    child: tokio::process::Child,
    port: u16,
}

impl HealthAwareHttp1Backend {
    /// 12.2: spawn the helper backend binary as a tokio subprocess (NOT a
    /// Docker container — the backend runs on the host alongside the
    /// differential harness; the Docker-running envoy + envoy-rust dial
    /// `host.docker.internal:port` per the existing 04.3 / 05.3 helper
    /// pattern). `kill_on_drop(true)` per 09 REVIEW M3 standing discipline.
    ///
    /// 13.1 Task 7: thin shim over `spawn_with_per_path(None)` so fixture
    /// 0019's default-arms semantics (200 on `/`, 503 on `/healthz`) carry
    /// forward unchanged while fixture 0020 opts into per-path status
    /// mapping via the new helper.
    pub async fn spawn() -> Result<Self> {
        Self::spawn_with_per_path(None).await
    }

    /// 13.1 Task 7 / D10: spawn the helper backend with an optional
    /// `--per-path PATH=STATUS[,PATH=STATUS,...]` value. `None` → no
    /// `--per-path` arg (fixture 0019 path); `Some(spec)` → forwards
    /// `--per-path <spec>` to the helper so each request status reflects
    /// the configured per-path arm (fixture 0020 path; the helper's
    /// per-path arm wins over the `/healthz` special-case + the default
    /// arm per `health-aware-http1-backend/src/main.rs`).
    pub async fn spawn_with_per_path(per_path: Option<String>) -> Result<Self> {
        Self::spawn_with_retry_script(None, per_path).await
    }

    /// 16 Task 6: spawn the helper backend with an optional
    /// `--retry-script PATH=fail:N[,...]` value and an optional
    /// `--per-path PATH=STATUS[,...]` value. Either or both may be `None`.
    /// The retry-script knob takes precedence over per-path for paths listed in
    /// the retry-script map (see `health-aware-http1-backend/src/main.rs`).
    ///
    /// Replaces the body of the previous `spawn_with_per_path`; that shim now
    /// delegates here with `retry_script = None`.
    pub async fn spawn_with_retry_script(
        retry_script: Option<String>,
        per_path: Option<String>,
    ) -> Result<Self> {
        let port = crate::reserve_port()?;
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .ok_or_else(|| anyhow::anyhow!("locating workspace root"))?;
        let helper_manifest = manifest.join("tests/helpers/health-aware-http1-backend/Cargo.toml");
        let mut cmd = tokio::process::Command::new(env!("CARGO"));
        cmd.arg("run")
            .arg("--quiet")
            .arg("--manifest-path")
            .arg(&helper_manifest)
            .arg("--")
            .arg("--port")
            .arg(port.to_string());
        if let Some(spec) = retry_script.as_deref() {
            cmd.arg("--retry-script").arg(spec);
        }
        if let Some(spec) = per_path.as_deref() {
            cmd.arg("--per-path").arg(spec);
        }
        let child = cmd
            .stdout(std::process::Stdio::null())
            // 12.2 state-5 review Cluster B I1: stderr is `inherit` (NOT `piped`)
            // matching the 4 sibling backends in this file (TcpProxyBackend,
            // TlsEchoBackend, Http1EchoBackend, Http2EchoBackend). Piped-without-
            // drain risks a pipe-buffer deadlock if the helper ever emits more
            // than ~64 KB of stderr (e.g. under RUST_LOG=debug). Inherit surfaces
            // any helper diagnostics on the test process's terminal and cannot
            // block. The backstop at `crates/envoy-bin/tests/upstream_active_
            // health_check.rs` uses `piped` per its own file-level convention
            // (drained on test-process exit by the test runner) — this divergence
            // is intentional, not an oversight.
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("spawning health-aware-http1-backend")?;
        // Brief readiness poll: connect to 127.0.0.1:port with retry up to ~30s
        // (CI cold-build budget; the helper binary may take >10s to compile via
        // `cargo run --manifest-path` on a cold cargo target/).
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}")
            .parse()
            .context("parsing readiness-probe addr")?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if tokio::net::TcpStream::connect(addr).await.is_ok() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                anyhow::bail!("health-aware-http1-backend did not become ready");
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        Ok(Self { child, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Address from inside a Docker container running on the host bridge
    /// network. Matches the existing `Http1EchoBackend` convention.
    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for HealthAwareHttp1Backend {
    fn drop(&mut self) {
        // kill_on_drop(true) handles the SIGKILL; this Drop is a no-op
        // anchor for the lifecycle contract (matches Http1EchoBackend).
        let _ = self.child.start_kill();
    }
}

/// Spawns the workspace's `http2-echo-server` helper on an ephemeral
/// 127.0.0.1 port and waits until an H2C handshake against it completes.
///
/// Mirrors `Http1EchoBackend`'s posture (per phase-04.3 D14 / SPEC §3 D6.a):
/// ephemeral port reservation; subprocess spawn via `tokio::process::Command`
/// with `kill_on_drop(true)`; SIGKILL-on-Drop polling loop with the awareness-
/// only 02.2 REVIEW M1 carryforward (`std::thread::sleep` from a tokio-runtime
/// thread) — inherited verbatim.
///
/// Accept-readiness polling is H2-shape aware: the poll opens a TCP connection
/// AND runs `h2::client::handshake` via `tokio::time::timeout` — success means
/// the helper has completed its H2 codec setup, not just that it's accepting
/// TCP. (Per SPEC §3 D6.a's option (a) — "H2 handshake polling because the
/// codec setup is what makes the helper actually ready to serve".)
pub struct Http2EchoBackend {
    port: u16,
    child: Option<tokio::process::Child>,
}

impl Http2EchoBackend {
    pub async fn spawn() -> Result<Self> {
        let port = reserve_port().context("reserving http2 backend port")?;
        let bin = locate_http2_echo_server().context("locating http2-echo-server binary")?;
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
        wait_h2_accept_ready(addr, Duration::from_secs(2))
            .await
            .with_context(|| format!("http2-echo-server never became h2-accept-ready on {addr}"))?;

        Ok(Self {
            port,
            child: Some(child),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Hostname the upstream Envoy container uses to reach this backend.
    /// Per ADR-0015 + 05.1 STRICT_DNS posture: always `host.docker.internal`.
    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for Http2EchoBackend {
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

/// H2-aware accept-readiness poll. Connects TCP then runs h2::client::handshake;
/// retries with exponential backoff up to `budget`. Distinct from
/// `wait_accept_ready` (which is TCP-only) per SPEC §3 D6.a's recommendation.
async fn wait_h2_accept_ready(addr: std::net::SocketAddr, budget: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(10);
    loop {
        let attempt = async {
            let tcp = tokio::net::TcpStream::connect(addr).await?;
            let (_send, conn) = h2::client::handshake(tcp).await?;
            tokio::spawn(async move {
                let _ = conn.await;
            });
            anyhow::Ok(())
        };
        match tokio::time::timeout(Duration::from_millis(500), attempt).await {
            Ok(Ok(())) => return Ok(()),
            _ if tokio::time::Instant::now() >= deadline => {
                bail!("http2-echo-server not h2-handshake-ready on {addr} within {budget:?}");
            }
            _ => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(200));
            }
        }
    }
}

/// Locate the workspace's `http2-echo-server` binary. Mirrors
/// `locate_http1_echo_server`. `pub(crate)` so the `lib.rs::tests`
/// cross-module dispatch test can probe binary availability.
pub(crate) fn locate_http2_echo_server() -> Result<PathBuf> {
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
    let mut bin = target_dir.join(profile).join("http2-echo-server");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    if !bin.exists() {
        bail!(
            "http2-echo-server not found at {}; run `cargo build -p http2-echo-server` or `cargo test --workspace`",
            bin.display()
        );
    }
    Ok(bin)
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
            response.starts_with("HTTP/1.1 200 OK\r\n"),
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

    /// 27 Task 6 (D6 / §6.2-LOCKED V2): two distinguishable single-endpoint
    /// echo backends. The EDS-reload fixture swaps `eds_backend`'s endpoint from
    /// `[backend_1]` to `[backend_2]`, so BOTH backends must be running AND
    /// distinguishable by a per-backend body marker (a `GET /probe` response's
    /// leading `backend: <marker>\n` line identifies which one served it). This
    /// is the genuinely-new harness capability Task 7's fixture 0035 consumes;
    /// the full Docker reload differential is native-Linux-CI-authoritative.
    #[tokio::test(flavor = "multi_thread")]
    async fn http1_echo_backends_are_distinguishable_by_marker() {
        if locate_http1_echo_server().is_err() {
            eprintln!(
                "skipping http1_echo_backends_are_distinguishable_by_marker — http1-echo-server not built; run `cargo test --workspace`"
            );
            return;
        }

        let backend_1 = Http1EchoBackend::spawn_with_marker("backend_1")
            .await
            .expect("spawn backend_1");
        let backend_2 = Http1EchoBackend::spawn_with_marker("backend_2")
            .await
            .expect("spawn backend_2");

        // Distinct ports — two independent host subprocesses.
        assert_ne!(
            backend_1.port(),
            backend_2.port(),
            "the two backends must bind distinct ports"
        );

        async fn probe_marker(port: u16) -> String {
            let mut s = TcpStream::connect(("127.0.0.1", port))
                .await
                .expect("connect");
            s.write_all(b"GET /probe HTTP/1.1\r\nHost: x.test\r\nContent-Length: 0\r\n\r\n")
                .await
                .expect("write");
            let mut buf = Vec::new();
            s.read_to_end(&mut buf).await.expect("read");
            String::from_utf8_lossy(&buf).into_owned()
        }

        let r1 = probe_marker(backend_1.port()).await;
        let r2 = probe_marker(backend_2.port()).await;
        assert!(
            r1.contains("backend: backend_1\n"),
            "backend_1 must carry its marker: {r1}"
        );
        assert!(
            r2.contains("backend: backend_2\n"),
            "backend_2 must carry its marker: {r2}"
        );
        assert!(
            !r1.contains("backend: backend_2"),
            "backend_1 must NOT carry backend_2's marker: {r1}"
        );

        drop(backend_1);
        drop(backend_2);
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

    #[tokio::test(flavor = "multi_thread")]
    async fn http2_echo_backend_spawns_and_echoes() {
        if locate_http2_echo_server().is_err() {
            eprintln!("skipping http2_echo_backend_spawns_and_echoes — binary not built");
            return;
        }
        let backend = Http2EchoBackend::spawn().await.expect("spawn");
        let addr: std::net::SocketAddr = format!("127.0.0.1:{}", backend.port()).parse().unwrap();
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://testharness/probe")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn http2_echo_backend_drop_terminates_child() {
        if locate_http2_echo_server().is_err() {
            eprintln!("skipping http2_echo_backend_drop_terminates_child — binary not built");
            return;
        }
        let port;
        {
            let backend = Http2EchoBackend::spawn().await.expect("spawn");
            port = backend.port();
        } // backend dropped here — SIGKILL fires
        // Give the OS up to 2s to finalize the kill (mirrors Http1EchoBackend
        // posture per phase-02.2 REVIEW M1 carryforward).
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        // Best-effort assertion: the port is now free (re-bindable).
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await;
        assert!(
            listener.is_ok(),
            "expected port {port} to be re-bindable after backend drop"
        );
    }

    #[test]
    fn locate_http2_echo_server_returns_existing_path() {
        match locate_http2_echo_server() {
            Ok(p) => {
                assert!(
                    p.exists(),
                    "locator returned non-existent path {}",
                    p.display()
                );
            }
            Err(_) => {
                eprintln!(
                    "skipping locate_http2_echo_server_returns_existing_path — binary not built"
                );
            }
        }
    }

    /// Phase 16 Task 6 (amended): stateful retry-script knob on the
    /// `health-aware-http1-backend`. Spawns the backend with:
    ///   --retry-script /retry-success=fail:1
    ///   --per-path /retry-exhausted=503
    /// Then exercises the CYCLIC window for `fail:1` (window length 2 →
    /// alternating 503,200,…):
    ///   • GET /retry-success #1 → 503 (fail body `fail\n`)
    ///   • GET /retry-success #2 → 200 (success body `ok\n`)
    ///   • GET /retry-success #3 → 503 (next window opens)
    ///   • GET /retry-success #4 → 200
    ///   • GET /retry-exhausted #1 → 503 (stateless --per-path arm)
    ///   • GET /retry-exhausted #2 → 503
    ///
    /// Cyclic-window semantics: a single global per-path counter (NOT source-IP
    /// keyed) partitions the request stream into repeating windows of length
    /// `fail_count + 1`. The differential fixture (Task 7) relies on this being
    /// NAT-immune: on macOS, Docker Desktop NATs Envoy-in-Docker's source IP to
    /// 127.0.0.1 — identical to envoy-rust's — so per-source keying is not
    /// viable. Because the harness drives the two proxies sequentially and each
    /// proxy's upstream attempts for one downstream request are consecutive,
    /// each proxy's retry sequence lands in its own fresh window and sees the
    /// same fail-then-succeed pattern.
    #[tokio::test(flavor = "multi_thread")]
    async fn backend_retry_script_stateful_fail_then_succeed() {
        let backend = HealthAwareHttp1Backend::spawn_with_retry_script(
            Some("/retry-success=fail:1".to_string()),
            Some("/retry-exhausted=503".to_string()),
        )
        .await;
        // If the helper binary isn't compiled yet this will fail with a spawn
        // error — skip gracefully so `cargo test -p differential` in isolation
        // doesn't hard-fail on a missing binary.
        let backend = match backend {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipping backend_retry_script_stateful_fail_then_succeed: {e}");
                return;
            }
        };

        let port = backend.port();
        let addr = format!("127.0.0.1:{port}");

        // Helper: issue one HTTP/1.1 GET with Connection: close and return
        // (status_code, body_bytes).
        async fn get(addr: &str, path: &str) -> (u16, Vec<u8>) {
            let mut stream = TcpStream::connect(addr).await.expect("connect");
            let req = format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n");
            stream.write_all(req.as_bytes()).await.expect("write");
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.expect("read");
            let raw = String::from_utf8_lossy(&buf);
            // Parse first line for status.
            let status: u16 = raw
                .split(' ')
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            // Body is after the double-CRLF.
            let body = if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                buf[pos + 4..].to_vec()
            } else {
                Vec::new()
            };
            (status, body)
        }

        // /retry-success: cyclic fail:1 window → 503,200,503,200
        let (s1, b1) = get(&addr, "/retry-success").await;
        assert_eq!(s1, 503, "/retry-success #1 must be 503 (fail attempt)");
        assert_eq!(b1, b"fail\n", "/retry-success #1 body must be `fail\\n`");

        let (s2, b2) = get(&addr, "/retry-success").await;
        assert_eq!(
            s2, 200,
            "/retry-success #2 must be 200 (success after fail)"
        );
        assert_eq!(b2, b"ok\n", "/retry-success #2 body must be `ok\\n`");

        let (s3, b3) = get(&addr, "/retry-success").await;
        assert_eq!(s3, 503, "/retry-success #3 must be 503 (next window opens)");
        assert_eq!(b3, b"fail\n", "/retry-success #3 body must be `fail\\n`");

        let (s4, b4) = get(&addr, "/retry-success").await;
        assert_eq!(s4, 200, "/retry-success #4 must be 200 (window closes)");
        assert_eq!(b4, b"ok\n", "/retry-success #4 body must be `ok\\n`");

        // /retry-exhausted: always 503 (stateless --per-path arm)
        let (e1, _) = get(&addr, "/retry-exhausted").await;
        assert_eq!(e1, 503, "/retry-exhausted #1 must be 503");

        let (e2, _) = get(&addr, "/retry-exhausted").await;
        assert_eq!(e2, 503, "/retry-exhausted #2 must be 503");

        drop(backend);
    }
}
