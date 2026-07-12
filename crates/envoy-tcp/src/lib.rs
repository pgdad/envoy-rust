#![forbid(unsafe_code)]

//! Phase 02.2 TCP proxy filter for envoy-rust. Implements
//! `envoy_listener::ConnectionHandler` for `TcpProxy`. Half-close posture
//! follows ADR-0016 (Envoy v1.33.0 default `enable_half_close: false`):
//! `tokio::io::copy` runs in both directions and EOF on either side
//! propagates via drop of the write half.

use std::net::SocketAddr;
use std::sync::Arc;

use envoy_listener::{BoxFuture, ConnectionHandler};

/// Local trait alias unifying tokio's `AsyncRead` + `AsyncWrite`. Auto-impl'd
/// for any `T: AsyncRead + AsyncWrite`. Used internally to box the upstream
/// stream in the 03.2 branched-dial path; both `TcpStream` (plaintext) and
/// `tokio_rustls::client::TlsStream<TcpStream>` (TLS-upstream) impl this.
trait AsyncReadWrite: tokio::io::AsyncRead + tokio::io::AsyncWrite {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite> AsyncReadWrite for T {}

/// Per-connection TCP proxy. Holds a cloneable `ClusterHandle` to the
/// upstream cluster and the cluster's name (carried separately for
/// diagnostics — `envoy-cluster` does not expose `Cluster::name()` in 02.1
/// per the REVIEW M1 deferral; M1 re-evaluated and re-deferred in 03.2 Task 5).
///
/// 03.2: gained `upstream_tls: Option<Arc<envoy_tls::UpstreamTls>>`. The
/// existing `new()` constructor leaves it `None` (plaintext upstream); the
/// new `with_upstream_tls()` constructor sets it to `Some(...)`.
pub struct TcpProxy {
    cluster: envoy_cluster::ClusterHandle,
    cluster_name: String,
    /// 03.2 NEW: when `Some`, the proxy performs an upstream rustls client
    /// handshake immediately after the TCP connect. Shared `Arc` because
    /// rustls's `ClientConfig` is `Send + Sync` and a single `UpstreamTls`
    /// is reused across every connection routed through this proxy.
    upstream_tls: Option<Arc<envoy_tls::UpstreamTls>>,
}

/// The established upstream side of a `tcp_proxy` connection: the (possibly
/// TLS-wrapped) stream, the RAII `cx_active` guard (held until relay ends), and
/// diagnostics. Produced by [`TcpProxy::connect_upstream`] at ESTABLISHMENT
/// (67.3 D4), so a caller can interpose the first-byte gate before relaying.
pub struct UpstreamConn {
    stream: Box<dyn AsyncReadWrite + Send + Unpin>,
    _cx_guard: envoy_cluster::ConnGaugeGuard,
    addr: SocketAddr,
    cluster_name: String,
}

impl TcpProxy {
    /// Plaintext-upstream constructor. Unchanged surface from 03.1; the new
    /// 03.2 `upstream_tls` field defaults to `None`.
    pub fn new(cluster: envoy_cluster::ClusterHandle, cfg: &envoy_config::TcpProxyConfig) -> Self {
        Self {
            cluster,
            cluster_name: cfg.cluster.clone(),
            upstream_tls: None,
        }
    }

    /// 03.2 NEW: TLS-upstream constructor. The provided `Arc<UpstreamTls>` is
    /// shared across every per-connection invocation of `handle`; rustls's
    /// `ClientConfig` is `Send + Sync` and re-used.
    pub fn with_upstream_tls(
        cluster: envoy_cluster::ClusterHandle,
        cfg: &envoy_config::TcpProxyConfig,
        upstream_tls: Arc<envoy_tls::UpstreamTls>,
    ) -> Self {
        Self {
            cluster,
            cluster_name: cfg.cluster.clone(),
            upstream_tls: Some(upstream_tls),
        }
    }

    /// 67.3 D4: the ESTABLISHMENT half — pick an endpoint, hold the `cx_active`
    /// guard, TCP-connect, tick `cluster.<name>.upstream_cx_total`, and (if
    /// configured) run the upstream rustls handshake. Preserves ADR-0016 posture
    /// and the exact 06.x guard/tick placement. Returns the upstream side so the
    /// caller can interpose the first-byte gate before relaying (67.3 D1).
    pub async fn connect_upstream(
        &self,
    ) -> Result<UpstreamConn, Box<dyn std::error::Error + Send + Sync>> {
        // 28 Task 6: TCP proxying has no HTTP-level `hash_policy` — pass `None`
        // (RoundRobin clusters ignore the key; a RING_HASH cluster would fall
        // back to the cursor path here).
        let pick = self.cluster.pick_endpoint(None, None);
        let cluster_name = self.cluster_name.clone();
        let addr = pick.ok_or_else(|| {
            Box::new(TcpProxyError::NoHealthyEndpoint {
                cluster: cluster_name.clone(),
            }) as Box<dyn std::error::Error + Send + Sync>
        })?;

        // 06.3 D15.3.b: RAII guard increments
        // `cluster.<name>.upstream_cx_active` before the dial attempt and
        // decrements via Drop at scope exit, covering both success and error
        // close paths uniformly (the `?` below short-circuits on error but
        // Drop still fires). Mirrors the H1 + H2 HCM guard placement. Held in
        // `UpstreamConn` until `relay`/`relay_gated` returns.
        let _cx_guard = self.cluster.cx_active_guard();

        let stream = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|source| {
                Box::new(TcpProxyError::UpstreamConnect { addr, source })
                    as Box<dyn std::error::Error + Send + Sync>
            })?;
        // 06.1 D4.b: per-cluster `upstream_cx_total` increment on
        // successful upstream TCP connect. Mirrors the H1 + H2 router-proxy
        // arms in envoy-http1::serve_connection / envoy-http2::handle_one_stream.
        // Fires only on the success arm; the `?` above short-circuits the
        // refused-connect path.
        self.cluster.cx_total().inc();

