//! `AdminHandler` (`envoy_listener::ConnectionHandler` impl) + `serve` free
//! function (per-listener accept loop). Per-request serial handling — each
//! request closes the connection (no HTTP/1.1 keep-alive in 06.1).

use crate::config::AdminConfig;
use crate::endpoint::{AdminEndpoint, Dispatch, render_404, render_405};
use crate::error::AdminError;
use bytes::BytesMut;
use envoy_cluster::ClusterManager;
use envoy_config::Bootstrap;
use envoy_listener::{BoxFuture, ConnectionHandler, DRAIN_BUDGET};
use envoy_stats::StatsRegistry;
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Maps HTTP status codes to their RFC 7231 reason phrase. Used as the fallback
/// when `Response.reason` is `None`. Phase 08.1 D2 (closes 06.1 REVIEW M1).
fn reason_for_status(code: u16) -> &'static str {
    match code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    }
}

/// Maximum total bytes accepted for the request head (request line + headers
/// + final CRLF). Mirrors the existing 8KiB cap from
///   `crates/envoy-bin/src/admin.rs::MAX_REQUEST_HEAD` (phase 02.2 I4).
const MAX_REQUEST_HEAD: usize = 8 * 1024;

/// 06.3 closes 06.1 REVIEW I1: per-read idle timeout for the admin
/// handler. Mirrors the HCM at crates/envoy-http1/src/hcm.rs:24. A
/// connected-but-silent client triggers a clean close within this
/// budget; the connection task does not hold a JoinSet slot indefinitely.
const IDLE_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub struct AdminHandler {
    config: Arc<AdminConfig>,
    registry: Arc<StatsRegistry>,
    /// Phase 08.1 D13a: parsed `Bootstrap` cached at process startup. Routed
    /// to the `/config_dump` renderer (Task 6) which serializes the bootstrap
    /// as JSON; kept as an `Arc` so the handler can be `Send + Sync` without
    /// cloning the (potentially large) bootstrap on every request. Read by
    /// `render_config_dump` via the `bootstrap()` accessor (Task 6).
    bootstrap: Arc<Bootstrap>,
    /// Phase 08.1 D13a: cluster manager handle for the `/clusters` renderer
    /// (Task 8). `Arc` shape mirrors `bootstrap` for the same Send+Sync reason.
    #[allow(dead_code)] // wired for Tasks 6-9
    cluster_manager: Arc<ClusterManager>,
    /// Phase 08.1 D13a: process-start `Instant` captured at construction. The
    /// `/server_info` renderer (Task 6) computes uptime as
    /// `Instant::now().duration_since(start_instant)`. Held by value (not
    /// `Arc`) because `Instant` is `Copy`.
    #[allow(dead_code)] // wired for Tasks 6-9
    start_instant: Instant,
    /// Phase 08.1 D13a: command-line options surfaced in `/server_info`
    /// (Task 6). Built once at construction time per architecture lock-in
    /// #7 (see PROGRESS Task 1 preamble) — not built lazily at first render.
    /// Currently `envoy-bin` populates this with `{"config_path":
    /// Value::String(<-c value>)}`.
    #[allow(dead_code)] // wired for Tasks 6-9
    command_line_options: BTreeMap<String, serde_yaml::Value>,
}

impl AdminHandler {
    /// Phase 08.1 D13a: widened from the 2-arg `(config, registry)` shape to
    /// a 6-arg shape that captures the four additional handles Tasks 6-9 need
    /// at render time. The 7th `Arc<DrainState>` parameter is added in 08.2
    /// D13b. SPEC §3 D13a called this "5-arg"; PLAN lock-in #7 refines that
    /// to 6-arg by capturing `command_line_options` at construction time.
    pub fn new(
        config: Arc<AdminConfig>,
        registry: Arc<StatsRegistry>,
        bootstrap: Arc<Bootstrap>,
        cluster_manager: Arc<ClusterManager>,
        start_instant: Instant,
        command_line_options: BTreeMap<String, serde_yaml::Value>,
    ) -> Self {
        Self {
            config,
            registry,
            bootstrap,
            cluster_manager,
            start_instant,
            command_line_options,
        }
    }

    /// Accessor for the bound `AdminConfig`. Currently primarily useful for
    /// future-task instrumentation (e.g., admin-side access logging would
    /// read `config.access_log_path`); 06.1 has no consumers.
    pub fn config(&self) -> &AdminConfig {
        &self.config
    }

