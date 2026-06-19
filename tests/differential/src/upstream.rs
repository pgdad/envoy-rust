use std::path::Path;
use std::time::Duration;

/// 18 Task 6 (ADR-0049 lock-in L1): the container-internal path the rendered
/// per-fixture CDS file is copied to for the upstream Envoy. The path MUST end
/// in `.yaml` — Envoy selects its config-file parser by extension, and a
/// non-`.yaml` path would make it parse the YAML-content CDS file as JSON-only
/// and fail. The `{{CDS_PATH}}` marker in the upstream `envoy.yaml`
/// (`dynamic_resources.cds_config.path_config_source.path`) is substituted to
/// this constant; the subject side substitutes a host temp path instead.
pub const CDS_CONTAINER_PATH: &str = "/etc/envoy-cds/cds.yaml";

/// 19 Task 6 (ADR-0050, mirrors the CDS L1 constraint): the container-internal
/// path the rendered upstream LDS file is copied to for the upstream Envoy. The
/// path MUST end in `.yaml` — Envoy selects its config-file parser by extension,
/// and a non-`.yaml` path would make it parse the YAML-content LDS file as
/// JSON-only and fail. The `{{LDS_PATH}}` marker in the upstream `envoy.yaml`
/// (`dynamic_resources.lds_config.path_config_source.path`) is substituted to
/// this constant; the subject side substitutes a host temp path instead.
pub const LDS_CONTAINER_PATH: &str = "/etc/envoy-lds/lds.yaml";

/// 20 Task 6 (ADR-0052, mirrors the CDS L1 constraint): the container-internal
/// path the rendered SHARED RDS file is copied to for the upstream Envoy. The
/// path MUST end in `.yaml` — Envoy selects its config-file parser by extension,
/// and a non-`.yaml` path would make it parse the YAML-content RDS file as
/// JSON-only and fail. The `{{RDS_PATH}}` marker in the upstream `envoy.yaml`
/// (HCM `rds.config_source.path_config_source.path`) is substituted to this
/// constant; the subject side substitutes a host temp path instead. Unlike LDS
/// (per-side files), the RDS payload is a bare `RouteConfiguration` that both
/// proxies accept, so a single SHARED `rds.yaml` is rendered per-side.
pub const RDS_CONTAINER_PATH: &str = "/etc/envoy-rds/rds.yaml";

