use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{IntoContainerPort, Mount, WaitFor},
    runners::AsyncRunner,
};

/// Matches ADR-0004 / `docs/envoy-rust/ENVOY_TARGET.md`.
pub const IMAGE_NAME: &str = "envoyproxy/envoy";
pub const IMAGE_TAG: &str = "v1.33.0";
/// Container-internal listener port. Host-side port is assigned by
/// testcontainers at runtime and reported via `host_port()`.
pub const CONTAINER_PORT: u16 = 10000;
/// 06.1 D6.a: container-internal admin listener port. Used by
/// `Driver::AdminScrape` fixtures (e.g. fixture 0011); host-mapped port is
/// reported via `host_admin_port()` when the caller passes
/// `expose_admin_port = true` to `start`. Distinct from `CONTAINER_PORT` so
/// upstream Envoy can listen on both simultaneously inside the container.
pub const ADMIN_CONTAINER_PORT: u16 = 9901;

/// Running upstream Envoy. Dropping this handle stops the container.
pub struct UpstreamProxy {
    _container: ContainerAsync<GenericImage>,
    host_port: u16,
    /// Host-side mapped port for the container's admin listener
    /// (`ADMIN_CONTAINER_PORT`), populated when `start` was called with
    /// `expose_admin_port = true`. None for fixtures that do not need an
    /// admin listener (the pre-06.1 default).
    host_admin_port: Option<u16>,
}

impl UpstreamProxy {
    pub fn host_port(&self) -> u16 {
        self.host_port
    }

    /// Host-mapped admin port if the container was started with
    /// `expose_admin_port = true`. None for fixtures that do not need an
    /// admin listener.
    pub fn host_admin_port(&self) -> Option<u16> {
        self.host_admin_port
    }
}

