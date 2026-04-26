use std::sync::Arc;

use envoy_listener::{BoxFuture, ConnectionHandler};
use envoy_tcp::TcpProxy;
use envoy_tls::DownstreamTls;

/// Adapter that runs `DownstreamTls::accept` before delegating to
/// `TcpProxy::handle::<TlsStream<TcpStream>>`. envoy-listener's
/// `ConnectionHandler` trait stays object-safe by keeping the trait method
/// concrete on `TcpStream`; the adapter calls the inner `TcpProxy`'s inherent
/// generic `handle::<S>` method directly via `Arc<TcpProxy>`. See SPEC §6
/// signposts 2 and 3.
pub struct TlsAcceptingHandler {
    pub tls: Arc<DownstreamTls>,
    pub inner: Arc<TcpProxy>,
}

impl ConnectionHandler for TlsAcceptingHandler {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let tls = self.tls.clone();
        let inner = self.inner.clone();
        Box::pin(async move {
            // Per SPEC §6 signpost 12 / parent-SPEC §6 signpost 19: TLS
            // handshake errors propagate via the boxed future's Err arm; the
            // listener's accept loop logs at warn! and drops the connection.
            let post_handshake = tls
                .accept(downstream)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            inner.handle(post_handshake).await
        })
    }
}