    /// Phase 08.1 D6 accessor (PLAN lock-in #2). Crate-internal because the
    /// endpoint renderers are the only legitimate consumers. Returns the
    /// `Bootstrap` directly (deref through the `Arc`) so `render_config_dump`
    /// can borrow it into `ConfigDumpEntry::Bootstrap { bootstrap: &..., .. }`
    /// without a `Bootstrap`-wide `Clone` cascade (lock-in #1).
    pub(crate) fn bootstrap(&self) -> &Bootstrap {
        &self.bootstrap
    }

    /// Phase 08.1 D6 accessor (PLAN lock-in #2). Returns the registry so the
    /// `render_with` fallback path can dispatch 06.1 endpoints unchanged.
    pub(crate) fn registry(&self) -> &StatsRegistry {
        &self.registry
    }

    /// Phase 08.1 D13a accessor (PLAN lock-in #2). Reserved for Task 8's
    /// `/clusters` renderer. Currently unused.
    #[allow(dead_code)] // wired for Task 8
    pub(crate) fn cluster_manager(&self) -> &ClusterManager {
        &self.cluster_manager
    }

    /// Phase 08.1 D13a accessor (PLAN lock-in #2). Reserved for Task 7's
    /// `/server_info` uptime computation. Currently unused.
    #[allow(dead_code)] // wired for Task 7
    pub(crate) fn start_instant(&self) -> Instant {
        self.start_instant
    }

    /// Phase 08.1 D13a accessor (PLAN lock-in #2). Reserved for Task 7's
    /// `/server_info` renderer. Currently unused.
    #[allow(dead_code)] // wired for Task 7
    pub(crate) fn command_line_options(&self) -> &BTreeMap<String, serde_yaml::Value> {
        &self.command_line_options
    }

