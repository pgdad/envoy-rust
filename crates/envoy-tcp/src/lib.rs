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

    /// 03.1: generalize over any `AsyncRead + AsyncWrite` stream so the
    /// listener can pass either a `TcpStream` (plaintext path) or a
    /// `TlsStream<TcpStream>` (post-handshake TLS path) into it. The proxy
    /// logic itself does not care.
    ///
    /// This is an inherent generic method, NOT a trait method — the
    /// `ConnectionHandler` trait stays object-safe with a `TcpStream`-only
    /// `handle`, and the `envoy-bin::TlsAcceptingHandler` adapter (Task 9)
    /// calls this inherent method directly via `Arc<TcpProxy>`. See SPEC §6
    /// signpost 3.
    pub async fn handle<S>(
        &self,
        downstream: S,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let pick = self.cluster.pick_endpoint();
        let cluster_name = self.cluster_name.clone();
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

        // ADR-0016: half-close posture. `tokio::select!` over the two copy
        // futures so EOF on either side drops the other future and propagates
        // FIN via Drop on the write half.
        //
        // Note: `tokio::io::split(downstream)` accepts any
        // `AsyncRead + AsyncWrite + Unpin` — works for both `TcpStream` and
        // `TlsStream<TcpStream>`. The previous `downstream.into_split()` was
        // `TcpStream`-specific; replaced with the generic `tokio::io::split`.
        let (mut dr, mut dw) = tokio::io::split(downstream);
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
        // Thin wrapper that defers to the inherent generic method via the
        // concrete TcpStream type. This impl exists for object safety —
        // `Listener::serve` works over `Arc<dyn ConnectionHandler>`.
        //
        // The trait's `&self` borrow doesn't extend into the boxed future
        // (needs `'static`), so we clone the two Arc-bearing fields and
        // reconstruct a fresh TcpProxy inside the async block. Cheap:
        // ClusterHandle is Arc<Cluster> internally. See PROGRESS.md for
        // the rationale (SPEC §6 signpost 3, option α).
        let cluster = self.cluster.clone();
        let cluster_name = self.cluster_name.clone();
        Box::pin(async move {
            let proxy = TcpProxy {
                cluster,
                cluster_name,
            };
            proxy.handle::<tokio::net::TcpStream>(downstream).await
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

    /// 03.1: prove `TcpProxy::handle::<S>` accepts a `TlsStream<TcpStream>` as
    /// the post-handshake downstream stream type. End-to-end byte-equality at
    /// the proxy boundary, no envoy-listener / envoy-bin / envoy-tls
    /// involvement (envoy-tcp + a stub TLS pair built in-test).
    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_payload_through_tls_downstream_stream() {
        use rcgen::{CertificateParams, KeyPair};
        use rustls::pki_types::ServerName;
        use std::convert::TryFrom;

        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        // Plaintext upstream echo backend.
        let upstream_addr = spawn_echo().await;

        // rcgen-built CA + leaf with SAN `localhost` for the in-test TLS pair.
        let mut ca_params = CertificateParams::new(vec!["test-ca".into()]).expect("ca params");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_kp = KeyPair::generate().expect("ca kp");
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca");

        let leaf_params = CertificateParams::new(vec!["localhost".into()]).expect("leaf params");
        let leaf_kp = KeyPair::generate().expect("leaf kp");
        let leaf_cert = leaf_params
            .signed_by(&leaf_kp, &ca_cert, &ca_kp)
            .expect("leaf");
        let leaf_der: rustls::pki_types::CertificateDer<'static> =
            leaf_cert.der().clone().into_owned();

        let leaf_key_pem = leaf_kp.serialize_pem();
        let mut key_slice = leaf_key_pem.as_bytes();
        let key = rustls_pemfile::private_key(&mut key_slice)
            .expect("priv key parse")
            .expect("priv key present");
        let signing =
            rustls::crypto::aws_lc_rs::sign::any_supported_type(&key).expect("any_supported_type");
        let certified = rustls::sign::CertifiedKey::new(vec![leaf_der], signing);
        let resolver_arc = Arc::new(certified);

        #[derive(Debug)]
        struct StaticResolver(Arc<rustls::sign::CertifiedKey>);
        impl rustls::server::ResolvesServerCert for StaticResolver {
            fn resolve(
                &self,
                _hello: rustls::server::ClientHello<'_>,
            ) -> Option<Arc<rustls::sign::CertifiedKey>> {
                Some(self.0.clone())
            }
        }

        let server_cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(StaticResolver(resolver_arc)) as Arc<_>);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(ca_cert.der().clone().into_owned())
            .expect("root add");
        let client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");

        let handle = mk_handle("backend", upstream_addr);
        let proxy = TcpProxy::new(handle, &mk_cfg("backend"));
        let proxy_arc: Arc<TcpProxy> = Arc::new(proxy);
        let proxy_arc_clone = proxy_arc.clone();
        let acceptor_clone = acceptor.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            let tls_stream = acceptor_clone.accept(stream).await.expect("server tls");
            // The post-handshake `TlsStream<TcpStream>` is the type the
            // generic `TcpProxy::handle::<S>` accepts.
            proxy_arc_clone.handle(tls_stream).await.expect("handle ok")
        });

        let server_name = ServerName::try_from("localhost").expect("server name");
        let tcp_client = TcpStream::connect(downstream_addr).await.expect("connect");
        let mut tls_client = connector
            .connect(server_name, tcp_client)
            .await
            .expect("client handshake");
        let payload = b"end-to-end through tls + tcp_proxy";
        tls_client.write_all(payload).await.expect("write");
        let mut buf = vec![0u8; payload.len()];
        tls_client.read_exact(&mut buf).await.expect("read_exact");
        assert_eq!(buf, payload);
        tls_client.shutdown().await.ok();
        drop(tls_client);

        proxy_task.await.expect("proxy task joins");
    }

    /// Regression: existing phase-02.2 plaintext path still works through the
    /// now-generic `handle` (call site type-resolves to `TcpStream`).
    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_payload_with_plaintext_stream_unchanged() {
        let upstream_addr = spawn_echo().await;
        let handle = mk_handle("backend", upstream_addr);
        let proxy = TcpProxy::new(handle, &mk_cfg("backend"));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_arc: Arc<TcpProxy> = Arc::new(proxy);
        let proxy_arc_clone = proxy_arc.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            // Call the inherent generic method directly with a TcpStream.
            proxy_arc_clone
                .handle::<TcpStream>(stream)
                .await
                .expect("handle ok")
        });

        let mut client = TcpStream::connect(downstream_addr).await.expect("connect");
        let payload = b"plaintext through generic handle";
        client.write_all(payload).await.expect("write");
        let mut buf = vec![0u8; payload.len()];
        client.read_exact(&mut buf).await.expect("read_exact");
        assert_eq!(buf, payload);
        client.shutdown().await.ok();
        drop(client);
        proxy_task.await.expect("proxy task joins");
    }

    /// TLS variant of `proxies_closes_upstream_on_downstream_close` — same
    /// half-close propagation property over a TLS-wrapped downstream.
    #[tokio::test(flavor = "multi_thread")]
    async fn tls_downstream_proxy_closes_upstream_on_downstream_close() {
        use rcgen::{CertificateParams, KeyPair};
        use rustls::pki_types::ServerName;
        use std::convert::TryFrom;

        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let upstream_seen_fin = Arc::new(tokio::sync::Notify::new());
        let upstream_seen_fin_signal = upstream_seen_fin.clone();
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let upstream_addr = upstream_listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (mut stream, _) = upstream_listener.accept().await.expect("accept");
            let mut buf = [0u8; 64];
            let n = stream.read(&mut buf).await.expect("read");
            assert_eq!(n, 0, "upstream expected EOF after downstream drop");
            upstream_seen_fin_signal.notify_one();
        });

        // Build a TLS pair as in `proxies_payload_through_tls_downstream_stream`.
        let mut ca_params = CertificateParams::new(vec!["test-ca".into()]).expect("ca");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_kp = KeyPair::generate().expect("ca kp");
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca");
        let leaf_params = CertificateParams::new(vec!["localhost".into()]).expect("leaf");
        let leaf_kp = KeyPair::generate().expect("leaf kp");
        let leaf_cert = leaf_params
            .signed_by(&leaf_kp, &ca_cert, &ca_kp)
            .expect("leaf");
        let leaf_der: rustls::pki_types::CertificateDer<'static> =
            leaf_cert.der().clone().into_owned();
        let leaf_key_pem = leaf_kp.serialize_pem();
        let mut key_slice = leaf_key_pem.as_bytes();
        let key = rustls_pemfile::private_key(&mut key_slice)
            .unwrap()
            .unwrap();
        let signing = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key).unwrap();
        let certified = Arc::new(rustls::sign::CertifiedKey::new(vec![leaf_der], signing));

        #[derive(Debug)]
        struct R(Arc<rustls::sign::CertifiedKey>);
        impl rustls::server::ResolvesServerCert for R {
            fn resolve(
                &self,
                _: rustls::server::ClientHello<'_>,
            ) -> Option<Arc<rustls::sign::CertifiedKey>> {
                Some(self.0.clone())
            }
        }
        let server_cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(R(certified)) as Arc<_>);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

        let mut roots = rustls::RootCertStore::empty();
        roots.add(ca_cert.der().clone().into_owned()).expect("root");
        let client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");

        let handle = mk_handle("backend", upstream_addr);
        let proxy_arc: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));
        let proxy_arc_clone = proxy_arc.clone();
        let acceptor_clone = acceptor.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            let tls = acceptor_clone.accept(stream).await.expect("server tls");
            proxy_arc_clone.handle(tls).await
        });

        let server_name = ServerName::try_from("localhost").expect("server name");
        let tcp_client = TcpStream::connect(downstream_addr).await.expect("connect");
        let tls_client = connector
            .connect(server_name, tcp_client)
            .await
            .expect("client handshake");
        drop(tls_client);

        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            upstream_seen_fin.notified(),
        )
        .await
        .expect("upstream observed FIN within 3s");
        let _ = proxy_task.await;
    }

    /// TLS-wrapped downstream + refused upstream → same `TcpProxyError::UpstreamConnect`
    /// as plaintext (TLS termination doesn't introduce new upstream errors when
    /// the upstream is plaintext).
    #[tokio::test(flavor = "multi_thread")]
    async fn tls_downstream_proxy_returns_err_on_upstream_connect_refused() {
        use rcgen::{CertificateParams, KeyPair};
        use rustls::pki_types::ServerName;
        use std::convert::TryFrom;

        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let refused: SocketAddr = "127.0.0.1:1".parse().unwrap();

        let mut ca_params = CertificateParams::new(vec!["test-ca".into()]).expect("ca");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_kp = KeyPair::generate().expect("ca kp");
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca");
        let leaf_params = CertificateParams::new(vec!["localhost".into()]).expect("leaf");
        let leaf_kp = KeyPair::generate().expect("leaf kp");
        let leaf_cert = leaf_params
            .signed_by(&leaf_kp, &ca_cert, &ca_kp)
            .expect("leaf");
        let leaf_der: rustls::pki_types::CertificateDer<'static> =
            leaf_cert.der().clone().into_owned();
        let leaf_key_pem = leaf_kp.serialize_pem();
        let mut key_slice = leaf_key_pem.as_bytes();
        let key = rustls_pemfile::private_key(&mut key_slice)
            .unwrap()
            .unwrap();
        let signing = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key).unwrap();
        let certified = Arc::new(rustls::sign::CertifiedKey::new(vec![leaf_der], signing));

        #[derive(Debug)]
        struct R(Arc<rustls::sign::CertifiedKey>);
        impl rustls::server::ResolvesServerCert for R {
            fn resolve(
                &self,
                _: rustls::server::ClientHello<'_>,
            ) -> Option<Arc<rustls::sign::CertifiedKey>> {
                Some(self.0.clone())
            }
        }
        let server_cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(R(certified)) as Arc<_>);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

        let mut roots = rustls::RootCertStore::empty();
        roots.add(ca_cert.der().clone().into_owned()).unwrap();
        let client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");

        let handle = mk_handle("backend", refused);
        let proxy_arc: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));
        let proxy_arc_clone = proxy_arc.clone();
        let acceptor_clone = acceptor.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            let tls = acceptor_clone.accept(stream).await.expect("server tls");
            proxy_arc_clone.handle(tls).await
        });

        let server_name = ServerName::try_from("localhost").expect("server name");
        let tcp_client = TcpStream::connect(downstream_addr).await.expect("connect");
        let _tls_client = connector
            .connect(server_name, tcp_client)
            .await
            .expect("client handshake");

        let result = proxy_task.await.expect("proxy task joins");
        let err = result.expect_err("upstream connect must fail");
        let formatted = format!("{err}");
        assert!(
            formatted.contains("connecting to upstream 127.0.0.1:1"),
            "expected UpstreamConnect, got: {formatted}",
        );
    }
}
