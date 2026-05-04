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
use std::time::Instant;
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
                // Connection-level error per h2 docs; the listener logs the
                // wrapped Http2Error::H2StreamAccept on return. Avoiding a
                // tracing::warn! here eliminates double-logging on the noisy
                // peer-reset path.
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
        BuildOutcome::Proxy {
            cluster: cluster_name,
        } => {
            // SPEC §3 D4 H2-side: symmetric H1-or-H2 dispatch keyed on
            // cluster.upstream_protocol(). The validator ensures every cluster
            // name referenced from a RouteAction::Route exists in the
            // bootstrap; the .expect() is defense-in-depth (mirrors
            // envoy-http1/src/hcm.rs:215-218).
            let cluster = config
                .cluster_mgr
                .get(&cluster_name)
                .expect("validator ensures cluster present");

            let endpoint = match cluster.pick_endpoint() {
                Some(e) => e,
                None => {
                    tracing::warn!(cluster = %cluster.name(), "no healthy endpoint — emitting 502");
                    return send_envoy_response(send_response, synth_h2_502()).await;
                }
            };

            // Extract Host: from the synthesized envoy_req. http_to_envoy_request
            // always synthesizes host from :authority at the bottom of headers
            // (per SPEC §6 signpost 12 + request.rs line 74), so the .expect()
            // is effectively infallible here.
            let host_header = envoy_req
                .headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("host"))
                .map(|(_, v)| v.clone())
                .expect("http_to_envoy_request always synthesizes Host from :authority");

            // Build the outbound request: strip H1 hop-by-hop headers (Connection,
            // Transfer-Encoding) mirroring envoy-http1/src/hcm.rs:244-248.
            let mut out_headers = envoy_req.headers.clone();
            out_headers.retain(|(n, _)| {
                !n.eq_ignore_ascii_case("connection")
                    && !n.eq_ignore_ascii_case("transfer-encoding")
            });
            let out_req = envoy_http1::codec::Request {
                method: envoy_req.method.clone(),
                path: envoy_req.path.clone(),
                version: envoy_http1::codec::HttpVersion::Http11,
                headers: out_headers,
                bytes_consumed: 0,
                body: envoy_req.body.clone(),
            };

            let start = Instant::now();
            let upstream_resp_result = match cluster.upstream_protocol() {
                envoy_cluster::UpstreamProtocol::Http1 => {
                    match envoy_http1::Client::connect(endpoint, &host_header).await {
                        Ok(mut s) => s.send_request(out_req).await.map_err(|e| format!("{e}")),
                        Err(e) => Err(format!("{e}")),
                    }
                }
                envoy_cluster::UpstreamProtocol::Http2 => {
                    match crate::Client::connect(endpoint, &host_header).await {
                        Ok(mut s) => s.send_request(out_req).await.map_err(|e| format!("{e}")),
                        Err(e) => Err(format!("{e}")),
                    }
                }
            };

            let upstream_resp = match upstream_resp_result {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(error = %e, "H2 listener: upstream dispatch failed — emitting 502");
                    return send_envoy_response(send_response, synth_h2_502()).await;
                }
            };

            // Append x-envoy-upstream-service-time per parent §6 signpost 10.
            let elapsed_ms = start.elapsed().as_millis();
            let mut headers = upstream_resp.headers;
            headers.push((
                "x-envoy-upstream-service-time".to_string(),
                elapsed_ms.to_string(),
            ));
            Response {
                status: upstream_resp.status,
                reason: upstream_resp.reason,
                headers,
                body: upstream_resp.body,
            }
        }
    };

    send_envoy_response(send_response, resp).await
}

/// Emit a generic 502 Bad Gateway response with no body. Used by
/// `handle_one_stream` when upstream dispatch fails (no healthy endpoint,
/// connect error, or send_request error). Mirrors the shape of
/// envoy-http1's `synth_status(502, _)` without the H1 Connection:
/// header (H2 has its own connection lifecycle).
fn synth_h2_502() -> Response {
    Response {
        status: 502,
        reason: None,
        headers: vec![
            ("server".to_string(), "envoy-rust".to_string()),
            ("content-type".to_string(), "text/plain".to_string()),
        ],
        body: Bytes::from_static(b""),
    }
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
    use std::net::SocketAddr;
    use std::sync::Arc;

    /// RAII handle that aborts the spawned listener task when dropped. Used
    /// by the per-test `_server` binding to stop per-test task leaks without
    /// requiring a manual `server.abort()` at the end of each test.
    struct TestServer {
        handle: tokio::task::JoinHandle<()>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

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
    /// addr + a `TestServer` RAII guard that aborts the listener task when
    /// dropped. The accept loop runs until the test's `_server` binding falls
    /// out of scope at end-of-test.
    async fn spawn_h2_hcm(config: Arc<Http1HCMConfig>) -> (std::net::SocketAddr, TestServer) {
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
        (addr, TestServer { handle: h })
    }

    /// Build a ClusterManager containing a single STATIC cluster named
    /// "backend" pointing at the given `upstream_addr` with the given
    /// `protocol`. Builds via YAML so that the envoy-cluster `from_bootstrap`
    /// path is exercised (fields on `Cluster` are `pub(crate)`, so direct
    /// construction isn't available cross-crate).
    ///
    /// `Http2` protocol is expressed via `typed_extension_protocol_options`;
    /// `Http1` is expressed by omitting the field (default).
    async fn build_cluster_mgr_with_upstream(
        upstream_addr: SocketAddr,
        protocol: envoy_cluster::UpstreamProtocol,
    ) -> Arc<envoy_cluster::ClusterManager> {
        let yaml = if protocol == envoy_cluster::UpstreamProtocol::Http2 {
            format!(
                r#"
node: {{ id: x, cluster: y }}
admin: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: 9901 }} }} }}
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {addr}
                      port_value: {port}
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options:
              max_concurrent_streams: 100