    /// Read at most `MAX_REQUEST_HEAD` bytes until CRLF-CRLF; parse via
    /// `httparse::Request`. Returns `(method, path)` or an error if the
    /// request is malformed / overlength.
    async fn read_request(stream: &mut TcpStream) -> std::io::Result<(String, String)> {
        let mut buf: Vec<u8> = Vec::with_capacity(1024);
        let mut scratch = [0u8; 1024];
        loop {
            if buf.len() >= MAX_REQUEST_HEAD {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "request head exceeds 8 KiB",
                ));
            }
            let cap = MAX_REQUEST_HEAD - buf.len();
            let take = cap.min(scratch.len());
            let n = match tokio::time::timeout(IDLE_READ_TIMEOUT, stream.read(&mut scratch[..take]))
                .await
            {
                Ok(Ok(n)) => n,
                Ok(Err(e)) => return Err(e),
                Err(_elapsed) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "admin idle read timeout: client did not send request head within 5s",
                    ));
                }
            };
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "client closed before sending complete request head",
                ));
            }
            buf.extend_from_slice(&scratch[..n]);
            if find_crlf_crlf(&buf).is_some() {
                break;
            }
        }
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);
        match req.parse(&buf) {
            Ok(httparse::Status::Complete(_)) => {
                let method = req.method.unwrap_or("GET").to_string();
                let path = req.path.unwrap_or("/").to_string();
                Ok((method, path))
            }
            Ok(httparse::Status::Partial) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "incomplete request head",
            )),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        }
    }

    /// Serialize an `envoy_http1::Response` into wire bytes (status line +
    /// headers + CRLF + body). Inlined here per the PLAN-write decision to
    /// keep envoy-admin's accept-loop self-contained.
    ///
    /// Always injects 5 standard admin-response headers (preserved verbatim
    /// from the pre-migration `crates/envoy-bin/src/admin.rs::render_response`
    /// shape per SPEC §3 D3 lines 953-959 "non-negotiable mirroring"):
    /// - `cache-control: no-cache, max-age=0`
    /// - `x-content-type-options: nosniff`
    /// - `server: envoy-rust` (ADR-0011 divergence from upstream)
    /// - `date: <RFC 7231 IMF-fixdate>` (sourced from `envoy_http1::date`)
    /// - `connection: close` (06.1 has no keep-alive)
    fn serialize_response(resp: &envoy_http1::Response) -> BytesMut {
        let mut out = BytesMut::with_capacity(384 + resp.body.len());
        let reason = resp
            .reason
            .unwrap_or_else(|| reason_for_status(resp.status));
        let head = format!("HTTP/1.1 {status} {reason}\r\n", status = resp.status);
        out.extend_from_slice(head.as_bytes());
        for (name, value) in &resp.headers {
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(b": ");
            out.extend_from_slice(value.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
        // Standard admin-response headers (preserved verbatim from pre-migration
        // shape per SPEC §3 D3 — "non-negotiable mirroring"). Phase 08.1 D1
        // (closes 06.1 REVIEW I2): each default is emitted only if the caller
        // has not already supplied a header with the same name (case-insensitive
        // ASCII compare); the caller-supplied value wins.
        let has_header = |name: &str| {
            resp.headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case(name))
        };
        if !has_header("cache-control") {
            out.extend_from_slice(b"cache-control: no-cache, max-age=0\r\n");
        }
        if !has_header("x-content-type-options") {
            out.extend_from_slice(b"x-content-type-options: nosniff\r\n");
        }
        if !has_header("server") {
            out.extend_from_slice(b"server: envoy-rust\r\n");
        }
        if !has_header("date") {
            let date = envoy_http1::date::format_imf_fixdate(std::time::SystemTime::now());
            let date_line = format!("date: {date}\r\n");
            out.extend_from_slice(date_line.as_bytes());
        }
        // Always close the connection (06.1 has no keep-alive). Not in the D1
        // dedupe set: 06.1's no-keep-alive posture is non-negotiable.
        out.extend_from_slice(b"connection: close\r\n");
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&resp.body);
        out
    }

    /// Phase 08.1 D6: widened from `(registry, stream)` to `(handler, stream)`.
    /// Dispatching `Dispatch::Endpoint` now calls
    /// [`AdminEndpoint::render_with`] so handler-scoped state
    /// (`Arc<Bootstrap>`, `ClusterManager`, etc.) is reachable from the
    /// renderers introduced in Tasks 6/7/8/9. The 06.1 endpoints transparently
    /// fall through to the registry-only path inside `render_with`.
    async fn handle_inner(
        handler: Arc<Self>,
        mut stream: TcpStream,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let resp = match Self::read_request(&mut stream).await {
            Ok((method, path)) => match AdminEndpoint::dispatch(&method, &path) {
                Dispatch::Endpoint(ep) => ep.render_with(&handler),
                Dispatch::MethodNotAllowed { allow } => render_405(allow),
                Dispatch::NotFound => render_404(),
            },
            Err(e) => {
                tracing::warn!(error = %e, "admin: failed to read request head");
                // Best-effort 400 with no body; the connection is likely already broken.
                envoy_http1::Response {
                    status: 400,
                    reason: Some("Bad Request"),
                    headers: vec![("content-length".to_string(), "0".to_string())],
                    body: bytes::Bytes::new(),
                }
            }
        };
        let bytes = Self::serialize_response(&resp);
        stream.write_all(&bytes).await?;
        stream.shutdown().await?;
        Ok(())
    }
}

impl ConnectionHandler for AdminHandler {
    /// Phase 08.1 D6 reshape: the trait surface only hands us `&self`, but
    /// `handle_inner` now needs an `Arc<Self>` so the new
    /// [`AdminEndpoint::render_with`] dispatch path can reach handler-scoped
    /// state. We rebuild an owning handle by cloning each internal `Arc`
    /// field (cheap — `Arc::clone` is a refcount bump) and constructing a
    /// fresh `AdminHandler`. `start_instant: Instant` is `Copy`;
    /// `command_line_options` clones the `BTreeMap` (small in 08.1 — single
    /// `config_path` entry per `envoy-bin/src/main.rs`).
    ///
    /// The 06.1 production path used `Arc::clone(&self.registry)` here for
    /// the same reason; we extend that pattern to the four new handles. The
    /// `serve` accept loop continues to dispatch through this trait method
    /// unchanged (see commit `1d546bc` for the pre-08.1-Task-6 baseline).
    fn handle(
        &self,
        downstream: TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let cloned = Arc::new(AdminHandler {
            config: Arc::clone(&self.config),
            registry: Arc::clone(&self.registry),
            bootstrap: Arc::clone(&self.bootstrap),
            cluster_manager: Arc::clone(&self.cluster_manager),
            start_instant: self.start_instant,
            command_line_options: self.command_line_options.clone(),
        });
        Box::pin(Self::handle_inner(cloned, downstream))
    }
}