        // 03.2 branched dial: TLS or plaintext upstream. Both arms unify into
        // `Box<dyn AsyncReadWrite + Send + Unpin>` so the relay body stays a
        // single code path.
        let stream: Box<dyn AsyncReadWrite + Send + Unpin> = match &self.upstream_tls {
            None => Box::new(stream),
            Some(tls) => {
                let tls_stream = tls.connect(stream).await.map_err(|source| {
                    Box::new(TcpProxyError::UpstreamTlsHandshake { source })
                        as Box<dyn std::error::Error + Send + Sync>
                })?;
                Box::new(tls_stream)
            }
        };

        Ok(UpstreamConn {
            stream,
            _cx_guard,
            addr,
            cluster_name,
        })
    }

    /// 67.3 D4: the DATA half — the ADR-0016 bidirectional half-close copy.
    /// Unchanged from the old `handle::<S>` tail; `up` carries the `cx_active`
    /// guard, dropped when this returns.
    pub async fn relay<D>(
        &self,
        downstream: D,
        up: UpstreamConn,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        D: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let UpstreamConn {
            stream: upstream,
            _cx_guard,
            addr,
            cluster_name,
        } = up;

        // ADR-0016: half-close posture. `tokio::select!` over the two copy
        // futures so EOF on either side drops the other future and propagates
        // FIN via Drop on the write half.
        //
        // Note: `tokio::io::split(downstream)` accepts any
        // `AsyncRead + AsyncWrite + Unpin` — works for both `TcpStream` and
        // `TlsStream<TcpStream>`. The previous `upstream.into_split()` was
        // `TcpStream`-specific; the boxed `dyn AsyncReadWrite + Send + Unpin`
        // upstream now goes through the generic `tokio::io::split` instead.
        let (mut dr, mut dw) = tokio::io::split(downstream);
        let (mut ur, mut uw) = tokio::io::split(upstream);
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

    /// 03.1 / 67.3 D4: generalize over any `AsyncRead + AsyncWrite` stream so the
    /// listener can pass either a `TcpStream` (plaintext path) or a
    /// `TlsStream<TcpStream>` (post-handshake TLS path) into it. The proxy
    /// logic itself does not care.
    ///
    /// This is an inherent generic method, NOT a trait method — the
    /// `ConnectionHandler` trait stays object-safe with a `TcpStream`-only
    /// `handle`, and the `envoy-bin::TlsAcceptingHandler` adapter (Task 9)
    /// calls this inherent method directly via `Arc<TcpProxy>`. See SPEC §6
    /// signpost 3.
    ///
    /// 67.3 D4: now composes [`connect_upstream`](Self::connect_upstream) +
    /// [`relay`](Self::relay). Behavior is identical to the pre-67.3 straight-line
    /// body (the regression tests prove it).
    pub async fn handle<S>(
        &self,
        downstream: S,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let up = self.connect_upstream().await?;
        self.relay(downstream, up).await
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
    /// 03.2 NEW: upstream TLS handshake failed. Wraps `envoy_tls::TlsError`.
    /// Per-connection failure: the listener's accept loop logs at `warn!` and
    /// drops the connection (phase 02.2 posture); the listener stays up.
    #[error("upstream TLS handshake failed: {source}")]
    UpstreamTlsHandshake {
        #[source]
        source: envoy_tls::TlsError,
    },
}

impl ConnectionHandler for TcpProxy {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        // Thin wrapper that defers to the inherent generic method via the
        // concrete TcpStream type. This impl exists for object safety —
        // `Listener::serve` works over `Arc<dyn ConnectionHandler>`. Used on the
        // lone-`tcp_proxy` (no non-terminal chain) path.
        //
        // The trait's `&self` borrow doesn't extend into the boxed future
        // (needs `'static`), so we clone the two Arc-bearing fields and
        // reconstruct a fresh TcpProxy inside the async block. Cheap:
        // ClusterHandle is Arc<Cluster> internally. See PROGRESS.md for
        // the rationale (SPEC §6 signpost 3, option α).
        let cluster = self.cluster.clone();
        let cluster_name = self.cluster_name.clone();
        let upstream_tls = self.upstream_tls.clone(); // 03.2 NEW: cheap Option<Arc<...>> clone
        Box::pin(async move {
            let proxy = TcpProxy {
                cluster,
                cluster_name,
                upstream_tls,
            };
            proxy.handle::<tokio::net::TcpStream>(downstream).await
        })
    }

    /// 67.3 D1/D3/D4: connect upstream at ESTABLISHMENT (before any downstream
    /// byte — so a server-first banner reaches a byte-less client), then gate the
    /// DOWNSTREAM→UPSTREAM direction on the first byte OR a data-less FIN. This
    /// OVERRIDES the trait default (which peeks first, deadlocking a server-first
    /// protocol). `self: Arc<Self>` keeps the returned future `'static`.
    fn handle_gated(
        self: Arc<Self>,
        downstream: tokio::net::TcpStream,
        gate: envoy_listener::FirstByteGate,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        Box::pin(async move {
            let conn = envoy_listener::ConnectionInfo {
                peer_addr: downstream.peer_addr()?,
                local_addr: downstream.local_addr()?,
            };
            let up = self.connect_upstream().await?; // establishment: cx_total ticks here
            self.relay_gated(downstream, up, gate, conn).await
        })
    }
}

impl TcpProxy {
    /// The gated relay (67.3 D3). Runs the upstream→downstream banner concurrently
    /// with the first-byte gate on the downstream read half; then:
    ///  - `Admitted(Some(b))`: re-inject `b` upstream, then the ADR-0016 select copy;
    ///  - `Admitted(None)` (data-less FIN): propagate FIN upstream, drain upstream→downstream;
    ///  - `Denied`: close downstream WITHOUT forwarding the byte upstream (R-2, W-4);
    ///  - `ClientGoneEarly`: drop.
    async fn relay_gated(
        &self,
        downstream: tokio::net::TcpStream,
        up: UpstreamConn,
        gate: envoy_listener::FirstByteGate,
        conn: envoy_listener::ConnectionInfo,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::io::AsyncWriteExt;
        let UpstreamConn {
            stream: upstream,
            _cx_guard,
            addr,
            cluster_name,
        } = up;
        let (mut dr, mut dw) = downstream.into_split(); // OwnedReadHalf/OwnedWriteHalf: reunitable
        let (mut ur, mut uw) = tokio::io::split(upstream);

        // The upstream→downstream copy is ONE continuous future for the whole
        // connection (67.3 REVIEW I-1): it delivers the server-first banner during
        // the gate wait AND all subsequent upstream bytes after admission. It is
        // NEVER dropped-and-restarted, so its internal `CopyBuffer` (read-but-
        // unwritten bytes under client backpressure) can never be silently lost.
        // `Box::pin` (not `pin!`) so the close paths can `drop` it early to reclaim
        // the `dw`/`ur` borrows for `reunite`.
        let mut u2d = Box::pin(tokio::io::copy(&mut ur, &mut dw));

        // Phase 1: `u2d` (banner) runs WHILE we await the first downstream byte /
        // FIN. The gate reads `dr`; `u2d` writes `dw`; disjoint halves, no contention.
        let (outcome, first) = {
            let mut gate_fut = std::pin::pin!(gate.evaluate_read_half(&mut dr, &conn));
            tokio::select! {
                biased;
                g = &mut gate_fut => g?,
                r = &mut u2d => {
                    // Upstream reached EOF before the client's first byte. ADR-0016
                    // (`enable_half_close:false`): the connection must be torn down
                    // NOW. Do NOT await the gate — a passive client would never send
                    // the byte the gate waits for, so the connection would HANG and
                    // the client would never see the FIN (67.3 REVIEW C-1). No byte
                    // was ever sent, so RBAC never evaluates (ADR-0131 case C): route
                    // to the clean-close path below (`SkippedCleanly` → reunite the
                    // downstream halves + `close_with_drain`), which sends the client
                    // a clean EOF.
                    r.map_err(|source| Box::new(TcpProxyError::CopyFailed { source })
                        as Box<dyn std::error::Error + Send + Sync>)?;
                    (envoy_listener::GateOutcome::SkippedCleanly, None)
                }
            }
        };

        match outcome {
            envoy_listener::GateOutcome::ClientGoneEarly => Ok(()),
            envoy_listener::GateOutcome::SkippedCleanly | envoy_listener::GateOutcome::Denied => {
                // W-4 / R-2: on Denied the first byte MUST NOT reach the upstream.
                // Drop `u2d` (releases the `ur`/`dw` borrows) and the upstream (guard
                // fires), reunite the downstream halves, and close cleanly (zero
                // bytes, clean EOF — close_with_drain).
                drop(u2d);
                drop((ur, uw));
                let ds = dr.reunite(dw).map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;
                envoy_listener::close_with_drain(ds).await?;
                Ok(())
            }
            envoy_listener::GateOutcome::Admitted => {
                match first {
                    Some(b) => {
                        // Re-inject the peeked byte, then the ADR-0016 half-close
                        // select — CONTINUING the same `u2d` copy (no restart, no
                        // lost buffer — 67.3 REVIEW I-1) against the now-open
                        // downstream→upstream copy.
                        uw.write_all(&[b]).await.map_err(|source| {
                            Box::new(TcpProxyError::CopyFailed { source })
                                as Box<dyn std::error::Error + Send + Sync>
                        })?;
                        let result: Result<(), std::io::Error> = tokio::select! {
                            res = tokio::io::copy(&mut dr, &mut uw) => res.map(|_| ()),
                            res = &mut u2d => res.map(|_| ()),
                        };
                        drop(u2d);
                        drop((dr, dw, ur, uw));
                        result.map_err(|source| {
                            Box::new(TcpProxyError::CopyFailed { source })
                                as Box<dyn std::error::Error + Send + Sync>
                        })?;
                    }
                    None => {
                        // Data-less FIN, ALLOW: propagate FIN upstream, drain the
                        // SAME continuous upstream→downstream copy to EOF (no restart).
                        uw.shutdown().await.ok();
                        let _ = u2d.await;
                        drop((dr, dw, ur, uw));
                    }
                }
                tracing::debug!(%addr, cluster = %cluster_name, "tcp proxy gated connection complete");
                Ok(())
            }
        }
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
    async fn mk_handle(name: &str, addr: SocketAddr) -> envoy_cluster::ClusterHandle {
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
        let mgr = envoy_cluster::from_bootstrap(
            &bootstrap,
            std::sync::Arc::new(envoy_stats::StatsRegistry::new()),
        )
        .await
        .expect("manager builds");
        mgr.get(name).expect("cluster present")
    }

    fn mk_cfg(cluster_name: &str) -> envoy_config::TcpProxyConfig {
        envoy_config::TcpProxyConfig {
            stat_prefix: "ingress_tcp".to_string(),
            cluster: cluster_name.to_string(),
        }
    }

    /// Spawn an in-process server-FIRST backend: writes `220 BANNER\r\n`
    /// immediately on accept, then records every byte it subsequently receives.
    /// Returns its address and the recording handle. (67.3 D7.)
    async fn spawn_banner_backend() -> (SocketAddr, Arc<tokio::sync::Mutex<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();
        let recorded = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let rec = recorded.clone();
        tokio::spawn(async move {
            if let Ok((mut s, _)) = listener.accept().await {
                s.write_all(b"220 BANNER\r\n").await.ok();
                s.flush().await.ok();
                let mut buf = [0u8; 1024];
                loop {
                    match s.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => rec.lock().await.extend_from_slice(&buf[..n]),
                    }
                }
            }
        });
        (addr, recorded)
    }

