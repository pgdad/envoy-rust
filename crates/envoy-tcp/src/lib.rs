#![forbid(unsafe_code)]

//! Phase 02.2 TCP proxy filter for envoy-rust. Implements
//! `envoy_listener::ConnectionHandler` for `TcpProxy`. Half-close posture
//! follows ADR-0016 (Envoy v1.33.0 default `enable_half_close: false`):
//! `tokio::io::copy` runs in both directions and EOF on either side
//! propagates via drop of the write half.

use std::net::SocketAddr;

use envoy_listener::{BoxFuture, ConnectionHandler};

/// Per-connection TCP proxy. Holds a cloneable `ClusterHandle` to the
/// upstream cluster and the cluster's name (carried separately for
/// diagnostics — `envoy-cluster` does not expose `Cluster::name()` in 02.1
/// per the REVIEW M1 deferral).
pub struct TcpProxy {
    cluster: envoy_cluster::ClusterHandle,
    cluster_name: String,
}

impl TcpProxy {
    pub fn new(cluster: envoy_cluster::ClusterHandle, cfg: &envoy_config::TcpProxyConfig) -> Self {
        Self {
            cluster,
            cluster_name: cfg.cluster.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TcpProxyError {
    #[error("no healthy endpoint available for cluster '{cluster}'")]
    NoHealthyEndpoint { cluster: String },
    #[error("connecting to upstream {addr}: {source}")]
    UpstreamConnect {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("bidirectional copy failed: {source}")]
    CopyFailed {
        #[source]
        source: std::io::Error,
    },
}

impl ConnectionHandler for TcpProxy {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let pick = self.cluster.pick_endpoint();
        let cluster_name = self.cluster_name.clone();
        Box::pin(async move {
            let addr = pick.ok_or_else(|| {
                Box::new(TcpProxyError::NoHealthyEndpoint {
                    cluster: cluster_name.clone(),
                }) as Box<dyn std::error::Error + Send + Sync>
            })?;

            let upstream = tokio::net::TcpStream::connect(addr)
                .await
                .map_err(|source| {
                    Box::new(TcpProxyError::UpstreamConnect { addr, source })
                        as Box<dyn std::error::Error + Send + Sync>
                })?;

            // Plan-time deviation from SPEC §D2 step 4: use `tokio::select!`
            // rather than `tokio::try_join!` so that an EOF on either side
            // immediately drops the other copy future, releasing the
            // corresponding write half and propagating FIN — matching ADR-
            // 0016's `enable_half_close: false` semantics.
            let (mut dr, mut dw) = downstream.into_split();
            let (mut ur, mut uw) = upstream.into_split();
            let result: Result<(), std::io::Error> = tokio::select! {
                res = tokio::io::copy(&mut dr, &mut uw) => res.map(|_| ()),
                res = tokio::io::copy(&mut ur, &mut dw) => res.map(|_| ()),
            };
            drop((dr, dw, ur, uw));
            result.map_err(|source| {
                Box::new(TcpProxyError::CopyFailed { source })
                    as Box<dyn std::error::Error + Send + Sync>
            })?;

            tracing::debug!(%addr, cluster = %cluster_name, "tcp proxy connection complete");
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    /// Spawn an in-process echo server on an ephemeral port. Returns the
    /// bound address. The server task echoes a single connection's bytes
    /// back via `tokio::io::copy` and then closes.
    async fn spawn_echo() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let (mut r, mut w) = stream.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            }
        });
        addr
    }

    /// Build a single-endpoint `ClusterHandle` pointing at `addr`. Use the
    /// YAML path so we go through `parse_bootstrap` + `from_bootstrap`,
    /// mirroring how `envoy-bin` will build the manager in Task 9.
    fn mk_handle(name: &str, addr: SocketAddr) -> envoy_cluster::ClusterHandle {
        let yaml = format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: {name}
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: {name}
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {ip}
                      port_value: {port}
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#,
            ip = addr.ip(),
            port = addr.port(),
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("valid YAML");
        let mgr = envoy_cluster::from_bootstrap(&bootstrap).expect("manager builds");
        mgr.get(name).expect("cluster present")
    }

    fn mk_cfg(cluster_name: &str) -> envoy_config::TcpProxyConfig {
        envoy_config::TcpProxyConfig {
            stat_prefix: "ingress_tcp".to_string(),
            cluster: cluster_name.to_string(),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_payload_end_to_end() {
        let upstream_addr = spawn_echo().await;
        let handle = mk_handle("backend", upstream_addr);
        let proxy = TcpProxy::new(handle, &mk_cfg("backend"));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_arc: Arc<TcpProxy> = Arc::new(proxy);
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            envoy_listener::ConnectionHandler::handle(&*proxy_arc, stream)
                .await
                .expect("handle ok")
        });

        let mut client = TcpStream::connect(downstream_addr).await.expect("connect");
        let payload = b"end-to-end through tcp_proxy";
        client.write_all(payload).await.expect("write");
        let mut buf = vec![0u8; payload.len()];
        client.read_exact(&mut buf).await.expect("read_exact");
        assert_eq!(buf, payload);
        client.shutdown().await.ok();
        drop(client);

        proxy_task.await.expect("proxy task joins");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_closes_downstream_on_upstream_close() {
        // Upstream "echo" server writes back the payload, then closes its
        // write side. The proxy's u2d copy returns EOF and drops `dw`,
        // closing the downstream's read side. The downstream client sees
        // the echoed bytes followed by EOF.
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let upstream_addr = upstream_listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (mut stream, _) = upstream_listener.accept().await.expect("accept");
            let mut buf = [0u8; 5];
            stream.read_exact(&mut buf).await.expect("read");
            stream.write_all(&buf).await.expect("write");
            stream.shutdown().await.ok();
            // Hold the read side open briefly so the downstream can drain.
            let mut tail = [0u8; 16];
            let _ = stream.read(&mut tail).await;
            drop(stream);
        });

        let handle = mk_handle("backend", upstream_addr);
        let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            envoy_listener::ConnectionHandler::handle(&*proxy, stream).await
        });

        let mut client = TcpStream::connect(downstream_addr).await.expect("connect");
        client.write_all(b"hello").await.expect("write");
        let mut echoed = [0u8; 5];
        client.read_exact(&mut echoed).await.expect("read_exact");
        assert_eq!(&echoed, b"hello");

        let mut tail = Vec::new();
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.read_to_end(&mut tail),
        )
        .await;
        assert!(
            tail.is_empty(),
            "expected EOF, got trailing bytes: {tail:?}"
        );

        let _ = proxy_task.await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_closes_upstream_on_downstream_close() {
        // Downstream client drops without writing anything. The proxy's d2u
        // copy returns EOF; select! drops u2d, dropping `uw`. Upstream
        // sees FIN.
        let upstream_seen_fin = Arc::new(tokio::sync::Notify::new());
        let upstream_seen_fin_signal = upstream_seen_fin.clone();
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let upstream_addr = upstream_listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (mut stream, _) = upstream_listener.accept().await.expect("accept");
            let mut buf = [0u8; 64];
            let n = stream.read(&mut buf).await.expect("read");
            assert_eq!(n, 0, "upstream expected to read EOF after downstream drop");
            upstream_seen_fin_signal.notify_one();
        });

        let handle = mk_handle("backend", upstream_addr);
        let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            envoy_listener::ConnectionHandler::handle(&*proxy, stream).await
        });

        let client = TcpStream::connect(downstream_addr).await.expect("connect");
        drop(client);

        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            upstream_seen_fin.notified(),
        )
        .await
        .expect("upstream observed FIN within 3s");
        let _ = proxy_task.await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_returns_err_on_upstream_connect_refused() {
        // 127.0.0.1:1 is reserved (kernel TCP RST) on every UNIX-like host.
        let refused: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let handle = mk_handle("backend", refused);
        let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            envoy_listener::ConnectionHandler::handle(&*proxy, stream).await
        });

        let _client = TcpStream::connect(downstream_addr).await.expect("connect");
        let result = proxy_task.await.expect("proxy task joins");
        let err = result.expect_err("upstream connect must fail");
        let formatted = format!("{err}");
        assert!(
            formatted.contains("connecting to upstream 127.0.0.1:1"),
            "expected UpstreamConnect, got: {formatted}",
        );
    }
}
