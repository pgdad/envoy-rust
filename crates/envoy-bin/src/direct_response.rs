//! `envoy.filters.network.direct_response` — the Network-filters family opener
//! (phase 66, ADR-0123).
//!
//! On each accepted downstream connection the filter writes its configured
//! payload IMMEDIATELY — without reading or waiting for any client bytes — then
//! half-closes (FIN) and drains the read half until the client closes.
//! Empirically matched against `envoyproxy/envoy:v1.33.0` (SPEC §0 R-0.5/R-0.7).
//!
//! Shaped after `echo.rs`: a standalone accept loop, NOT a
//! `envoy_listener::ConnectionHandler` impl (that trait serves the tcp_proxy
//! and HCM arms).

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio::time::timeout;

/// Graceful drain budget on shutdown, mirroring `echo::DRAIN_TIMEOUT`.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Accept loop. Each accepted connection gets the configured payload, then a
/// FIN, then a read-half drain.
///
/// Returns `Ok(())` after a clean drain on shutdown. Individual connection
/// errors are logged via `tracing::warn!` and never propagate.
pub async fn serve(
    listener: TcpListener,
    payload: Arc<[u8]>,
    shutdown: impl Future<Output = ()>,
) -> Result<()> {
    let mut set: JoinSet<()> = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => {
                tracing::info!("shutdown signal received; closing listener");
                drop(listener);
                break;
            }
            accept = listener.accept() => {
                match accept {
                    Ok((stream, peer)) => {
                        tracing::debug!(%peer, "accepted connection");
                        let payload = Arc::clone(&payload);
                        set.spawn(async move {
                            if let Err(err) = direct_response_once(stream, &payload).await {
                                tracing::warn!(%peer, error = %err, "direct_response connection failed");
                            }
                        });
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "accept failed; continuing");
                    }
                }
            }
        }
    }

    let in_flight = set.len();
    tracing::info!(in_flight, "draining in-flight connections");
    let drained = timeout(DRAIN_TIMEOUT, async {
        while set.join_next().await.is_some() {}
    })
    .await;
    if drained.is_err() {
        tracing::warn!("drain timeout; aborting remaining tasks");
        set.shutdown().await;
    }
    Ok(())
}

async fn direct_response_once(mut stream: tokio::net::TcpStream, payload: &[u8]) -> Result<()> {
    let (mut reader, mut writer) = stream.split();

    // Write the payload immediately; never read first. An empty payload is a
    // legal config (SPEC §0 R-0.7) and yields a zero-byte write.
    writer.write_all(payload).await?;
    writer.flush().await?;

    // Half-close: the client observes a clean EOF here.
    writer.shutdown().await?;

    // ADR-0124 (SPEC V-3): drain the read half until the client closes.
    //
    // Closing the socket while unread bytes sit in the receive queue makes the
    // kernel send an RST, so a client that writes after our FIN would see
    // BrokenPipe/ConnectionReset. Upstream Envoy accepts such a write (measured
    // at 0 / 21 / 200_000 unread bytes), so envoy-rust drains to match. Bounded
    // by the caller's shutdown drain (DRAIN_TIMEOUT), exactly as `echo.rs` is.
    let mut sink = [0u8; 8192];
    loop {
        match reader.read(&mut sink).await {
            Ok(0) => break,    // client closed — done
            Ok(_) => continue, // discard and keep draining
            Err(_) => break,   // peer reset/error — nothing left to do
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::oneshot;

    async fn spawn(payload: &'static [u8]) -> (std::net::SocketAddr, oneshot::Sender<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind :0");
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = serve(listener, Arc::from(payload), async move {
                let _ = rx.await;
            })
            .await;
        });
        (addr, tx)
    }

    #[tokio::test]
    async fn writes_payload_then_clean_eof() {
        let (addr, _tx) = spawn(b"hello-from-direct-response\n").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF, not RST");
        assert_eq!(out, b"hello-from-direct-response\n");
    }

    #[tokio::test]
    async fn empty_payload_writes_zero_bytes_then_closes() {
        // SPEC §0 R-0.7: Envoy with `response` omitted writes 0 bytes + closes.
        let (addr, _tx) = spawn(b"").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF");
        assert!(out.is_empty(), "expected zero bytes, got {out:?}");
    }

    #[tokio::test]
    async fn client_that_writes_first_still_receives_payload() {
        // SPEC §0 R-0.5: Envoy ignores client input and still delivers.
        let (addr, _tx) = spawn(b"PAYLOAD\n").await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"PING-NEVER-READ\n").await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.expect("clean EOF");
        assert_eq!(out, b"PAYLOAD\n");
    }

    /// MUTATION CHECK for the drain (ADR-0124 / SPEC V-3).
    ///
    /// Upstream Envoy accepts a client write issued AFTER the client observes
    /// EOF (measured: `post_write=writes_ok` at 0 / 21 / 200_000 unread bytes).
    /// A server that closes without draining its read half sends an RST, and
    /// this write fails with BrokenPipe/ConnectionReset.
    ///
    /// DELETE THE DRAIN LOOP IN `direct_response_once` AND THIS TEST MUST FAIL.
    #[tokio::test]
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

    #[tokio::test]
    async fn shutdown_signal_stops_the_accept_loop() {
        let (addr, tx) = spawn(b"x").await;
        let _ = TcpStream::connect(addr).await.unwrap();
        tx.send(()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(
            TcpStream::connect(addr).await.is_err(),
            "listener must be closed"
        );
    }
}