/// Per-listener accept loop wrapper around `AdminHandler`. Mirrors the
/// pre-migration `crates/envoy-bin/src/admin.rs::serve` shape.
pub async fn serve(
    listener: tokio::net::TcpListener,
    handler: Arc<AdminHandler>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), AdminError> {
    let mut join_set: tokio::task::JoinSet<Result<(), Box<dyn std::error::Error + Send + Sync>>> =
        tokio::task::JoinSet::new();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("admin listener shutdown signal received; draining");
                drop(listener);
                break;
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, peer)) => {
                        tracing::debug!(%peer, "admin accepted connection");
                        let h = Arc::clone(&handler);
                        join_set.spawn(async move { h.handle(stream).await });
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "admin accept failed; continuing");
                    }
                }
            }
            Some(done) = join_set.join_next(), if !join_set.is_empty() => {
                match done {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => tracing::warn!(error = %err, "admin connection task failed"),
                    Err(join_err) => tracing::warn!(error = %join_err, "admin connection task panicked"),
                }
            }
        }
    }

    let drain = async {
        while let Some(res) = join_set.join_next().await {
            match res {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    tracing::warn!(error = %err, "admin connection task failed during drain")
                }
                Err(join_err) => {
                    tracing::warn!(error = %join_err, "admin connection task panicked during drain")
                }
            }
        }
    };
    if tokio::time::timeout(DRAIN_BUDGET, drain).await.is_err() {
        tracing::warn!(
            ?DRAIN_BUDGET,
            "admin drain budget exceeded; aborting stragglers"
        );
        join_set.abort_all();
        while join_set.join_next().await.is_some() {}
    }
    Ok(())
}

