//! Per-connection plaintext HTTP/1.1 client. No pooling; one TCP connection
//! per upstream request (pooling is upstream-robustness-family territory,
//! out of phase 04 per parent SPEC §4 + 04.3 SPEC §4 non-goals).
//!
//! This module is the workspace's SOLE user of `httparse::Response::parse`
//! per parent-04 SPEC §3 cross-sub-phase architectural rule 1. The 04.1
//! codec module uses `httparse::Request::parse`; this module is the only
//! consumer of the response parser.

use crate::error::Http1Error;

/// Per-connection plaintext HTTP/1.1 client. Stateless; the per-stream
/// state lives on `ClientStream`.
pub struct Client;

impl Client {
    /// TCP-connect to `addr`. The `host` value is captured for the eventual
    /// `Host:` header on `send_request`. No bytes are sent during connect;
    /// the caller's first `send_request` is the first wire write.
    ///
    /// Errors: `Http1Error::UpstreamConnect { addr, source }` on any
    /// `tokio::net::TcpStream::connect` failure.
    pub async fn connect(
        addr: std::net::SocketAddr,
        host: &str,
    ) -> Result<ClientStream, Http1Error> {
        let stream = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|source| Http1Error::UpstreamConnect { addr, source })?;
        Ok(ClientStream {
            stream,
            host: host.to_string(),
            buf: bytes::BytesMut::with_capacity(8192),
        })
    }
}

/// Active per-connection state: the underlying TCP stream, the host string
/// captured at connect time (used as the `Host:` header default if the
/// outgoing request doesn't carry one), and a read buffer for the response.
#[derive(Debug)]
#[allow(dead_code)] // 04.3 Task 5: `stream` + `buf` are wired up in Task 6's send_request.
pub struct ClientStream {
    pub(crate) stream: tokio::net::TcpStream,
    pub(crate) host: String,
    pub(crate) buf: bytes::BytesMut,
}

impl ClientStream {
    // 04.3 Task 6: send_request lands here.
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_succeeds_against_in_process_acceptor() {
        // Bind an in-process acceptor on an ephemeral port. Client::connect
        // should TCP-connect cleanly and return a ClientStream.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // Spawn an acceptor that just drops every connection.
        tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
            }
        });
        let stream = Client::connect(addr, "envoy-rust.test")
            .await
            .expect("connect");
        assert_eq!(stream.host, "envoy-rust.test");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_returns_upstream_connect_on_refused_port() {
        // 127.0.0.1:1 is kernel-refused on every Linux box. macOS may differ
        // but the failure mode is still a connect-time io::Error which the
        // map_err arm wraps in UpstreamConnect.
        let addr: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
        let err = Client::connect(addr, "envoy-rust.test")
            .await
            .expect_err("connect must fail");
        match err {
            Http1Error::UpstreamConnect {
                addr: got_addr,
                source,
            } => {
                assert_eq!(got_addr, addr);
                // The exact io::ErrorKind varies by OS (ConnectionRefused on
                // Linux, ConnectionRefused or AddrNotAvailable on macOS); just
                // assert there's some Display output.
                assert!(!source.to_string().is_empty());
            }
            other => panic!("expected UpstreamConnect, got: {other:?}"),
        }
    }
}