"#,
                addr = upstream_addr.ip(),
                port = upstream_addr.port(),
            )
        } else {
            format!(
                r#"
node: {{ id: x, cluster: y }}
admin: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: 9901 }} }} }}
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {addr}
                      port_value: {port}
"#,
                addr = upstream_addr.ip(),
                port = upstream_addr.port(),
            )
        };
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("parse bootstrap");
        Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap)
                .await
                .expect("from_bootstrap"),
        )
    }

    /// Build a minimal HCM config whose single route proxies everything to
    /// the "backend" cluster. The caller supplies the ClusterManager so both
    /// H1- and H2-cluster variants can reuse this helper.
    fn synth_h2_hcm_config_proxy(
        cluster_mgr: Arc<envoy_cluster::ClusterManager>,
    ) -> Arc<Http1HCMConfig> {
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test-proxy".to_string(),
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
        Arc::new(Http1HCMConfig::from_config(&cfg, cluster_mgr).expect("build HCM config"))
    }

    /// Spawn an in-process H2 server that responds to the first accepted
    /// request with 200 and a fixed body. Returns the bound addr + a
    /// `JoinHandle` that is the server's lifecycle.
    async fn spawn_upstream_h2_server(
        body: &'static [u8],
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (tcp, _peer) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut conn = match h2::server::handshake(tcp).await {
                Ok(c) => c,
                Err(_) => return,
            };
            while let Some(result) = conn.accept().await {
                let (_req, mut send_response) = match result {
                    Ok(p) => p,
                    Err(_) => return,
                };
                let resp = http::Response::builder().status(200).body(()).unwrap();
                let mut send_stream = match send_response.send_response(resp, false) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let _ = send_stream.send_data(bytes::Bytes::from_static(body), true);
            }
        });
        (addr, handle)
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
    async fn h2_proxy_outcome_dispatches_to_upstream() {
        // SPEC §3 D4 H2-side: H2 listener with an H2-cluster upstream.
        // Spawns an in-process H2 upstream returning 200 "h2-upstream-ok",
        // wires the HCM to proxy the "backend" cluster there, drives a GET /
        // through the HCM, and asserts 200 with body "h2-upstream-ok".
        let (upstream_addr, _upstream_handle) = spawn_upstream_h2_server(b"h2-upstream-ok").await;
        let cluster_mgr =
            build_cluster_mgr_with_upstream(upstream_addr, envoy_cluster::UpstreamProtocol::Http2)
                .await;
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config_proxy(cluster_mgr)).await;
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
        assert_eq!(resp.status().as_u16(), 200);
        // Drain body and assert.
        let (_parts, mut body) = resp.into_parts();
        let mut body_bytes = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            body_bytes.extend_from_slice(&chunk);
            let _ = body.flow_control().release_capacity(chunk.len());
        }
        assert_eq!(body_bytes.as_ref(), b"h2-upstream-ok");
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
    async fn h2_proxy_outcome_dispatches_to_h1_upstream_when_cluster_is_http1() {
        // Per SPEC §3 D4 test 5: H2 listener-side HCM with a cluster of
        // upstream_protocol: Http1 dispatches via envoy_http1::Client.
        // Uses a minimal ad-hoc H1 server (raw TCP write) since the
        // tests/helpers/http1-echo-server isn't usable from a unit test.
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        // Spawn a minimal H1 server that returns 200 with body "h1-from-h2-listener".
        // Content-Length must match the body exactly (19 bytes).
        let _upstream_handle = tokio::spawn(async move {
            if let Ok((mut tcp, _)) = upstream_listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = tcp.read(&mut buf).await;
                let _ = tcp
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 19\r\n\r\nh1-from-h2-listener")
                    .await;
                let _ = tcp.shutdown().await;
            }
        });
        let cluster_mgr =
            build_cluster_mgr_with_upstream(upstream_addr, envoy_cluster::UpstreamProtocol::Http1)
                .await;
        let (listener_addr, _hcm) = spawn_h2_hcm(synth_h2_hcm_config_proxy(cluster_mgr)).await;
        let tcp = tokio::net::TcpStream::connect(listener_addr).await.unwrap();
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
            200,
            "H2 listener with H1 cluster must proxy to the H1 upstream and return 200"
        );
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
