use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio::time::timeout;

/// Graceful drain budget per D3 step 5 of the SPEC (5 seconds).
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Accept loop. Each accepted connection copies bytes from the read half to the
/// write half until the client half-closes, mirroring Envoy's
/// `envoy.filters.network.echo` filter.
///
/// Returns `Ok(())` after a clean drain on `shutdown`. Individual connection
/// errors are logged via `tracing::warn!` and do not propagate; a connection
/// failure never takes down the server.
pub async fn serve<F>(listener: TcpListener, shutdown: F) -> Result<()>
where
    F: Future<Output = ()>,
{
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
                        set.spawn(async move {
                            if let Err(err) = echo_once(stream).await {
                                tracing::warn!(%peer, error = %err, "echo connection failed");
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

    // Drain: wait up to DRAIN_TIMEOUT for all in-flight echoes to finish.
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

async fn echo_once(mut stream: tokio::net::TcpStream) -> Result<()> {
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::oneshot;

    async fn bind_random_local() -> TcpListener {
        TcpListener::bind(("127.0.0.1", 0)).await.expect("bind :0")
    }

    #[tokio::test]
    async fn echoes_single_payload_and_drains_on_shutdown() {
        let listener = bind_random_local().await;
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            serve(listener, async move {
                let _ = rx.await;
            })
            .await
            .unwrap();
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        let payload = b"hello, envoy-rust\n";
        client.write_all(payload).await.unwrap();
        client.shutdown().await.unwrap();
        let mut echoed = Vec::new();
        client.read_to_end(&mut echoed).await.unwrap();
        assert_eq!(echoed, payload);

        tx.send(()).unwrap();
        timeout(Duration::from_secs(5), server)
            .await
            .expect("server exits within drain window")
            .unwrap();
    }

    #[tokio::test]
    async fn handles_two_concurrent_connections() {
        let listener = bind_random_local().await;
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            serve(listener, async move {
                let _ = rx.await;
            })
            .await
            .unwrap();
        });

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

        tx.send(()).unwrap();
        timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    }
}