    /// 67.3 D7 / C-1 REGRESSION WITNESS. `[rbac(ALLOW, any), tcp_proxy]` over a
    /// server-first backend: the banner must reach a client that has SENT
    /// NOTHING. Fails against the post-Task-2 code (the default `handle_gated`
    /// peeks, so tcp_proxy never connects upstream and the banner never arrives).
    #[tokio::test(flavor = "multi_thread")]
    async fn banner_reaches_a_client_that_sends_nothing_through_rbac_allow() {
        use envoy_listener::{
            ChainHandler, ConnectionHandler, ConnectionInfo, NetworkFilter, NetworkFilterStatus,
        };
        struct AllowAll;
        impl NetworkFilter for AllowAll {
            fn on_new_connection(&self, _: &ConnectionInfo) -> NetworkFilterStatus {
                NetworkFilterStatus::Continue
            }
        }

        let (backend_addr, _rec) = spawn_banner_backend().await;
        let handle = mk_handle("backend", backend_addr).await;
        let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(AllowAll) as Arc<dyn NetworkFilter>],
            proxy,
        ));

        let dl = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let daddr = dl.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (s, _) = dl.accept().await.unwrap();
            chain.handle(s).await
        });

        let mut client = TcpStream::connect(daddr).await.unwrap();
        // Client sends NOTHING. It must still receive the banner.
        let mut buf = [0u8; 12];
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            client.read_exact(&mut buf),
        )
        .await
        .expect("banner must reach a byte-less client")
        .expect("read banner");
        assert_eq!(&buf, b"220 BANNER\r\n");
        drop(client);
        let _ = task.await;
    }

    /// 67.3 D7 / W-4 / R-2. `[rbac(DENY), tcp_proxy]` over a server-first
    /// backend: the banner is STILL delivered (upstream connected at
    /// establishment), then the client's first byte triggers the DENY and the
    /// connection closes — and that byte must NEVER reach the backend.
    #[tokio::test(flavor = "multi_thread")]
    async fn deny_delivers_banner_then_closes_without_forwarding_the_byte() {
        use envoy_listener::{
            ChainHandler, ConnectionHandler, ConnectionInfo, NetworkFilter, NetworkFilterStatus,
        };
        struct DenyAll;
        impl NetworkFilter for DenyAll {
            fn on_new_connection(&self, _: &ConnectionInfo) -> NetworkFilterStatus {
                NetworkFilterStatus::StopIteration
            }
        }
        let (backend_addr, rec) = spawn_banner_backend().await;
        let handle = mk_handle("backend", backend_addr).await;
        let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(DenyAll) as Arc<dyn NetworkFilter>],
            proxy,
        ));
        let dl = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let daddr = dl.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (s, _) = dl.accept().await.unwrap();
            chain.handle(s).await
        });

        let mut client = TcpStream::connect(daddr).await.unwrap();
        // Banner still delivered (upstream connected at establishment).
        let mut buf = [0u8; 12];
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            client.read_exact(&mut buf),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&buf, b"220 BANNER\r\n");
        // Now send the first byte → DENY closes; the byte must not reach the backend.
        client.write_all(b"Z").await.ok();
        let mut tail = Vec::new();
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            client.read_to_end(&mut tail),
        )
        .await;
        drop(client);
        let _ = task.await;
        assert!(
            !rec.lock().await.contains(&b'Z'),
            "DENY must NOT forward the first byte upstream"
        );
    }

    /// 67.3 D7 / D3. A data-less FIN through `[rbac(ALLOW), tcp_proxy]` reaches
    /// the backend as a clean EOF: the upstream is connected at establishment and
    /// the downstream FIN propagates upstream (half-close). The banner is also
    /// delivered. Pairs with the envoy-bin FIN-matrix backstop (Task 5) that
    /// contrasts this against `echo` (which does NOT evaluate on a data-less FIN).
    #[tokio::test(flavor = "multi_thread")]
    async fn dataless_fin_through_rbac_allow_reaches_backend_as_eof() {
        use envoy_listener::{
            ChainHandler, ConnectionHandler, ConnectionInfo, NetworkFilter, NetworkFilterStatus,
        };
        struct AllowAll;
        impl NetworkFilter for AllowAll {
            fn on_new_connection(&self, _: &ConnectionInfo) -> NetworkFilterStatus {
                NetworkFilterStatus::Continue
            }
        }
        // Backend that signals when it has seen its client's EOF (the propagated FIN).
        let saw_eof = Arc::new(tokio::sync::Notify::new());
        let saw_eof_sig = saw_eof.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let backend_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut s, _)) = listener.accept().await {
                s.write_all(b"220 BANNER\r\n").await.ok();
                s.flush().await.ok();
                let mut buf = [0u8; 64];
                // Read to EOF: a data-less FIN propagated from downstream yields Ok(0).
                loop {
                    match s.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => continue,
                    }
                }
                saw_eof_sig.notify_one();
            }
        });

        let handle = mk_handle("backend", backend_addr).await;
        let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(AllowAll) as Arc<dyn NetworkFilter>],
            proxy,
        ));
        let dl = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let daddr = dl.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (s, _) = dl.accept().await.unwrap();
            chain.handle(s).await
        });

        let mut client = TcpStream::connect(daddr).await.unwrap();
        // Read the banner (server-first), then half-close WITHOUT sending data.
        let mut buf = [0u8; 12];
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            client.read_exact(&mut buf),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&buf, b"220 BANNER\r\n");
        client.shutdown().await.expect("half-close write side");

        tokio::time::timeout(std::time::Duration::from_secs(3), saw_eof.notified())
            .await
            .expect("backend observed the propagated data-less FIN within 3s");
        drop(client);
        let _ = task.await;
    }

    /// Spawn a server-FIRST backend that writes the banner then CLOSES immediately
    /// (upstream EOF right after the banner). (67.3 REVIEW C-1.)
    async fn spawn_banner_then_close_backend() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut s, _)) = listener.accept().await {
                s.write_all(b"220 BANNER\r\n").await.ok();
                s.flush().await.ok();
                // Close immediately — do NOT wait for any client data.
            }
        });
        addr
    }

    /// 67.3 REVIEW C-1 REGRESSION WITNESS. When the `tcp_proxy` upstream reaches
    /// EOF BEFORE the client's first downstream byte (a server-first backend that
    /// closes right after its banner), the connection must be torn down promptly
    /// (ADR-0016 `enable_half_close:false`): the client sees the banner then a
    /// clean EOF. Against the pre-fix code `relay_gated`'s banner branch awaits the
    /// gate for a byte the passive client never sends, so `dw` stays open and the
    /// client HANGS — this test times out.
    #[tokio::test(flavor = "multi_thread")]
    async fn upstream_eof_before_first_byte_closes_downstream_promptly() {
        use envoy_listener::{
            ChainHandler, ConnectionHandler, ConnectionInfo, NetworkFilter, NetworkFilterStatus,
        };
        struct AllowAll;
        impl NetworkFilter for AllowAll {
            fn on_new_connection(&self, _: &ConnectionInfo) -> NetworkFilterStatus {
                NetworkFilterStatus::Continue
            }
        }
        let backend_addr = spawn_banner_then_close_backend().await;
        let handle = mk_handle("backend", backend_addr).await;
        let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(AllowAll) as Arc<dyn NetworkFilter>],
            proxy,
        ));
        let dl = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let daddr = dl.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (s, _) = dl.accept().await.unwrap();
            chain.handle(s).await
        });

        let mut client = TcpStream::connect(daddr).await.unwrap();
        // Read the banner (server-first), then send NOTHING.
        let mut buf = [0u8; 12];
        client.read_exact(&mut buf).await.expect("read banner");
        assert_eq!(&buf, b"220 BANNER\r\n");
        // Upstream already EOF'd. The client must observe a PROMPT EOF (`Ok(0)`),
        // not hang. A read returning 0 within the budget == clean teardown.
        let mut tail = [0u8; 1];
        let n = tokio::time::timeout(std::time::Duration::from_secs(3), client.read(&mut tail))
            .await
            .expect("connection must be torn down promptly on upstream EOF (67.3 REVIEW C-1)")
            .expect("read");
        assert_eq!(
            n, 0,
            "expected a clean EOF after upstream closed, got {n} bytes"
        );
        drop(client);
        let _ = task.await;
    }

    /// 67.3 REVIEW I-2 / I-1 GUARD. The `Admitted(Some(b))` re-inject + duplex
    /// branch had no behavioural witness (every ALLOW test was byte-less; every
    /// first-byte test was DENY). This drives an ALLOWED first byte + payload
    /// through `[rbac(ALLOW), tcp_proxy]` over a server-first backend and asserts:
    /// (a) the re-injected first byte AND the subsequent client payload reach the
    /// backend intact and IN ORDER (no dropped/reordered bytes — the I-1 concern
    /// on the upstream→downstream side is guarded by the banner+response integrity
    /// check below); (b) the banner and a backend response both flow downstream.
    #[tokio::test(flavor = "multi_thread")]
    async fn allowed_first_byte_and_payload_round_trip_both_directions() {
        use envoy_listener::{
            ChainHandler, ConnectionHandler, ConnectionInfo, NetworkFilter, NetworkFilterStatus,
        };
        struct AllowAll;
        impl NetworkFilter for AllowAll {
            fn on_new_connection(&self, _: &ConnectionInfo) -> NetworkFilterStatus {
                NetworkFilterStatus::Continue
            }
        }
        const REQUEST: &[u8] = b"HELLO WORLD, this is the client payload after the first byte";
        // Server-first backend: banner, then read exactly REQUEST.len() bytes,
        // assert they equal REQUEST (re-inject + payload order), then reply.
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let backend_addr = listener.local_addr().unwrap();
        let got = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let got_be = got.clone();
        tokio::spawn(async move {
            if let Ok((mut s, _)) = listener.accept().await {
                s.write_all(b"220 BANNER\r\n").await.ok();
                s.flush().await.ok();
                let mut buf = vec![0u8; REQUEST.len()];
                if s.read_exact(&mut buf).await.is_ok() {
                    *got_be.lock().await = buf;
                }
                s.write_all(b"250 OK\r\n").await.ok();
                s.flush().await.ok();
            }
        });

        let handle = mk_handle("backend", backend_addr).await;
        let proxy: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));
        let chain: Arc<dyn ConnectionHandler> = Arc::new(ChainHandler::new(
            vec![Arc::new(AllowAll) as Arc<dyn NetworkFilter>],
            proxy,
        ));
        let dl = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let daddr = dl.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (s, _) = dl.accept().await.unwrap();
            chain.handle(s).await
        });

        let mut client = TcpStream::connect(daddr).await.unwrap();
        // Read the banner (server-first), then send the ALLOWED first byte + payload.
        let mut banner = [0u8; 12];
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            client.read_exact(&mut banner),
        )
        .await
        .expect("banner within 3s")
        .expect("read banner");
        assert_eq!(&banner, b"220 BANNER\r\n");
        client.write_all(REQUEST).await.expect("send request");
        client.flush().await.ok();
        // Read the backend's response back through the proxy.
        let mut resp = [0u8; 8];
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            client.read_exact(&mut resp),
        )
        .await
        .expect("response within 3s")
        .expect("read response");
        assert_eq!(&resp, b"250 OK\r\n");
        drop(client);
        let _ = task.await;
        // The re-injected first byte + payload reached the backend intact & in order.
        assert_eq!(
            got.lock().await.as_slice(),
            REQUEST,
            "re-inject + payload must reach the backend byte-exact and in order"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_payload_end_to_end() {
        let upstream_addr = spawn_echo().await;
        let handle = mk_handle("backend", upstream_addr).await;
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

        let handle = mk_handle("backend", upstream_addr).await;
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

        let handle = mk_handle("backend", upstream_addr).await;
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
        let handle = mk_handle("backend", refused).await;
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
        use rustls::pki_types::ServerName;
        use std::convert::TryFrom;

        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        // Plaintext upstream echo backend.
        let upstream_addr = spawn_echo().await;

        // rcgen-built CA + leaf with SAN `localhost` for the in-test TLS pair.
        let pki = build_upstream_pki("localhost");
        let acceptor = acceptor_from_pki(&pki);
        let connector = client_connector_for(&pki);

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");

        let handle = mk_handle("backend", upstream_addr).await;
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
        let handle = mk_handle("backend", upstream_addr).await;
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
        let pki = build_upstream_pki("localhost");
        let acceptor = acceptor_from_pki(&pki);
        let connector = client_connector_for(&pki);

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");

        let handle = mk_handle("backend", upstream_addr).await;
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
        use rustls::pki_types::ServerName;
        use std::convert::TryFrom;

        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let refused: SocketAddr = "127.0.0.1:1".parse().unwrap();

        let pki = build_upstream_pki("localhost");
        let acceptor = acceptor_from_pki(&pki);
        let connector = client_connector_for(&pki);

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");

        let handle = mk_handle("backend", refused).await;
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

    // ---------------------------------------------------------------------
    // 03.2 Task 5: upstream-TLS dial tests.
    //
    // The 3 tests below exercise `TcpProxy::with_upstream_tls` + the
    // branched dial body. PKI is set up inline per existing 03.1 cadence
    // (no shared `test_pki` module). The CA PEM is materialized to a
    // `TempDir`-owned file so `UpstreamTlsContext.validation_context.trusted_ca`
    // (which holds a filesystem path) can resolve it.
    // ---------------------------------------------------------------------

    /// Helper struct: rcgen-built single-leaf PKI with the CA written to a
    /// TempDir-owned file at `ca_pem_path`. The `TempDir` is held in `_tmpdir`
    /// to keep the directory alive for the test's lifetime.
    struct UpstreamPki {
        leaf_der: rustls::pki_types::CertificateDer<'static>,
        leaf_key: rustls::pki_types::PrivateKeyDer<'static>,
        ca_pem_path: std::path::PathBuf,
        _tmpdir: tempfile::TempDir,
    }

    fn build_upstream_pki(leaf_san: &str) -> UpstreamPki {
        use rcgen::{CertificateParams, KeyPair};

        let mut ca_params = CertificateParams::new(vec!["test-ca".into()]).expect("ca params");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_kp = KeyPair::generate().expect("ca kp");
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca");

        let leaf_params = CertificateParams::new(vec![leaf_san.into()]).expect("leaf params");
        let leaf_kp = KeyPair::generate().expect("leaf kp");
        let leaf_cert = leaf_params
            .signed_by(&leaf_kp, &ca_cert, &ca_kp)
            .expect("leaf");
        let leaf_der: rustls::pki_types::CertificateDer<'static> =
            leaf_cert.der().clone().into_owned();

        let leaf_key_pem = leaf_kp.serialize_pem();
        let mut key_slice = leaf_key_pem.as_bytes();
        let leaf_key = rustls_pemfile::private_key(&mut key_slice)
            .expect("priv key parse")
            .expect("priv key present");

        let tmpdir = tempfile::tempdir().expect("tmpdir");
        let ca_pem_path = tmpdir.path().join("ca.pem");
        std::fs::write(&ca_pem_path, ca_cert.pem()).expect("write ca pem");

        UpstreamPki {
            leaf_der,
            leaf_key,
            ca_pem_path,
            _tmpdir: tmpdir,
        }
    }

    fn upstream_ctx_for(pki: &UpstreamPki, sni: &str) -> envoy_config::UpstreamTlsContext {
        envoy_config::UpstreamTlsContext {
            common_tls_context: envoy_config::CommonTlsContext {
                tls_certificates: vec![],
                validation_context: Some(envoy_config::CertificateValidationContext {
                    trusted_ca: envoy_config::DataSource {
                        filename: Some(pki.ca_pem_path.to_string_lossy().into_owned()),
                        inline_string: None,
                    },
                }),
            },
            sni: sni.to_string(),
        }
    }

    /// Build a `tokio_rustls::TlsAcceptor` for the leaf in `pki`, with an
    /// optional `ResolvesServerCert` override (used by the SNI-capture test).
    fn acceptor_from_pki(pki: &UpstreamPki) -> tokio_rustls::TlsAcceptor {
        let signing = rustls::crypto::aws_lc_rs::sign::any_supported_type(&pki.leaf_key)
            .expect("any_supported_type");
        let certified = Arc::new(rustls::sign::CertifiedKey::new(
            vec![pki.leaf_der.clone()],
            signing,
        ));

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
        tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg))
    }

    /// Build a `tokio_rustls::TlsConnector` whose root store trusts `pki`'s
    /// CA (read back from the TempDir-materialized `ca_pem_path`). Used by
    /// the downstream-TLS tests as the in-test client side of the pair.
    fn client_connector_for(pki: &UpstreamPki) -> tokio_rustls::TlsConnector {
        let mut roots = rustls::RootCertStore::empty();
        let ca_pem = std::fs::read(&pki.ca_pem_path).expect("read ca pem");
        let mut slice = ca_pem.as_slice();
        for cert in rustls_pemfile::certs(&mut slice) {
            roots.add(cert.expect("parse ca cert")).expect("root add");
        }
        let client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        tokio_rustls::TlsConnector::from(Arc::new(client_cfg))
    }

    /// 03.2 Task 5 (test 1): byte-exact round-trip via a TLS upstream when
    /// the client's trust bundle accepts the upstream's leaf cert.
    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_to_tls_upstream_with_valid_cert() {
        let _ = envoy_tls::install_default_crypto_provider();

        let pki = build_upstream_pki("envoy-rust.test");
        let acceptor = acceptor_from_pki(&pki);

        // In-process TLS echo backend (inline — `spawn_tls_echo` is unused
        // because we drive split/reunite directly here for clarity).
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let upstream_addr = upstream_listener.local_addr().expect("local_addr");
        let acceptor_clone = acceptor.clone();
        tokio::spawn(async move {
            if let Ok((stream, _)) = upstream_listener.accept().await
                && let Ok(tls) = acceptor_clone.accept(stream).await
            {
                let (mut r, mut w) = tokio::io::split(tls);
                let _ = tokio::io::copy(&mut r, &mut w).await;
            }
        });

        let upstream_ctx = upstream_ctx_for(&pki, "envoy-rust.test");
        let upstream_tls =
            Arc::new(envoy_tls::UpstreamTls::from_context(&upstream_ctx).expect("upstream tls"));

        let cluster = mk_handle("backend", upstream_addr).await;
        let proxy = TcpProxy::with_upstream_tls(cluster, &mk_cfg("backend"), upstream_tls);
        let proxy_arc: Arc<TcpProxy> = Arc::new(proxy);

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_arc_clone = proxy_arc.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            envoy_listener::ConnectionHandler::handle(&*proxy_arc_clone, stream)
                .await
                .expect("handle ok")
        });

        let mut client = TcpStream::connect(downstream_addr).await.expect("connect");
        let payload = b"hello, tls upstream\n";
        client.write_all(payload).await.expect("write");
        let mut buf = vec![0u8; payload.len()];
        client.read_exact(&mut buf).await.expect("read_exact");
        assert_eq!(buf, payload);
        client.shutdown().await.ok();
        drop(client);

        proxy_task.await.expect("proxy task joins");
    }

    /// 03.2 Task 5 (test 2): the proxy's UpstreamTls trust bundle is built
    /// from CA2; the upstream presents a leaf signed by CA1. The handshake
    /// must fail and the proxy must surface a `TcpProxyError::UpstreamTlsHandshake`.
    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_returns_err_on_upstream_tls_handshake_fail() {
        let _ = envoy_tls::install_default_crypto_provider();

        let pki1 = build_upstream_pki("envoy-rust.test");
        let pki2 = build_upstream_pki("envoy-rust.test"); // independent CA
        let acceptor = acceptor_from_pki(&pki1);

        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let upstream_addr = upstream_listener.local_addr().expect("local_addr");
        let acceptor_clone = acceptor.clone();
        tokio::spawn(async move {
            if let Ok((stream, _)) = upstream_listener.accept().await {
                // Server-side handshake will fail because the client aborts
                // on cert-verification failure; ignore the error here.
                let _ = acceptor_clone.accept(stream).await;
            }
        });

        // Trust bundle = pki2's CA — independent of pki1's CA that signed
        // the upstream's leaf.
        let upstream_ctx = upstream_ctx_for(&pki2, "envoy-rust.test");
        let upstream_tls =
            Arc::new(envoy_tls::UpstreamTls::from_context(&upstream_ctx).expect("upstream tls"));

        let cluster = mk_handle("backend", upstream_addr).await;
        let proxy = TcpProxy::with_upstream_tls(cluster, &mk_cfg("backend"), upstream_tls);
        let proxy_arc: Arc<TcpProxy> = Arc::new(proxy);

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_arc_clone = proxy_arc.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            envoy_listener::ConnectionHandler::handle(&*proxy_arc_clone, stream).await
        });

        let _client = TcpStream::connect(downstream_addr).await.expect("connect");
        let result = proxy_task.await.expect("proxy task joins");
        let err = result.expect_err("upstream tls handshake must fail");
        let formatted = format!("{err}");
        assert!(
            formatted.contains("upstream TLS handshake")
                || formatted.contains("UpstreamTlsHandshake")
                || formatted.to_lowercase().contains("certificate"),
            "expected UpstreamTlsHandshake-shaped error, got: {formatted}",
        );

        // Also assert the underlying error variant matches the new
        // TcpProxyError::UpstreamTlsHandshake constructor.
        let downcast = err
            .downcast_ref::<TcpProxyError>()
            .expect("err must downcast to TcpProxyError");
        assert!(
            matches!(downcast, TcpProxyError::UpstreamTlsHandshake { .. }),
            "expected TcpProxyError::UpstreamTlsHandshake, got: {downcast:?}",
        );
    }

    /// 03.2 Task 5 (test 3): the SNI sent in the ClientHello matches
    /// `UpstreamTlsContext.sni`. Uses a custom `ResolvesServerCert` impl that
    /// captures `client_hello.server_name()` into a Mutex before delegating
    /// to a normal CertifiedKey resolver.
    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_to_tls_upstream_sends_sni_in_client_hello() {
        use std::sync::Mutex;

        let _ = envoy_tls::install_default_crypto_provider();

        let pki = build_upstream_pki("envoy-rust.test");

        let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let signing = rustls::crypto::aws_lc_rs::sign::any_supported_type(&pki.leaf_key)
            .expect("any_supported_type");
        let certified = Arc::new(rustls::sign::CertifiedKey::new(
            vec![pki.leaf_der.clone()],
            signing,
        ));

        #[derive(Debug)]
        struct CapturingResolver {
            inner: Arc<rustls::sign::CertifiedKey>,
            captured: Arc<Mutex<Option<String>>>,
        }
        impl rustls::server::ResolvesServerCert for CapturingResolver {
            fn resolve(
                &self,
                hello: rustls::server::ClientHello<'_>,
            ) -> Option<Arc<rustls::sign::CertifiedKey>> {
                let sni = hello.server_name().map(|s| s.to_string());
                if let Ok(mut guard) = self.captured.lock() {
                    *guard = sni;
                }
                Some(self.inner.clone())
            }
        }

        let resolver = Arc::new(CapturingResolver {
            inner: certified,
            captured: captured.clone(),
        });
        let server_cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(resolver as Arc<_>);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let upstream_addr = upstream_listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            if let Ok((stream, _)) = upstream_listener.accept().await
                && let Ok(tls) = acceptor.accept(stream).await
            {
                let (mut r, mut w) = tokio::io::split(tls);
                let _ = tokio::io::copy(&mut r, &mut w).await;
            }
        });

        let upstream_ctx = upstream_ctx_for(&pki, "envoy-rust.test");
        let upstream_tls =
            Arc::new(envoy_tls::UpstreamTls::from_context(&upstream_ctx).expect("upstream tls"));

        let cluster = mk_handle("backend", upstream_addr).await;
        let proxy = TcpProxy::with_upstream_tls(cluster, &mk_cfg("backend"), upstream_tls);
        let proxy_arc: Arc<TcpProxy> = Arc::new(proxy);

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_arc_clone = proxy_arc.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            envoy_listener::ConnectionHandler::handle(&*proxy_arc_clone, stream)
                .await
                .expect("handle ok")
        });

        let mut client = TcpStream::connect(downstream_addr).await.expect("connect");
        client.write_all(b"sni-probe").await.expect("write");
        let mut buf = [0u8; 9];
        client.read_exact(&mut buf).await.expect("read_exact");
        client.shutdown().await.ok();
        drop(client);

        proxy_task.await.expect("proxy task joins");

        let captured_sni = captured.lock().expect("captured lock").clone();
        assert_eq!(
            captured_sni.as_deref(),
            Some("envoy-rust.test"),
            "expected SNI 'envoy-rust.test' in ClientHello, got {captured_sni:?}",
        );
    }
}
