//! `envoy.filters.network.direct_response` — the Network-filters family opener
//! (phase 66, ADR-0123), a TERMINAL network filter.
//!
//! On each accepted downstream connection the filter writes its configured
//! payload IMMEDIATELY — without reading or waiting for any client bytes — then
//! half-closes (FIN) and drains the read half until the client closes.
//! Empirically matched against `envoyproxy/envoy:v1.33.0` (phase-66 SPEC §0
//! R-0.5/R-0.7).
//!
//! 67.1 (ADR-0130): the standalone accept loop this module used to own was
//! DELETED, in the same sub-phase as `echo.rs`'s — preserving the "echo is the
//! structural model" invariant the phase-66 review required, and consuming
//! carry-forward **M66-3** by removal. `direct_response` is now a plain
//! `envoy_listener::ConnectionHandler` served by the ONE shared
//! `envoy_listener::Listener` accept loop.

use std::sync::Arc;

use envoy_listener::{BoxFuture, ConnectionHandler, close_with_drain};
use tokio::io::AsyncWriteExt;

/// The terminal `direct_response` network filter, as a per-connection handler.
pub struct DirectResponseHandler {
    payload: Arc<[u8]>,
}

impl DirectResponseHandler {
    /// `payload` may be empty — `response` omitted is a legal config
    /// (phase-66 SPEC §0 R-0.7) and yields a zero-byte write plus a clean close.
    pub fn new(payload: Arc<[u8]>) -> Self {
        Self { payload }
    }
}

impl ConnectionHandler for DirectResponseHandler {
    fn handle(
        &self,
        mut downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let payload = Arc::clone(&self.payload);
        Box::pin(async move {
            // Write the payload immediately; never read first.
            downstream.write_all(&payload).await?;
            downstream.flush().await?;

            // ADR-0124 (phase-66 SPEC V-3): half-close, then drain the read half
            // until the client closes. Closing the socket while unread bytes sit
            // in the receive queue makes the kernel send an RST, so a client that
            // writes after our FIN would see BrokenPipe/ConnectionReset. Upstream
            // Envoy accepts such a write (measured at 0 / 21 / 200_000 unread
            // bytes), so envoy-rust drains to match.
            //
            // 67.1 (consumes M66-4 — the stale doc-precision line this replaces):
            // the drain is bounded by `envoy_listener::DRAIN_BUDGET`. When
            // `Listener::serve` drains, a connection still parked in this loop
            // past the budget is aborted by the accept loop's
            // `JoinSet::abort_all()`. The previous wording named a module-local
            // `DRAIN_TIMEOUT` and `echo.rs`'s accept loop; neither exists now.
            close_with_drain(downstream).await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_listener::{ConnectionHandler, DrainState, Listener};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::oneshot;

    fn listener_cfg(port: u16) -> envoy_config::Listener {
        serde_yaml::from_str(&format!(
            "name: dr_listener\naddress:\n  socket_address:\n    address: 127.0.0.1\n    port_value: {port}\nfilter_chains:\n  - filters: []\n"
        ))
        .expect("hand-constructed listener YAML parses")
    }

    /// 67.1: served by the SHARED `envoy_listener::Listener` accept loop. The
    /// standalone loop this module used to own was deleted (M66-3).
    async fn spawn(payload: &'static [u8]) -> (std::net::SocketAddr, oneshot::Sender<()>) {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let handler: Arc<dyn ConnectionHandler> =
            Arc::new(DirectResponseHandler::new(Arc::from(payload)));
        let listener = Listener::bind(&listener_cfg(0), handler, Arc::clone(&registry))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let drain = Arc::new(DrainState::new(&registry));
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(listener.serve(
            async move {
                let _ = rx.await;
            },
            drain,
        ));
        (addr, tx)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn writes_payload_then_clean_eof() {
        let (addr, _tx) = spawn(b"hello-from-direct-response\n").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF, not RST");
        assert_eq!(out, b"hello-from-direct-response\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn empty_payload_writes_zero_bytes_then_closes() {
        // Phase-66 SPEC §0 R-0.7: Envoy with `response` omitted writes 0 bytes + closes.
        let (addr, _tx) = spawn(b"").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF");
        assert!(out.is_empty(), "expected zero bytes, got {out:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn client_that_writes_first_still_receives_payload() {
        // Phase-66 SPEC §0 R-0.5: Envoy ignores client input and still delivers.
        let (addr, _tx) = spawn(b"PAYLOAD\n").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"PING-NEVER-READ\n").await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF");
        assert_eq!(out, b"PAYLOAD\n");
    }

    /// MUTATION CHECK for the drain (ADR-0124 / phase-66 SPEC V-3).
    ///
    /// Upstream Envoy accepts a client write issued AFTER the client observes
    /// EOF (measured: `post_write=writes_ok` at 0 / 21 / 200_000 unread bytes).
    /// A server that closes without draining its read half sends an RST, and
    /// this write fails with BrokenPipe/ConnectionReset.
    ///
    /// 67.1 re-plumbed the drain into `envoy_listener::close_with_drain`.
    /// DELETE THAT DRAIN LOOP AND THIS TEST MUST FAIL.
    #[tokio::test(flavor = "multi_thread")]
    async fn post_eof_client_write_is_accepted_not_reset() {
        let (addr, _tx) = spawn(b"PAYLOAD\n").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF");
        assert_eq!(out, b"PAYLOAD\n");

        // Two writes: the first may be absorbed locally; a returning RST
        // surfaces on the second. Sleep between them so an RST can land.
        s.write_all(b"y").await.expect("first post-EOF write");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        s.write_all(b"y")
            .await
            .expect("second post-EOF write must not be reset");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shutdown_signal_stops_the_accept_loop() {
        let (addr, tx) = spawn(b"x").await;
        let _ = TcpStream::connect(addr).await.unwrap();
        tx.send(()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert!(
            TcpStream::connect(addr).await.is_err(),
            "listener must be closed"
        );
    }

    /// 67.1: composes under `ChainHandler` like every other terminal filter.
    #[tokio::test(flavor = "multi_thread")]
    async fn direct_response_handler_is_a_connection_handler() {
        let _: Arc<dyn ConnectionHandler> =
            Arc::new(DirectResponseHandler::new(Arc::from(&b"x"[..])));
    }
}
