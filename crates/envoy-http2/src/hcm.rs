//! HCM ConnectionHandler impl for downstream H2C listeners. See SPEC §3 D3.
//!
//! The HCM consumes envoy_http1::HCMConfig (re-exported as HCMConfig from
//! envoy-http2's lib.rs for ergonomic naming) and dispatches per-stream
//! through envoy_http1::hcm::build_response, identical to the H1 HCM at
//! envoy_http1::HCM. Only the codec layer at the connection edge differs
//! (h2::server vs. Http1Codec). Per cross-sub-phase architectural rule 2.
//!
//! The trait shape (BoxFuture-returning, NOT async-trait) mirrors the
//! envoy-listener trait at crates/envoy-listener/src/lib.rs:29-34. SPEC §6
//! local signpost 19 mandates this — do NOT introduce async-trait ad-hoc.

use crate::codec::build_h2_server;
use crate::error::Http2Error;
use crate::request::http_to_envoy_request;
use crate::response::send_envoy_response;
use bytes::Bytes;
use envoy_http1::{BuildOutcome, HCMConfig as Http1HCMConfig, Response, build_response};
use envoy_listener::{BoxFuture, ConnectionHandler};
use std::sync::Arc;
use tokio::net::TcpStream;

/// Re-export of envoy_http1::HCMConfig under the envoy-http2 namespace.
/// Per cross-sub-phase architectural rule 2 the configuration is identical
/// across H1 and H2; only runtime dispatch differs.
pub type HCMConfig = Http1HCMConfig;

/// HTTP/2 cleartext (H2C prior-knowledge) HCM. Implements
/// `envoy_listener::ConnectionHandler`.
#[derive(Clone)]
pub struct HCM {
    config: Arc<HCMConfig>,
}

impl HCM {
    pub fn new(config: Arc<HCMConfig>) -> Self {
        Self { config }
    }
}

