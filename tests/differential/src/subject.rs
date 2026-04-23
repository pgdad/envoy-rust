use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::process::{Child, Command};

/// A running envoy-rust subprocess. Dropping aborts it; calling `shutdown`
/// sends SIGKILL (via tokio's `start_kill`) and waits for the process to exit.
pub struct Subject {
    child: Option<Child>,
    port: u16,
}

impl Subject {
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Terminate the subprocess via SIGKILL (tokio's `start_kill`) and wait
    /// up to `budget` for it to exit. Graceful-drain on SIGTERM is covered by
    /// envoy-bin's own unit tests in Task 7; this harness path only needs the
    /// process to end deterministically between fixture runs.
    //
    // TODO(phase-01): switch to SIGTERM + drain-wait + SIGKILL-escalate so the
    // harness exercises envoy-bin's graceful-drain path. That requires sending
    // POSIX signals to a `tokio::process::Child` (stable tokio exposes only
    // `start_kill` = SIGKILL on Unix), which means adopting the `nix` crate.
    // `nix` is not on the D-3.2 permitted-foundations list for phase 00, so
    // the switch is deferred to phase 01 under its own ADR. Until then this
    // harness relies on SIGKILL for deterministic between-fixture teardown;
    // SIGTERM drain behavior is validated by the envoy-bin unit tests.
    pub async fn shutdown(&mut self, budget: Duration) -> Result<()> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        child.start_kill().ok();
        match tokio::time::timeout(budget, child.wait()).await {
            Ok(Ok(_status)) => Ok(()),
            Ok(Err(err)) => bail!("waiting for envoy-rust: {err}"),
            Err(_) => bail!("envoy-rust did not exit within {budget:?}"),
        }
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Best-effort cleanup on test failure. SIGKILL + forget.
            let _ = child.start_kill();
        }
    }
}

/// Locate the envoy-bin binary built by `cargo test --workspace`. The test
/// crate does not declare envoy-bin as a dependency (no `artifact = "bin"` on
/// stable as of rustc 1.95.0), so we compute the path by convention:
/// `<workspace_root>/target/<profile>/envoy-bin`, honoring `CARGO_TARGET_DIR`.
pub fn locate_envoy_bin() -> Result<PathBuf> {
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
    let mut bin = target_dir.join(profile).join("envoy-bin");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    if !bin.exists() {
        bail!(
            "envoy-bin not found at {}; run `cargo build -p envoy-bin` or `cargo test --workspace`",
            bin.display(),
        );
    }
    Ok(bin)
}

pub async fn start(config_path: &Path, port: u16) -> Result<Subject> {
    let bin = locate_envoy_bin()?;
    let child = Command::new(&bin)
        .arg("-c")
        .arg(config_path)
        .env("ENVOY_RUST_LOG", "info")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawning {}", bin.display()))?;
    Ok(Subject {
        child: Some(child),
        port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn locate_envoy_bin_points_at_target_dir() {
        // This test assumes `cargo test --workspace` was the entry point, so
        // envoy-bin is already built. Under `cargo test -p differential` in
        // isolation it may fail — that is the documented caveat.
        if let Err(err) = locate_envoy_bin() {
            eprintln!(
                "skipping: {err}\n\
                 (run `cargo build -p envoy-bin` or use `cargo test --workspace`)",
            );
            return;
        }
        let p = locate_envoy_bin().unwrap();
        assert!(p.ends_with("envoy-bin") || p.ends_with("envoy-bin.exe"));
    }

    #[tokio::test]
    async fn starts_and_shuts_down_envoy_rust() {
        if locate_envoy_bin().is_err() {
            eprintln!("skipping: envoy-bin not built");
            return;
        }
        let port = crate::reserve_port().unwrap();
        let yaml = format!(
            r#"
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
"#,
        );
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
        f.flush().unwrap();
        let mut subject = start(f.path(), port).await.unwrap();
        let addr = format!("127.0.0.1:{port}").parse().unwrap();
        crate::wait_accept_ready(addr, Duration::from_secs(5))
            .await
            .unwrap();
        subject.shutdown(Duration::from_secs(5)).await.unwrap();
    }
}
