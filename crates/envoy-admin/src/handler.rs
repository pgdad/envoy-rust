//! `AdminHandler` (`envoy_listener::ConnectionHandler` impl) + `serve` free
//! function (per-listener accept loop). Per-request serial handling — each
//! request closes the connection (no HTTP/1.1 keep-alive in 06.1).

use crate::config::AdminConfig;
use crate::endpoint::{AdminEndpoint, render_404, render_405};
use crate::error::AdminError;
use bytes::BytesMut;
use envoy_listener::{BoxFuture, ConnectionHandler};
use envoy_stats::StatsRegistry;
use std::future::Future;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Maximum total bytes accepted for the request head (request line + headers
/// + final CRLF). Mirrors the existing 8KiB cap from
///   `crates/envoy-bin/src/admin.rs::MAX_REQUEST_HEAD` (phase 02.2 I4).
const MAX_REQUEST_HEAD: usize = 8 * 1024;

/// 06.3 closes 06.1 REVIEW I1: per-read idle timeout for the admin
/// handler. Mirrors the HCM at crates/envoy-http1/src/hcm.rs:24. A
/// connected-but-silent client triggers a clean close within this
/// budget; the connection task does not hold a JoinSet slot indefinitely.
const IDLE_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Drain budget for in-flight admin requests when shutdown fires.
const DRAIN_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

pub struct AdminHandler {
    config: Arc<AdminConfig>,
    registry: Arc<StatsRegistry>,
}

impl AdminHandler {
    pub fn new(config: Arc<AdminConfig>, registry: Arc<StatsRegistry>) -> Self {
        Self { config, registry }
    }

    /// Accessor for the bound `AdminConfig`. Currently primarily useful for
    /// future-task instrumentation (e.g., admin-side access logging would
    /// read `config.access_log_path`); 06.1 has no consumers.
    pub fn config(&self) -> &AdminConfig {
        &self.config
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
        let reason = resp.reason.unwrap_or("OK");
        let head = format!("HTTP/1.1 {status} {reason}\r\n", status = resp.status);
        out.extend_from_slice(head.as_bytes());
        for (name, value) in &resp.headers {
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(b": ");
            out.extend_from_slice(value.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
        // Standard admin-response headers (preserved verbatim from pre-migration
        // shape per SPEC §3 D3 — "non-negotiable mirroring").
        out.extend_from_slice(b"cache-control: no-cache, max-age=0\r\n");
        out.extend_from_slice(b"x-content-type-options: nosniff\r\n");
        out.extend_from_slice(b"server: envoy-rust\r\n");
        let date = envoy_http1::date::format_imf_fixdate(std::time::SystemTime::now());
        let date_line = format!("date: {date}\r\n");
        out.extend_from_slice(date_line.as_bytes());
        // Always close the connection (06.1 has no keep-alive).
        out.extend_from_slice(b"connection: close\r\n");
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&resp.body);
        out
    }

    async fn handle_inner(
        registry: Arc<StatsRegistry>,
        mut stream: TcpStream,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let resp = match Self::read_request(&mut stream).await {
            Ok((method, path)) => {
                if method != "GET" {
                    render_405()
                } else {
                    match AdminEndpoint::from_path(&path) {
                        Some(ep) => ep.render(&registry),
                        None => render_404(),
                    }
                }
            }
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
    fn handle(
        &self,
        downstream: TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let registry = Arc::clone(&self.registry);
        Box::pin(Self::handle_inner(registry, downstream))
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
        let handler = Arc::new(AdminHandler::new(cfg, registry));
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
        let handler = Arc::new(AdminHandler::new(cfg, Arc::clone(&registry)));
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
        let handler = Arc::new(AdminHandler::new(cfg, registry));
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
        let handler = Arc::new(AdminHandler::new(cfg, registry));
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
        let handler = Arc::new(AdminHandler::new(cfg, registry));
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
        let handler = Arc::new(AdminHandler::new(cfg, registry));
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
        let handler = Arc::new(AdminHandler::new(cfg, registry));
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