/// Start upstream Envoy with `envoy_yaml_path` bind-mounted to
/// `/etc/envoy/envoy.yaml`. The caller must have already rendered any
/// `{{PORT}}` token in the YAML to `CONTAINER_PORT`.
///
/// `host_gateway = true` adds `with_host("host.docker.internal", Host::HostGateway)`
/// to the container image (per ADR-0015) — required when the fixture YAML
/// references `host.docker.internal` to reach a host-running backend.
/// `false` keeps the pre-02.2 behavior for fixtures that don't need
/// container-to-host reachability.
///
/// `tls_pki = Some(&pki)` copies each PEM in `pki.container_mounts()` into the
/// container at `/etc/envoy-rust-tls/<filename>.pem` via
/// `with_copy_to` (per parent-SPEC §6 signpost 7 / SPEC §6 signpost 7).
/// Note: testcontainers 0.23.x API uses `with_copy_to(target, source)` rather
/// than the plan-anticipated `with_copy_to_container(host, container)` form —
/// the CopyDataSource wraps a PathBuf directly.
pub async fn start(
    envoy_yaml_path: &Path,
    host_gateway: bool,
    tls_pki: Option<&crate::tls::TlsTestPki>,
    expose_admin_port: bool,
    // 06.2 D4.2.c (revised CI fix #2): bind-mount each (host_dir, container_dir)
    // pair so the container's access-log writes surface on the host where
    // the harness reads them. The PARENT DIRECTORY of the access-log file
    // is mounted (not the file itself): Linux Docker bind-mount semantics
    // for individual files don't reliably propagate write permission to
    // the in-container envoy UID even at 0o666. The caller pre-creates the
    // host-side directory with 0o777 perms; the in-container envoy creates
    // the log file fresh inside under its own UID. Empty slice for fixtures
    // with no access-log surface (pre-06.2 behavior).
    access_log_mounts: &[(String, String)],
) -> Result<UpstreamProxy> {
    let absolute = envoy_yaml_path
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", envoy_yaml_path.display()))?;
    let mut image = GenericImage::new(IMAGE_NAME, IMAGE_TAG)
        .with_exposed_port(CONTAINER_PORT.tcp())
        .with_wait_for(WaitFor::message_on_stderr("starting main dispatch loop"));
    // 06.1 D6.a: when the fixture references {{ADMIN_PORT}}, expose the
    // container's admin listener port too so the harness can scrape
    // /stats/prometheus over the host-mapped address.
    if expose_admin_port {
        image = image.with_exposed_port(ADMIN_CONTAINER_PORT.tcp());
    }
    let mut request = image
        .with_cmd(["-c", "/etc/envoy/envoy.yaml", "--log-level", "info"])
        .with_mount(Mount::bind_mount(
            absolute.to_string_lossy().to_string(),
            "/etc/envoy/envoy.yaml",
        ));
    if host_gateway {
        request = request.with_host(
            "host.docker.internal",
            testcontainers::core::Host::HostGateway,
        );
    }
    if let Some(pki) = tls_pki {
        // Copy each PEM into the container at /etc/envoy-rust-tls/<name>.pem.
        // testcontainers 0.23.x: `with_copy_to(target, source)` where
        // `source: impl Into<CopyDataSource>` and PathBuf implements
        // `From<PathBuf> for CopyDataSource`. PLAN anticipated
        // `with_copy_to_container(host, container)` — actual API differs;
        // adapted without ADR per SPEC §6 signpost 7 (mechanical surface).
        for (host_path, container_path) in pki.container_mounts() {
            request = request.with_copy_to(container_path, host_path);
        }
    }
    // 06.2 D4.2.c: bind-mount access-log file paths so the container's
    // append-mode writes surface on the host where the harness's
    // `std::fs::read_to_string(path)` call reads them.
    for (host_path, container_path) in access_log_mounts.iter() {
        request = request.with_mount(Mount::bind_mount(host_path, container_path));
    }
    let container = request
        .start()
        .await
        .context("starting upstream envoy container")?;
    let host_port = container
        .get_host_port_ipv4(CONTAINER_PORT.tcp())
        .await
        .context("reading host-mapped port from testcontainers")?;
    let host_admin_port = if expose_admin_port {
        Some(
            container
                .get_host_port_ipv4(ADMIN_CONTAINER_PORT.tcp())
                .await
                .context("reading host-mapped admin port from testcontainers")?,
        )
    } else {
        None
    };
    // 05.4 NEW per SPEC §3 D6: STRICT_DNS DNS resolution may not have
    // completed by the 500ms mark on host-gateway fixtures (DNS via
    // Docker's host-gateway races the first test probe); bump to 2000ms
    // for those. The 3 unaffected fixtures (0001/0002/0007) do NOT set
    // host_gateway = true and continue at 500ms.
    let settle_ms = if host_gateway { 2000 } else { 500 };
    tokio::time::sleep(Duration::from_millis(settle_ms)).await;
    Ok(UpstreamProxy {
        _container: container,
        host_port,
        host_admin_port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_envoy_yaml() -> tempfile::NamedTempFile {
        // Smallest legal bootstrap that starts an echo listener. The container
        // listens on CONTAINER_PORT internally.
        let yaml = format!(
            r#"
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.echo.v3.Echo
"#,
            port = CONTAINER_PORT,
        );
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[tokio::test]
    #[ignore = "requires Docker; runs under `cargo test --workspace` in CI"]
    async fn starts_upstream_envoy_and_exposes_host_port() {
        let yaml = tmp_envoy_yaml();
        let proxy = start(yaml.path(), false, None, false, &[]).await.unwrap();
        assert!(proxy.host_port() > 0);
        // Validate accept-readiness via the library's own helper.
        let addr: std::net::SocketAddr =
            format!("127.0.0.1:{}", proxy.host_port()).parse().unwrap();
        crate::wait_accept_ready(addr, Duration::from_secs(15))
            .await
            .unwrap();
        drop(proxy);
    }
}