/// 21 Task 6 (ADR-0054, mirrors the CDS L1 constraint): the container-internal
/// path the rendered SHARED EDS file is copied to for the upstream Envoy. The
/// path MUST end in `.yaml` — Envoy selects its config-file parser by extension,
/// and a non-`.yaml` path would make it parse the YAML-content EDS file as
/// JSON-only and fail. The `{{EDS_PATH}}` marker in the upstream `envoy.yaml`
/// (cluster `eds_cluster_config.eds_config.path_config_source.path`) is
/// substituted to this constant; the subject side substitutes a host temp path
/// instead. LIKE CDS (a single SHARED `eds.yaml` rendered per-side through each
/// side's kv map) and UNLIKE LDS (per-side files) — but the rendered endpoint
/// `socket_address.address` differs per side via the `{{EDS_BACKEND_IP}}`
/// numeric-IP marker (Envoy → the discovered host-gateway IP; envoy-rust →
/// `127.0.0.1`), since EDS rejects hostnames (L1).
pub const EDS_CONTAINER_PATH: &str = "/etc/envoy-eds/eds.yaml";

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

    /// 26 Task 7: atomic-rename `new_content` over the in-container RDS file
    /// (`RDS_CONTAINER_PATH`) via `docker exec` — base64-decode into a temp path on the
    /// container's own filesystem, then `mv -f` over the watched path (atomic, same fs).
    /// Must run INSIDE the container: virtiofs bind-mounts do not propagate inotify, so a
    /// host-side rewrite would not trigger the container Envoy's watch (§6.2/ADR-0066).
    ///
    /// NOT locally unit-tested (requires Docker) — exercised by the Task-8 fixture on
    /// native-Linux CI. `new_content` is base64-encoded on the host so it survives the
    /// `sh -c` argv transit intact (arbitrary YAML bytes, newlines, quotes).
    pub async fn reload_rds_atomic(&self, new_content: &str) -> Result<()> {
        use base64::Engine as _;
        use testcontainers::core::{CmdWaitFor, ExecCommand};

        let b64 = base64::engine::general_purpose::STANDARD.encode(new_content.as_bytes());
        // Decode into a temp path on the container's OWN filesystem (a sibling of
        // the watched file, so `mv -f` is a same-fs atomic rename — the ONLY
        // rewrite Envoy's default file-watch observes), then atomically swap.
        let tmp = format!("{RDS_CONTAINER_PATH}.reload-tmp");
        let script = format!(
            "set -e; printf %s '{b64}' | base64 -d > '{tmp}'; mv -f '{tmp}' '{RDS_CONTAINER_PATH}'"
        );
        // `CmdWaitFor::exit_code(0)` makes `exec()` BLOCK until the command
        // finishes (it polls `inspect_exec` until a non-None exit code appears),
        // and errors on a non-zero code. Without it `ExecCommand` defaults to
        // `CmdWaitFor::Nothing` — `exec()` returns as soon as the command is
        // STARTED, so `exit_code()` reads Docker's `ExitCode: null` (None) on a
        // still-running exec and the reload spuriously fails "exited with code
        // None" even though the rename succeeds.
        let result = self
            ._container
            .exec(
                ExecCommand::new(vec!["sh".to_string(), "-c".to_string(), script])
                    .with_cmd_ready_condition(CmdWaitFor::exit_code(0)),
            )
            .await
            .context("docker exec for in-container RDS atomic-rename reload")?;
        // Belt-and-suspenders: the ready condition above already asserted exit 0,
        // so this re-read returns Some(0); keep it as an explicit guard.
        let code = result
            .exit_code()
            .await
            .context("reading exit code of in-container RDS reload exec")?;
        if code != Some(0) {
            anyhow::bail!("in-container RDS atomic-rename reload exited with code {code:?}");
        }
        Ok(())
    }

    /// 27 Task 6 (D6 / §6.2-LOCKED V2 / ADR-0068): atomic-rename `new_content`
    /// over the in-container EDS file (`EDS_CONTAINER_PATH`) via `docker exec` —
    /// the EDS sibling of `reload_rds_atomic`. Base64-decode into a temp path on
    /// the container's own filesystem, then `mv -f` over the watched path
    /// (atomic, same fs). Must run INSIDE the container: virtiofs bind-mounts do
    /// not propagate inotify, so a host-side rewrite would not trigger the
    /// container Envoy's watch (§6.2/ADR-0066).
    ///
    /// NOT locally unit-tested (requires Docker) — exercised by the Task-7
    /// fixture on native-Linux CI. `new_content` is base64-encoded on the host
    /// so it survives the `sh -c` argv transit intact (arbitrary YAML bytes,
    /// newlines, quotes).
    pub async fn reload_eds_atomic(&self, new_content: &str) -> Result<()> {
        use base64::Engine as _;
        use testcontainers::core::{CmdWaitFor, ExecCommand};

        let b64 = base64::engine::general_purpose::STANDARD.encode(new_content.as_bytes());
        let tmp = format!("{EDS_CONTAINER_PATH}.reload-tmp");
        let script = format!(
            "set -e; printf %s '{b64}' | base64 -d > '{tmp}'; mv -f '{tmp}' '{EDS_CONTAINER_PATH}'"
        );
        // See reload_rds_atomic for why CmdWaitFor::exit_code(0) is load-bearing
        // (exec returns before the in-container mv completes, so exit_code() reads
        // None and the reload spuriously fails) and why the exit code is re-read
        // below as a belt-and-suspenders guard.
        let result = self
            ._container
            .exec(
                ExecCommand::new(vec!["sh".to_string(), "-c".to_string(), script])
                    .with_cmd_ready_condition(CmdWaitFor::exit_code(0)),
            )
            .await
            .context("docker exec for in-container EDS atomic-rename reload")?;
        let code = result
            .exit_code()
            .await
            .context("reading exit code of in-container EDS reload exec")?;
        if code != Some(0) {
            anyhow::bail!("in-container EDS atomic-rename reload exited with code {code:?}");
        }
        Ok(())
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
//
// 20 Task 6 (ADR-0052): adding the `rds_file` param (alongside the existing
// `cds_file`/`lds_file`/`tls_pki`/access-log threads) crosses clippy's
// `too_many_arguments` (7) bound. These are all independent
// fixture-feature toggles threaded straight from `run_fixture`; bundling them
// into a struct would add indirection without clarifying the per-feature
// dispatch, so the lint is allowed locally.
#[allow(clippy::too_many_arguments)]
pub async fn start(
    envoy_yaml_path: &Path,
    host_gateway: bool,
    tls_pki: Option<&crate::tls::TlsTestPki>,
    expose_admin_port: bool,
    // 18 Task 6 (ADR-0049 L1): host path of the rendered upstream CDS file.
    // When `Some`, it is copied into the container at `CDS_CONTAINER_PATH`
    // (`.yaml`-suffixed per L1) via `with_copy_to`, mirroring the TLS-PKI
    // mounting pattern. `None` for fixtures with no `{{CDS_PATH}}` marker
    // (all pre-18 fixtures).
    cds_file: Option<&Path>,
    // 19 Task 6 (ADR-0050 L1): host path of the rendered upstream LDS file.
    // When `Some`, it is copied into the container at `LDS_CONTAINER_PATH`
    // (`.yaml`-suffixed per L1) via `with_copy_to`, mirroring `cds_file`
    // exactly. `None` for fixtures with no `{{LDS_PATH}}` marker (all pre-19
    // fixtures).
    lds_file: Option<&Path>,
    // 20 Task 6 (ADR-0052 L1): host path of the rendered SHARED upstream RDS
    // file. When `Some`, it is copied into the container at `RDS_CONTAINER_PATH`
    // (`.yaml`-suffixed per L1) via `with_copy_to`, mirroring `cds_file`
    // exactly. `None` for fixtures with no `{{RDS_PATH}}` marker (all pre-20
    // fixtures).
    rds_file: Option<&Path>,
    // 21 Task 6 (ADR-0054 L1): host path of the rendered SHARED upstream EDS
    // file. When `Some`, it is copied into the container at `EDS_CONTAINER_PATH`
    // (`.yaml`-suffixed per L1) via `with_copy_to`, mirroring `cds_file`
    // exactly. `None` for fixtures with no `{{EDS_PATH}}` marker (all pre-21
    // fixtures).
    eds_file: Option<&Path>,
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
    // 18 Task 6 (ADR-0049 L1): copy the rendered CDS file into the container at
    // `CDS_CONTAINER_PATH` (a constant ending in `.yaml`). Same `with_copy_to`
    // shape as the TLS-PKI mounts above. The host source path is canonicalized
    // so a relative temp path resolves regardless of the container's working
    // directory.
    if let Some(cds) = cds_file {
        let cds_abs = cds
            .canonicalize()
            .with_context(|| format!("canonicalizing CDS file {}", cds.display()))?;
        request = request.with_copy_to(CDS_CONTAINER_PATH, cds_abs);
    }
    // 19 Task 6 (ADR-0050 L1): copy the rendered upstream LDS file into the
    // container at `LDS_CONTAINER_PATH` (a constant ending in `.yaml`). Same
    // `with_copy_to` shape as the CDS mount above; the host source path is
    // canonicalized so a relative temp path resolves regardless of the
    // container's working directory.
    if let Some(lds) = lds_file {
        let lds_abs = lds
            .canonicalize()
            .with_context(|| format!("canonicalizing LDS file {}", lds.display()))?;
        request = request.with_copy_to(LDS_CONTAINER_PATH, lds_abs);
    }
    // 20 Task 6 (ADR-0052 L1): copy the rendered SHARED upstream RDS file into
    // the container at `RDS_CONTAINER_PATH` (a constant ending in `.yaml`). Same
    // `with_copy_to` shape as the CDS/LDS mounts above; the host source path is
    // canonicalized so a relative temp path resolves regardless of the
    // container's working directory.
    if let Some(rds) = rds_file {
        let rds_abs = rds
            .canonicalize()
            .with_context(|| format!("canonicalizing RDS file {}", rds.display()))?;
        request = request.with_copy_to(RDS_CONTAINER_PATH, rds_abs);
    }
    // 21 Task 6 (ADR-0054 L1): copy the rendered SHARED upstream EDS file into
    // the container at `EDS_CONTAINER_PATH` (a constant ending in `.yaml`). Same
    // `with_copy_to` shape as the CDS/LDS/RDS mounts above; the host source path
    // is canonicalized so a relative temp path resolves regardless of the
    // container's working directory.
    if let Some(eds) = eds_file {
        let eds_abs = eds
            .canonicalize()
            .with_context(|| format!("canonicalizing EDS file {}", eds.display()))?;
        request = request.with_copy_to(EDS_CONTAINER_PATH, eds_abs);
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
        let proxy = start(yaml.path(), false, None, false, None, None, None, None, &[])
            .await
            .unwrap();
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