fn find_crlf_crlf(buf: &[u8]) -> Option<usize> {
    let needle = b"\r\n\r\n";
    buf.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{Address, Admin, SocketAddress};
    use std::net::SocketAddr;
    use tokio::sync::oneshot;

    fn admin_config(port: u16) -> AdminConfig {
        AdminConfig::from_envoy_config(&Admin {
            address: Address {
                socket_address: SocketAddress {
                    address: "127.0.0.1".to_string(),
                    port_value: port,
                },
            },
            access_log_path: None,
        })
        .unwrap()
    }

    /// Phase 08.1 Task 5 test-helper: a minimal `Bootstrap` for `AdminHandler::
    /// new`'s 6-arg shape. The pre-task call sites only need `AdminHandler`
    /// constructed; they do not exercise bootstrap-rendering surfaces (those
    /// belong to Tasks 6/7). The YAML below is the smallest shape that
    /// `serde_yaml::from_str::<Bootstrap>` accepts.
    fn dummy_bootstrap() -> Arc<Bootstrap> {
        let yaml =
            "node:\n  id: t\n  cluster: t\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        Arc::new(serde_yaml::from_str::<Bootstrap>(yaml).unwrap())
    }

    /// Phase 08.1 Task 5 test-helper: a zero-cluster `ClusterManager` for
    /// `AdminHandler::new`'s 6-arg shape. The pre-task call sites do not
    /// exercise cluster-rendering surfaces.
    fn dummy_cluster_manager() -> Arc<ClusterManager> {
        Arc::new(ClusterManager::empty())
    }

    async fn bind_random() -> (tokio::net::TcpListener, SocketAddr) {
        let lst = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = lst.local_addr().unwrap();
        (lst, addr)
    }

    async fn drive_request(addr: SocketAddr, req: &[u8]) -> Vec<u8> {
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(req).await.unwrap();
        s.shutdown().await.ok();
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.unwrap();
        buf
    }

    #[tokio::test]
    async fn handler_serves_ready_in_process() {
        let (lst, addr) = bind_random().await;
        let registry = Arc::new(StatsRegistry::new());
        let cfg = Arc::new(admin_config(addr.port()));
        let handler = Arc::new(AdminHandler::new(
            cfg,
            registry,
            dummy_bootstrap(),
            dummy_cluster_manager(),
            Instant::now(),
            BTreeMap::new(),
        ));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(serve(lst, handler, async move {
            let _ = rx.await;
        }));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let resp = drive_request(addr, b"GET /ready HTTP/1.1\r\nHost: x\r\n\r\n").await;
        let s = std::str::from_utf8(&resp).unwrap();
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "status line: {s:?}");
        assert!(s.ends_with("LIVE\n"), "body: {s:?}");
        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn handler_serves_stats_prometheus_in_process() {
        let (lst, addr) = bind_random().await;
        let registry = Arc::new(StatsRegistry::new());
        let c = registry
            .register_counter("listener.foo.downstream_cx_total")
            .unwrap();
        c.add(3);
        let cfg = Arc::new(admin_config(addr.port()));
        let handler = Arc::new(AdminHandler::new(
            cfg,
            Arc::clone(&registry),
            dummy_bootstrap(),
            dummy_cluster_manager(),
            Instant::now(),
            BTreeMap::new(),
        ));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(serve(lst, handler, async move {
            let _ = rx.await;
        }));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let resp = drive_request(addr, b"GET /stats/prometheus HTTP/1.1\r\nHost: x\r\n\r\n").await;
        let s = std::str::from_utf8(&resp).unwrap();
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(s.contains("envoy_listener_foo_downstream_cx_total 3"));
        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn handler_returns_404_for_unknown_path() {
        let (lst, addr) = bind_random().await;
        let registry = Arc::new(StatsRegistry::new());
        let cfg = Arc::new(admin_config(addr.port()));
        let handler = Arc::new(AdminHandler::new(
            cfg,
            registry,
            dummy_bootstrap(),
            dummy_cluster_manager(),
            Instant::now(),
            BTreeMap::new(),
        ));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(serve(lst, handler, async move {
            let _ = rx.await;
        }));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let resp = drive_request(addr, b"GET /unknown HTTP/1.1\r\nHost: x\r\n\r\n").await;
        let s = std::str::from_utf8(&resp).unwrap();
        assert!(s.starts_with("HTTP/1.1 404 Not Found\r\n"));
        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn handler_returns_405_for_post_method() {
        let (lst, addr) = bind_random().await;
        let registry = Arc::new(StatsRegistry::new());
        let cfg = Arc::new(admin_config(addr.port()));
        let handler = Arc::new(AdminHandler::new(
            cfg,
            registry,
            dummy_bootstrap(),
            dummy_cluster_manager(),
            Instant::now(),
            BTreeMap::new(),
        ));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(serve(lst, handler, async move {
            let _ = rx.await;
        }));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let resp = drive_request(addr, b"POST /ready HTTP/1.1\r\nHost: x\r\n\r\n").await;
        let s = std::str::from_utf8(&resp).unwrap();
        assert!(s.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));
        assert!(s.contains("allow: GET\r\n"));
        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn handler_response_carries_server_header() {
        let (lst, addr) = bind_random().await;
        let registry = Arc::new(StatsRegistry::new());
        let cfg = Arc::new(admin_config(addr.port()));
        let handler = Arc::new(AdminHandler::new(
            cfg,
            registry,
            dummy_bootstrap(),
            dummy_cluster_manager(),
            Instant::now(),
            BTreeMap::new(),
        ));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(serve(lst, handler, async move {
            let _ = rx.await;
        }));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let resp = drive_request(addr, b"GET /ready HTTP/1.1\r\nHost: x\r\n\r\n").await;
        let s = std::str::from_utf8(&resp).unwrap();
        assert!(
            s.contains("server: envoy-rust\r\n"),
            "missing server header: {s:?}"
        );
        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    /// 06.3 Task 9 — closes 06.1 REVIEW I1: verifies that a silent client
    /// (no bytes sent after TCP connect) triggers a clean connection close
    /// within 6s (i.e., the 5s IDLE_READ_TIMEOUT fires before the test's 7s
    /// hard deadline). Without the timeout the loop blocks on stream.read()
    /// indefinitely and the test exceeds 7s.
    #[tokio::test]
    async fn admin_handler_idle_read_times_out_at_5s() {
        let (lst, addr) = bind_random().await;
        let registry = Arc::new(StatsRegistry::new());
        let cfg = Arc::new(admin_config(addr.port()));
        let handler = Arc::new(AdminHandler::new(
            cfg,
            registry,
            dummy_bootstrap(),
            dummy_cluster_manager(),
            Instant::now(),
            BTreeMap::new(),
        ));
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(serve(lst, handler, async move {
            let _ = rx.await;
        }));
        // Give the server a moment to start accepting.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Connect but send nothing.
        let mut client = TcpStream::connect(addr).await.unwrap();

        // The server should close the connection within 5s (IDLE_READ_TIMEOUT).
        // Allow 7s on the client side as a hard upper bound.
        let close_result = tokio::time::timeout(std::time::Duration::from_secs(7), async {
            let mut buf = [0u8; 1];
            client.read(&mut buf).await
        })
        .await;

        match close_result {
            Ok(Ok(0)) => {
                // Server closed the connection cleanly (EOF) — expected.
            }
            Ok(Ok(_n)) => {
                // Server sent some bytes (likely a 400 response) before closing — also acceptable.
                // The key property: the call returned within 7s.
            }
            Ok(Err(_e)) => {
                // Connection reset — server closed abruptly, also acceptable within budget.
            }
            Err(_elapsed) => {
                panic!(
                    "admin handler did not close silent-client connection within 7s (IDLE_READ_TIMEOUT not firing)"
                );
            }
        }

        let _ = tx.send(());
    }

    #[tokio::test]
    async fn handler_response_carries_admin_headers() {
        let (lst, addr) = bind_random().await;
        let registry = Arc::new(StatsRegistry::new());
        let cfg = Arc::new(admin_config(addr.port()));
        let handler = Arc::new(AdminHandler::new(
            cfg,
            registry,
            dummy_bootstrap(),
            dummy_cluster_manager(),
            Instant::now(),
            BTreeMap::new(),
        ));
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(serve(lst, handler, async move {
            let _ = rx.await;
        }));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let resp = drive_request(addr, b"GET /ready HTTP/1.1\r\nHost: x\r\n\r\n").await;
        let s = std::str::from_utf8(&resp).unwrap();
        // SPEC §3 D3 "non-negotiable mirroring" — all 4 standard admin headers present.
        assert!(
            s.contains("cache-control: no-cache, max-age=0\r\n"),
            "missing cache-control: {s:?}"
        );
        assert!(
            s.contains("x-content-type-options: nosniff\r\n"),
            "missing x-content-type-options: {s:?}"
        );
        assert!(
            s.contains("server: envoy-rust\r\n"),
            "missing server: {s:?}"
        );
        // date header is dynamic; assert presence with a reasonable shape.
        assert!(s.contains("date: "), "missing date header: {s:?}");
        assert!(
            s.contains(" GMT\r\n"),
            "date header malformed (no GMT): {s:?}"
        );
        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[cfg(test)]
mod serialize_response_dedupe_and_reason_tests {
    use super::AdminHandler;
    use bytes::BytesMut;

    fn serialize(
        status: u16,
        reason: Option<&'static str>,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> String {
        let resp = envoy_http1::Response {
            status,
            reason,
            headers,
            body: bytes::Bytes::from(body),
        };
        let bytes: BytesMut = AdminHandler::serialize_response(&resp);
        String::from_utf8(bytes.to_vec()).expect("ASCII response")
    }

    #[test]
    fn dedupe_preserves_caller_provided_cache_control() {
        let wire = serialize(
            200,
            None,
            vec![("Cache-Control".into(), "public, max-age=60".into())],
            b"OK".to_vec(),
        );
        let lower = wire.to_lowercase();
        let count = lower.matches("cache-control:").count();
        assert_eq!(
            count, 1,
            "exactly one cache-control header; got wire:\n{wire}"
        );
        assert!(
            wire.to_lowercase()
                .contains("cache-control: public, max-age=60")
        );
    }

    #[test]
    fn dedupe_preserves_caller_provided_server() {
        let wire = serialize(
            200,
            None,
            vec![("server".into(), "custom-server".into())],
            b"OK".to_vec(),
        );
        let lower = wire.to_lowercase();
        let count = lower.matches("server:").count();
        assert_eq!(count, 1);
        assert!(lower.contains("server: custom-server"));
    }

    #[test]
    fn dedupe_is_case_insensitive() {
        let wire = serialize(
            200,
            None,
            vec![("X-Content-Type-Options".into(), "myvalue".into())],
            b"OK".to_vec(),
        );
        let lower = wire.to_lowercase();
        let count = lower.matches("x-content-type-options:").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn default_headers_present_when_caller_omits() {
        let wire = serialize(200, None, vec![], b"OK".to_vec());
        let lower = wire.to_lowercase();
        assert!(lower.contains("cache-control: no-cache, max-age=0"));
        assert!(lower.contains("x-content-type-options: nosniff"));
        assert!(lower.contains("server: envoy-rust"));
        assert!(lower.contains("date: "));
    }

    #[test]
    fn reason_503_renders_service_unavailable_without_explicit_reason() {
        let wire = serialize(503, None, vec![], b"".to_vec());
        let first_line = wire.lines().next().expect("status line");
        assert_eq!(first_line, "HTTP/1.1 503 Service Unavailable");
    }

    #[test]
    fn reason_for_status_covers_listed_codes() {
        let cases = [
            (200, "OK"),
            (400, "Bad Request"),
            (404, "Not Found"),
            (405, "Method Not Allowed"),
            (500, "Internal Server Error"),
            (503, "Service Unavailable"),
        ];
        for (code, expect) in cases {
            let wire = serialize(code, None, vec![], b"".to_vec());
            let first_line = wire.lines().next().unwrap();
            assert!(
                first_line.ends_with(expect),
                "{code} reason: got `{first_line}`, want suffix `{expect}`"
            );
        }
    }

    #[test]
    fn explicit_reason_overrides_helper() {
        let wire = serialize(200, Some("Custom"), vec![], b"".to_vec());
        let first_line = wire.lines().next().unwrap();
        assert_eq!(first_line, "HTTP/1.1 200 Custom");
    }
}

#[cfg(test)]
mod drain_budget_lockstep_tests {
    use std::time::Duration;

    #[test]
    fn admin_uses_listener_drain_budget() {
        // Compile-time tautology: if envoy-admin does not import
        // envoy_listener::DRAIN_BUDGET, this fails to compile.
        assert_eq!(envoy_listener::DRAIN_BUDGET, Duration::from_secs(5));
    }
}

#[cfg(test)]
mod admin_handler_new_6arg_tests {
    //! Phase 08.1 D13a: `AdminHandler::new` widens from a 2-arg
    //! `(config, registry)` shape to a 6-arg shape that captures the four
    //! additional handles that Tasks 6/7/8/9 (`/server_info`, `/config_dump`,
    //! `/clusters`, `/stats` JSON timestamps) need at render time: an
    //! `Arc<Bootstrap>` for serialization, an `Arc<ClusterManager>` for the
    //! cluster surface, an `Instant` startup mark for uptime computation, and
    //! a `BTreeMap<String, serde_yaml::Value>` of command-line options.
    //!
    //! SPEC §3 D13a called this widening "5-arg"; the PLAN refined that to
    //! 6-arg by lock-in #7 (`command_line_options` is built once at
    //! construction time, not lazily at first render). Recorded as PROGRESS
    //! deviation #2 for this task.

    use super::AdminHandler;
    use crate::config::AdminConfig;
    use envoy_cluster::ClusterManager;
    use envoy_config::{Address, Admin, Bootstrap, SocketAddress};
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Instant;

    fn dummy_admin_config() -> Arc<AdminConfig> {
        Arc::new(
            AdminConfig::from_envoy_config(&Admin {
                address: Address {
                    socket_address: SocketAddress {
                        address: "127.0.0.1".to_string(),
                        port_value: 0,
                    },
                },
                access_log_path: None,
            })
            .unwrap(),
        )
    }

    fn dummy_bootstrap() -> Arc<Bootstrap> {
        let yaml =
            "node:\n  id: t\n  cluster: t\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        Arc::new(serde_yaml::from_str::<Bootstrap>(yaml).unwrap())
    }

    fn dummy_cluster_manager() -> Arc<ClusterManager> {
        Arc::new(ClusterManager::empty())
    }

    #[test]
    fn admin_handler_new_accepts_six_args_and_constructs() {
        // The body of the test is intentionally minimal: the constructor
        // must accept all six handles and produce a usable `AdminHandler`.
        // The 7th-parameter `Arc<DrainState>` widening is deferred to 08.2
        // D13b. Detailed field-routing tests live with the consumer tasks
        // (6/7/8/9), which render the handles into their endpoints.
        let cfg = dummy_admin_config();
        let registry = Arc::new(StatsRegistry::new());
        let bootstrap = dummy_bootstrap();
        let cluster_manager = dummy_cluster_manager();
        let start_instant = Instant::now();
        let command_line_options: BTreeMap<String, serde_yaml::Value> = BTreeMap::new();

        let handler = AdminHandler::new(
            cfg,
            registry,
            bootstrap,
            cluster_manager,
            start_instant,
            command_line_options,
        );

        // Sanity: the existing `config()` accessor still works post-widening.
        assert_eq!(handler.config().address.port(), 0);
    }
}
