//! `envoy.filters.network.echo` — a TERMINAL network filter.
//!
//! Each accepted connection copies bytes from the read half to the write half
//! until the client half-closes, mirroring upstream Envoy's
//! `envoy.filters.network.echo`.
//!
//! 67.1 (ADR-0130): the standalone accept loop this module used to own was
//! DELETED. `echo` is now a plain `envoy_listener::ConnectionHandler`, served by
//! the ONE shared `envoy_listener::Listener` accept loop that `tcp_proxy` and
//! HCM already used — which reaps its completed `JoinSet` tasks and bounds
//! in-flight connections by `DRAIN_BUDGET`. That deletion is how carry-forward
//! **M66-3** is consumed. `direct_response.rs` was converted in the same
//! sub-phase, preserving the "echo is the structural model" invariant the
//! phase-66 review required.

use envoy_listener::{BoxFuture, ConnectionHandler};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The terminal `echo` network filter, as a per-connection handler.
pub struct EchoHandler;

impl ConnectionHandler for EchoHandler {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        Box::pin(async move {
            echo_once(downstream).await?;
            Ok(())
        })
    }
}

/// Copy bytes back until the client half-closes, then half-close in turn.
///
/// Fixture `0001` asserts this byte-exact against upstream Envoy. Do NOT swap it
/// for `tokio::io::copy`: that would not issue the trailing `shutdown()`, and
/// the differential harness's ADR-0007 trailing-byte poll depends on the peer
/// either closing or staying silent.
async fn echo_once(mut stream: tokio::net::TcpStream) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.split();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            writer.shutdown().await.ok();
            return Ok(());
        }
        writer.write_all(&buf[..n]).await?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_listener::{ConnectionHandler, DrainState, Listener};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::oneshot;

    fn listener_cfg(port: u16) -> envoy_config::Listener {
        serde_yaml::from_str(&format!(
            r#"
name: echo_listener
address:
  socket_address:
    address: 127.0.0.1
    port_value: {port}
filter_chains:
  - filters:
      - name: envoy.filters.network.echo
"#
        ))
        .expect("hand-constructed listener YAML parses")
    }

    /// Spawn `EchoHandler` behind the SHARED `envoy_listener::Listener` accept
    /// loop — the same loop `tcp_proxy` and HCM use. 67.1 deleted `echo::serve`'s
    /// standalone, non-reaping loop (M66-3).
    async fn spawn() -> (std::net::SocketAddr, oneshot::Sender<()>) {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let handler: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
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
    async fn echoes_single_payload_and_drains_on_shutdown() {
        let (addr, tx) = spawn().await;
        let mut client = TcpStream::connect(addr).await.unwrap();
        let payload = b"hello, envoy-rust\n";
        client.write_all(payload).await.unwrap();
        client.shutdown().await.unwrap();
        let mut echoed = Vec::new();
        client.read_to_end(&mut echoed).await.unwrap();
        assert_eq!(echoed, payload);

        tx.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            TcpStream::connect(addr).await.is_err(),
            "listener closed on shutdown"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn handles_two_concurrent_connections() {
        let (addr, _tx) = spawn().await;
        let one = tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            c.write_all(b"AAA").await.unwrap();
            c.shutdown().await.unwrap();
            let mut out = Vec::new();
            c.read_to_end(&mut out).await.unwrap();
            out
        });
        let two = tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            c.write_all(b"BBBB").await.unwrap();
            c.shutdown().await.unwrap();
            let mut out = Vec::new();
            c.read_to_end(&mut out).await.unwrap();
            out
        });
        assert_eq!(one.await.unwrap(), b"AAA");
        assert_eq!(two.await.unwrap(), b"BBBB");
    }

    /// 67.1: `EchoHandler` is a plain `ConnectionHandler`, so it composes under
    /// `ChainHandler` exactly as `tcp_proxy` and HCM do.
    #[tokio::test(flavor = "multi_thread")]
    async fn echo_handler_is_a_connection_handler() {
        let _: Arc<dyn ConnectionHandler> = Arc::new(EchoHandler);
    }
}