impl ConnectionHandler for HCM {
    fn handle(
        &self,
        downstream: TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let config = self.config.clone();
        Box::pin(async move {
            serve_h2_connection(config, downstream)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

async fn serve_h2_connection(
    config: Arc<HCMConfig>,
    downstream: TcpStream,
) -> Result<(), Http2Error> {
    // Thread the listener-side Http2ProtocolOptions (carried on HCMConfig per
    // the 05.2 extension) through to the codec's h2::server::Builder.
    let mut h2_conn = build_h2_server(config.http2_protocol_options.as_ref())
        .handshake(downstream)
        .await
        .map_err(|source| Http2Error::H2Handshake { source })?;

    while let Some(result) = h2_conn.accept().await {
        let (req, send_response) = match result {
            Ok(pair) => pair,
            Err(source) => {
                tracing::warn!(error = ?source, "H2 stream accept failed");
                return Err(Http2Error::H2StreamAccept { source });
            }
        };
        let config = config.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_one_stream(config, req, send_response).await {
                tracing::error!(error = ?e, "H2 stream handler failed");
            }
        });
    }
    Ok(())
}

async fn handle_one_stream(
    config: Arc<HCMConfig>,
    req: http::Request<h2::RecvStream>,
    send_response: h2::server::SendResponse<Bytes>,
) -> Result<(), Http2Error> {
    // Drain the body. For 05.2 fixture 0009 (direct_response) the body is
    // empty; the drain is a no-op. For future fixtures with a body, the
    // unbounded drain is per parent §6 signpost 9 (deferred body-budget
    // posture).
    let (parts, mut body) = req.into_parts();
    let mut body_bytes = bytes::BytesMut::new();
    while let Some(chunk) = body.data().await {
        let chunk = chunk.map_err(|source| Http2Error::H2BodyRead { source })?;
        body_bytes.extend_from_slice(&chunk);
        // Release flow-control window for the chunk.
        body.flow_control()
            .release_capacity(chunk.len())
            .map_err(|source| Http2Error::H2BodyRead { source })?;
    }
    let req_with_body = http::Request::from_parts(parts, body_bytes.freeze());

    // Translate H2 request → envoy Request value-type.
    let envoy_req = http_to_envoy_request(req_with_body)?;

    // Hand to the existing 04.x route-walk. close=false because H2 has its
    // own connection lifecycle; the close flag is only meaningful for H1.
    let outcome = build_response(&config, &envoy_req, /* close = */ false);

    let resp: Response = match outcome {
        BuildOutcome::Synth(r) => r,
        BuildOutcome::Proxy { .. } => {
            // 05.2 STUB: the upstream H2 dispatch lands in 05.3 D13.3.
            // Per SPEC §6 local signpost 21: emit a generic 502 with a
            // doctrine-line body; no cluster names or endpoint addresses.
            tracing::warn!(
                "H2 BuildOutcome::Proxy reached at sub-phase 05.2 — upstream H2 dispatch \
                 not yet wired (lands in 05.3); responding 502 Bad Gateway"
            );
            Response {
                status: 502,
                reason: None,
                headers: vec![
                    ("server".to_string(), "envoy-rust".to_string()),
                    ("content-type".to_string(), "text/plain".to_string()),
                ],
                body: Bytes::from_static(b"upstream H2 not yet wired (sub-phase 05.3)\n"),
            }
        }
    };

    send_envoy_response(send_response, resp).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{
        CodecType, DataSource, DirectResponse, HttpConnectionManagerConfig, HttpFilter,
        HttpFilterTypedConfig, Route, RouteAction, RouteAction_Route, RouteConfiguration,
        RouteMatch, RouterConfig, VirtualHost,
    };
    use envoy_http1::HCMConfig as Http1HCMConfig;
    use envoy_listener::ConnectionHandler;
    use std::sync::Arc;

    /// Build a minimal HCM config with a single VH + single direct_response
    /// route (status 200, body "ok\n"). Used by most tests below.
    fn synth_h2_hcm_config() -> Arc<Http1HCMConfig> {
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            route_config: RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                    }],
                }],
            },
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        Arc::new(Http1HCMConfig::from_config(&cfg, cluster_mgr).expect("build HCM config"))
    }

    /// Spawn an HCM-driven accept loop on an ephemeral port; return the bound
    /// addr + a JoinHandle that owns the listener task. The accept loop runs
    /// until the test drops it (the JoinHandle is held implicitly by the
    /// test's `_server` binding).
    async fn spawn_h2_hcm(
        config: Arc<Http1HCMConfig>,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hcm = HCM::new(config);
        let h = tokio::spawn(async move {
            loop {
                let (stream, _peer) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let hcm_clone = hcm.clone();
                tokio::spawn(async move {
                    let _ = hcm_clone.handle(stream).await;
                });
            }
        });
        (addr, h)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_handshake_completes_against_in_process_listener() {
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config()).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.expect("handshake");
        tokio::spawn(async move {
            let _ = conn.await;
        });
        // Trivial probe: send a HEADERS-only GET / and expect a response.
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _stream) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.expect("response");
        assert_eq!(resp.status().as_u16(), 200);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_get_resolves_to_direct_response_synth() {
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config()).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        let mut body = resp.into_body();
        let mut bytes = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            bytes.extend_from_slice(&chunk);
        }
        assert_eq!(&bytes[..], b"ok\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_authority_header_synthesizes_host_for_route_walk() {
        // Build an HCM config with TWO virtual hosts: one matching "test.example"
        // exactly, one catch-all "*". The matching VH responds with body
        // "specific\n"; the catch-all responds with "ok\n". Drive a request
        // with :authority = test.example; assert the matching VH is selected.
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            route_config: RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![
                    VirtualHost {
                        name: "specific".to_string(),
                        domains: vec!["test.example".to_string()],
                        routes: vec![Route {
                            r#match: RouteMatch {
                                prefix: Some("/".to_string()),
                                path: None,
                                headers: vec![],
                            },
                            action: RouteAction::DirectResponse(DirectResponse {
                                status: 200,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("specific\n".to_string()),
                                },
                            }),
                        }],
                    },
                    VirtualHost {
                        name: "catch_all".to_string(),
                        domains: vec!["*".to_string()],
                        routes: vec![Route {
                            r#match: RouteMatch {
                                prefix: Some("/".to_string()),
                                path: None,
                                headers: vec![],
                            },
                            action: RouteAction::DirectResponse(DirectResponse {
                                status: 200,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("ok\n".to_string()),
                                },
                            }),
                        }],
                    },
                ],
            },
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        let config = Arc::new(Http1HCMConfig::from_config(&cfg, cluster_mgr).unwrap());
        let (addr, _server) = spawn_h2_hcm(config).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        let mut body = resp.into_body();
        let mut bytes = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            bytes.extend_from_slice(&chunk);
        }
        assert_eq!(
            &bytes[..],
            b"specific\n",
            ":authority test.example must select the specific VH not the catch-all"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_two_requests_share_one_tcp_connection() {
        // Open ONE h2 client connection; send two GET / on different stream
        // IDs (sequentially via send_request twice). Both must return 200 with
        // body "ok\n", verifying that the HCM accepts multiple streams over
        // the same TCP connection (i.e., does not single-shot close after
        // request 1).
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config()).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });

        for _ in 0..2 {
            let req = http::Request::builder()
                .method("GET")
                .uri("http://test.example/")
                .body(())
                .unwrap();
            let (response_fut, _) = send_request.send_request(req, true).unwrap();
            let resp = response_fut.await.expect("response");
            assert_eq!(resp.status().as_u16(), 200);
            let mut body = resp.into_body();
            let mut bytes = bytes::BytesMut::new();
            while let Some(chunk) = body.data().await {
                let chunk = chunk.unwrap();
                bytes.extend_from_slice(&chunk);
            }
            assert_eq!(&bytes[..], b"ok\n");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_response_strips_hop_by_hop_headers_defensively() {
        // The 04.x H1 synth path emits `connection: keep-alive` (or
        // `connection: close`) on every direct_response synth via
        // synth_direct_response. When that response is serialized over H2 via
        // build_http_response, the H2-forbidden hop-by-hop names (RFC 7540
        // §8.1.2.2) MUST be stripped — defense-in-depth (the h2 codec would
        // also reject them at emission). Verify the strip is observable on
        // the client side.
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config()).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.expect("response");
        for forbidden in &[
            "connection",
            "transfer-encoding",
            "upgrade",
            "keep-alive",
            "proxy-connection",
        ] {
            assert!(
                !resp.headers().contains_key(*forbidden),
                "H2 response must not carry hop-by-hop header `{forbidden}`; got headers {:?}",
                resp.headers()
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_proxy_outcome_returns_502_in_05_2() {
        // Build an HCM whose route action proxies to a non-existent cluster
        // ("backend"); cluster_mgr is empty. The HCM's Proxy arm returns the
        // 05.2 STUB 502 (per SPEC §6 local signpost 21) — the real upstream
        // H2 dispatch lands in 05.3 D13.3.
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            route_config: RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                        }),
                    }],
                }],
            },
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        let config = Arc::new(Http1HCMConfig::from_config(&cfg, cluster_mgr).unwrap());
        let (addr, _server) = spawn_h2_hcm(config).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.expect("response");
        assert_eq!(
            resp.status().as_u16(),
            502,
            "Proxy outcome at 05.2 must return 502 (upstream H2 dispatch lands in 05.3)"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_handshake_fails_on_garbage_preamble() {
        // Connect raw TCP to the HCM port and write a bare HTTP/1.1 request
        // (no H2 PRI preamble). The h2::server handshake must reject; the HCM
        // returns Err(H2Handshake), which propagates to the listener-task
        // shutdown. From the peer's perspective, the read returns 0 bytes
        // (clean FIN) within a small budget. Per parent §6 signpost 13: trust
        // the h2 codec to reject malformed handshakes.
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config()).await;
        let mut tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        tcp.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let mut buf = [0u8; 64];
        let n = tokio::time::timeout(std::time::Duration::from_secs(1), tcp.read(&mut buf))
            .await
            .expect("h2 codec must close the connection within 1s");
        // n is io::Result<usize>; the typical close shape is Ok(0) (clean FIN
        // after GOAWAY) but Err is also acceptable (RST, ECONNRESET on
        // platforms where h2 errors map that way). What matters is that the
        // peer observed connection closure.
        match n {
            Ok(0) => { /* clean FIN — expected */ }
            Ok(other) => {
                // h2 may emit a brief response (e.g. GOAWAY) before close;
                // reading the bytes is fine, but we should not see a full
                // HTTP/1.1 status line.
                let s = std::str::from_utf8(&buf[..other]).unwrap_or("");
                assert!(
                    !s.starts_with("HTTP/"),
                    "garbage preamble must not yield an HTTP/1 response, got: {s:?}"
                );
            }
            Err(_) => { /* RST / ECONNRESET — also expected */ }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "h2-crate client-side observability of peer SETTINGS_MAX_CONCURRENT_STREAMS \
                is not deterministically surfaced via the public h2::client API at h2-0.4 \
                without racing the response loop. Plan permits #[ignore] per Step 9.5; \
                the codec-side smoke test in codec.rs::build_h2_server_applies_protocol_options \
                already covers the configuration-edge of the same setter (max_concurrent_streams)"]
    async fn h2_protocol_options_max_concurrent_streams_applied() {
        // Intentionally unimplemented — see #[ignore] reason above. When h2
        // adds a stable observability surface for peer SETTINGS, this test
        // expands to: open client, observe SETTINGS_MAX_CONCURRENT_STREAMS=1,
        // attempt 2 concurrent send_request calls, assert the second blocks
        // until the first stream closes.
    }
}
