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
use envoy_http1::{BuildOutcome, HCMConfig as Http1HCMConfig, Request, Response, build_response};
use envoy_listener::{BoxFuture, ConnectionHandler};
use std::sync::Arc;
use std::time::{Instant, SystemTime};
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
    // 06.1 D4.c: per-request entry-path counter (mirrors envoy-http1's
    // increment in `serve_connection`). Per SPEC §6 signpost 5 the
    // increment fires at the entry of the per-stream handler, BEFORE the
    // route walk + dispatch. Counts attempts including malformed bodies.
    config.stats.downstream_rq_total.inc();

    // 06.2 Task 7: capture arrival time-points before the body drain so
    // the access-log's %START_TIME% and %DURATION% tokens cover the full
    // per-stream lifecycle (mirrors envoy-http1::serve_connection).
    let req_arrival_instant = Instant::now();
    let req_arrival_systime = SystemTime::now();

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
    let request_body_len: u64 = body_bytes.len() as u64;
    let req_with_body = http::Request::from_parts(parts, body_bytes.freeze());

    // Translate H2 request → envoy Request value-type.
    let envoy_req = http_to_envoy_request(req_with_body)?;

    // Hand to the existing 04.x route-walk. close=false because H2 has its
    // own connection lifecycle; the close flag is only meaningful for H1.
    let outcome = build_response(&config, &envoy_req, /* close = */ false);

    // 06.2 Task 7: per-stream access-log state. Populated below as the
    // build/proxy dispatch resolves the final downstream response. The
    // `upstream_host_for_log_h2` variable is `None` on synth + picker-None
    // paths and is set to the resolved endpoint on the successful proxy
    // path before any upstream IO is attempted.
    let response_status_for_log: u16;
    let response_body_len: u64;
    let response_headers_for_log: Vec<(String, String)>;
    let mut upstream_host_for_log_h2: Option<String> = None;

    let resp: Response = match outcome {
        BuildOutcome::Synth(r) => {
            response_status_for_log = r.status;
            response_body_len = r.body.len() as u64;
            response_headers_for_log = r.headers.clone();
            r
        }
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
                    let r = synth_h2_502();
                    response_status_for_log = r.status;
                    response_body_len = r.body.len() as u64;
                    response_headers_for_log = r.headers.clone();
                    // Funnel through the unified send + access-log
                    // dispatch site at the bottom of handle_one_stream.
                    return finalize_h2_stream(
                        &config,
                        send_response,
                        r,
                        req_arrival_instant,
                        req_arrival_systime,
                        &envoy_req,
                        request_body_len,
                        response_status_for_log,
                        response_body_len,
                        &response_headers_for_log,
                        upstream_host_for_log_h2,
                    )
                    .await;
                }
            };

            // 06.2 Task 7: capture the resolved upstream endpoint
            // for the access-log `%UPSTREAM_HOST%` token.
            upstream_host_for_log_h2 = Some(endpoint.to_string());

            // Extract Host: from the synthesized envoy_req.
            // http_to_envoy_request always synthesizes host from
            // :authority at the bottom of headers (per SPEC §6
            // signpost 12 + request.rs line 74), so the .expect()
            // is effectively infallible here.
            let host_header = envoy_req
                .headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("host"))
                .map(|(_, v)| v.clone())
                .expect("http_to_envoy_request always synthesizes Host from :authority");

            // Build the outbound request: strip H1 hop-by-hop headers
            // (Connection, Transfer-Encoding) mirroring
            // envoy-http1/src/hcm.rs:244-248.
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

            // 06.3 D15.3.b: RAII guard increments
            // `cluster.<name>.upstream_cx_active` before either protocol arm
            // connects and decrements via Drop at scope exit, covering both
            // success and error close paths uniformly. A single guard covers
            // both the H1 and H2 arms of the match below.
            let _cx_guard = cluster.cx_active_guard();

            let start = Instant::now();
            let upstream_resp_result = match cluster.upstream_protocol() {
                envoy_cluster::UpstreamProtocol::Http1 => {
                    match envoy_http1::Client::connect(endpoint, &host_header).await {
                        Ok(mut s) => {
                            // 06.1 D4.b: per-cluster upstream_cx_total
                            // increment on successful upstream H1 connect
                            // (mirrors envoy-http1::serve_connection's
                            // proxy arm).
                            cluster.cx_total().inc();
                            s.send_request(out_req).await.map_err(|e| format!("{e}"))
                        }
                        Err(e) => Err(format!("{e}")),
                    }
                }
                envoy_cluster::UpstreamProtocol::Http2 => {
                    match crate::Client::connect(endpoint, &host_header).await {
                        Ok(mut s) => {
                            // 06.1 D4.b: per-cluster upstream_cx_total
                            // increment on successful upstream H2 connect.
                            cluster.cx_total().inc();
                            s.send_request(out_req).await.map_err(|e| format!("{e}"))
                        }
                        Err(e) => Err(format!("{e}")),
                    }
                }
            };

            let upstream_resp = match upstream_resp_result {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(error = %e, "H2 listener: upstream dispatch failed — emitting 502");
                    let r = synth_h2_502();
                    response_status_for_log = r.status;
                    response_body_len = r.body.len() as u64;
                    response_headers_for_log = r.headers.clone();
                    // Funnel through the unified send + access-log
                    // dispatch site at the bottom of handle_one_stream.
                    return finalize_h2_stream(
                        &config,
                        send_response,
                        r,
                        req_arrival_instant,
                        req_arrival_systime,
                        &envoy_req,
                        request_body_len,
                        response_status_for_log,
                        response_body_len,
                        &response_headers_for_log,
                        upstream_host_for_log_h2,
                    )
                    .await;
                }
            };

            // 06.3 D15.3.c: per PLAN-write SPEC correction 3, the H2 router-arm
            // does NOT call write_proxied_response (it builds the downstream
            // Response inline below). Inline 2-line increments parallel the H1
            // path. Both fire on the success arm only (the Err arm returned via
            // finalize_h2_stream above).
            cluster.upstream_rq_total().inc();
            if upstream_resp.status / 100 == 5 {
                cluster.upstream_rq_5xx().inc();
            }

            // Build the downstream response: mirror envoy-http1::router::
            // write_proxied_response's header policy — replace upstream
            // `server` with `server: envoy-rust`; replace or inject `date`
            // with a fresh IMF-fixdate; append x-envoy-upstream-service-time.
            // The H2 forbidden hop-by-hop headers (connection, transfer-encoding,
            // etc.) are stripped later by build_http_response in response.rs.
            let elapsed_ms = start.elapsed().as_millis();
            let now_date = envoy_http1::date::format_imf_fixdate(SystemTime::now());
            let mut headers: Vec<(String, String)> =
                Vec::with_capacity(upstream_resp.headers.len() + 3);
            let mut saw_server = false;
            let mut saw_date = false;
            for (name, value) in upstream_resp.headers.into_iter() {
                let lc = name.to_ascii_lowercase();
                if lc == "server" {
                    saw_server = true;
                    headers.push(("server".to_string(), "envoy-rust".to_string()));
                } else if lc == "date" {
                    saw_date = true;
                    headers.push(("date".to_string(), now_date.clone()));
                } else {
                    headers.push((lc, value));
                }
            }
            if !saw_server {
                headers.push(("server".to_string(), "envoy-rust".to_string()));
            }
            if !saw_date {
                headers.push(("date".to_string(), now_date));
            }
            headers.push((
                "x-envoy-upstream-service-time".to_string(),
                elapsed_ms.to_string(),
            ));
            let proxy_resp = Response {
                status: upstream_resp.status,
                reason: upstream_resp.reason,
                headers,
                body: upstream_resp.body,
            };
            response_status_for_log = proxy_resp.status;
            response_body_len = proxy_resp.body.len() as u64;
            response_headers_for_log = proxy_resp.headers.clone();
            proxy_resp
        }
    };

    finalize_h2_stream(
        &config,
        send_response,
        resp,
        req_arrival_instant,
        req_arrival_systime,
        &envoy_req,
        request_body_len,
        response_status_for_log,
        response_body_len,
        &response_headers_for_log,
        upstream_host_for_log_h2,
    )
    .await
}

/// 06.2 Task 7: factored per-stream finalization — sends the
/// downstream response via `send_envoy_response`, then (if the HCM
/// config carries access-log sinks) builds an `AccessLogRecord` and
/// emits it once per sink. Mirrors the H1 factored join-point at
/// `envoy_http1::serve_connection`'s tail.
///
/// Per PLAN-write SPEC correction 2 the access-log dispatch lands
/// AFTER `send_envoy_response` returns (covers both the empty-body
/// `send_response(.., end_of_stream=true)` branch and the non-empty
/// `send_data(.., end_of_stream=true)` branch uniformly).
#[allow(clippy::too_many_arguments)]
async fn finalize_h2_stream(
    config: &Arc<HCMConfig>,
    send_response: h2::server::SendResponse<Bytes>,
    resp: Response,
    req_arrival_instant: Instant,
    req_arrival_systime: SystemTime,
    envoy_req: &Request,
    request_body_len: u64,
    response_status_for_log: u16,
    response_body_len: u64,
    response_headers_for_log: &[(String, String)],
    upstream_host_for_log_h2: Option<String>,
) -> Result<(), Http2Error> {
    let send_result = send_envoy_response(send_response, resp).await;

    // 06.3 D15.3.a NEW — symmetric per-response-class HCM counter increment
    // on the H2 path. Uses the `response_status_for_log` parameter already
    // threaded through finalize_h2_stream from each H2 writer arm. The
    // `envoy_http2::HCMConfig` type alias makes `config.stats.downstream_rq_Nxx`
    // resolve via the envoy_http1::HCMStats struct.
    match response_status_for_log / 100 {
        2 => config.stats.downstream_rq_2xx.inc(),
        3 => config.stats.downstream_rq_3xx.inc(),
        4 => config.stats.downstream_rq_4xx.inc(),
        5 => config.stats.downstream_rq_5xx.inc(),
        _ => {}
    }

    // 06.2: per-stream access-log dispatch on the H2 path. Mirrors the
    // H1 factored join-point per parent-06 SPEC §3 D3.2 + PLAN-write
    // SPEC correction 2. Lands AFTER send_envoy_response returns
    // (covering both empty-body and non-empty-body emit branches).
    if !config.access_log.is_empty() {
        let duration = req_arrival_instant.elapsed();
        let record = envoy_accesslog::AccessLogRecord {
            start_time: req_arrival_systime,
            method: envoy_req.method.clone(),
            path: x_envoy_original_path_or_path(envoy_req).to_owned(),
            protocol: "HTTP/2".to_owned(),
            response_code: response_status_for_log,
            response_flags: "-".to_owned(),
            bytes_received: request_body_len,
            bytes_sent: response_body_len,
            duration,
            upstream_service_time: extract_upstream_service_time(response_headers_for_log),
            forwarded_for: access_log_header_value(&envoy_req.headers, "x-forwarded-for"),
            user_agent: access_log_header_value(&envoy_req.headers, "user-agent"),
            request_id: access_log_header_value(&envoy_req.headers, "x-request-id"),
            authority: access_log_header_value(&envoy_req.headers, "host"),
            upstream_host: upstream_host_for_log_h2,
        };
        // 06.3 D15.3.e NEW: symmetric access-log counters on the H2 path.
        // Counter::add(N) per 06.1 REVIEW §7 R-8; fires BEFORE the per-sink
        // await so failures do NOT deflate access_logs_total (parent SPEC §6
        // Rule 4 fire-and-forget posture).
        config
            .stats
            .access_logs_total
            .add(config.access_log.len() as u64);
        for sink in &config.access_log {
            if let Err(err) = sink.emit(&record).await {
                // 06.3 D15.3.e NEW: count emission failures alongside the warn.
                config.stats.access_logs_failed.inc();
                tracing::warn!(error = ?err, "access log emission failed");
            }
        }
    }

    send_result
}

// 06.2 Task 7 — access-log dispatch helpers. Cloned verbatim from
// `envoy_http1::hcm` (PLAN's recommendation: clone ~30 LoC rather
// than re-export across the crate boundary). Mirrors the
// field-population shape expected by `AccessLogRecord` + Envoy's
// default-format substitutions.

/// Resolve `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%`: returns the value
/// of `x-envoy-original-path` header if present, else the
/// request-target/path. Case-insensitive lookup per RFC 7230.
fn x_envoy_original_path_or_path(req: &Request) -> &str {
    for (name, value) in &req.headers {
        if name.eq_ignore_ascii_case("x-envoy-original-path") {
            return value.as_str();
        }
    }
    req.path.as_str()
}

/// Case-insensitive header-value lookup returning an owned `String`.
fn access_log_header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

/// Parse the upstream's `x-envoy-upstream-service-time` response
/// header (integer milliseconds per envoy-rust's own injection in
/// `router::write_proxied_response` / the H2 proxy arm above) into a
/// `Duration`. Returns `None` when the header is absent or the value
/// isn't a parseable `u64`.
fn extract_upstream_service_time(headers: &[(String, String)]) -> Option<std::time::Duration> {
    let v = access_log_header_value(headers, "x-envoy-upstream-service-time")?;
    let ms: u64 = v.parse().ok()?;
    Some(std::time::Duration::from_millis(ms))
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
    async fn synth_h2_hcm_config() -> Arc<Http1HCMConfig> {
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            // 06.2 Task 5: field added to the schema; access-log wiring
            // lands in Task 7 (H2). Empty here.
            access_log: vec![],
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
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, registry)
                .await
                .expect("build HCM config"),
        )
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
            envoy_cluster::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
                .await
                .expect("from_bootstrap"),
        )
    }

    /// Build a minimal HCM config whose single route proxies everything to
    /// the "backend" cluster. The caller supplies the ClusterManager so both
    /// H1- and H2-cluster variants can reuse this helper.
    async fn synth_h2_hcm_config_proxy(
        cluster_mgr: Arc<envoy_cluster::ClusterManager>,
    ) -> Arc<Http1HCMConfig> {
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test-proxy".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            // 06.2 Task 5: field added to the schema; access-log wiring
            // lands in Task 7 (H2). Empty here.
            access_log: vec![],
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
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, registry)
                .await
                .expect("build HCM config"),
        )
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
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config().await).await;
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
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config().await).await;
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
            // 06.2 Task 5: field added to the schema; access-log wiring
            // lands in Task 7 (H2). Empty here.
            access_log: vec![],
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
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let config = Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, registry)
                .await
                .unwrap(),
        );
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
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config().await).await;
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
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config().await).await;
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
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config_proxy(cluster_mgr).await).await;
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
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config().await).await;
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
        let (listener_addr, _hcm) =
            spawn_h2_hcm(synth_h2_hcm_config_proxy(cluster_mgr).await).await;
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

    /// 06.1 D4.c: per-HCM `downstream_rq_total` increments once per H2
    /// stream handled. Mirrors the H1 unit test in
    /// `envoy_http1::hcm::tests::hcm1_increments_downstream_rq_total_on_request`.
    /// Builds the HCMConfig via the production constructor so the H2
    /// handler reaches the same `Arc<Counter>` the test re-registers via
    /// the shared registry.
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm2_increments_downstream_rq_total_on_request() {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test_h2".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            // 06.2 Task 5: field added to the schema; access-log wiring
            // lands in Task 7 (H2). Empty here.
            access_log: vec![],
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
        let config = Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, Arc::clone(&registry))
                .await
                .expect("HCMConfig builds"),
        );

        let cx_counter = registry
            .register_counter("http.test_h2.downstream_rq_total")
            .expect("counter registers");
        assert_eq!(cx_counter.value(), 0);

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
        assert_eq!(resp.status().as_u16(), 200, "direct response 200");

        // Brief settle so the spawn'd handle_one_stream task's increment
        // is observable from this thread (the increment happens inside a
        // `tokio::spawn` per `serve_h2_connection`).
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            cx_counter.value(),
            1,
            "expected exactly one downstream_rq_total increment per H2 stream",
        );
    }

    // ----- 06.2 Task 7: H2 access-log wiring tests -----

    /// Build an HCMConfig with a single VH `domains: ["*"]`, a single
    /// `/`-prefix DirectResponse route returning 200 `ok\n`, and the
    /// supplied access-log sinks. Mirrors envoy-http1's
    /// `hcm_config_with_access_log` (the field already exists via the
    /// 05.2 D1 type-aliased `HCMConfig`).
    async fn h2_hcm_config_with_access_log(
        sinks: Vec<Arc<envoy_accesslog::FileSink>>,
    ) -> Arc<Http1HCMConfig> {
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "ingress_http_h2".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            // Bypass envoy-config's AccessLog parsing — directly seed
            // the materialized sinks below.
            access_log: vec![],
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
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mut built = Http1HCMConfig::from_config(&cfg, cluster_mgr, registry)
            .await
            .expect("build HCM config");
        built.access_log = sinks;
        Arc::new(built)
    }

    /// Open a FileSink at each path, drive one direct-response GET /
    /// request through an in-process H2 HCM, drop the sinks (forcing
    /// flush at file close), and return the per-sink line contents.
    /// Mirrors envoy-http1's `serve_one_request_with_access_log`.
    async fn serve_one_h2_request_with_access_log(
        paths: &[std::path::PathBuf],
    ) -> Vec<Vec<String>> {
        let mut sinks: Vec<Arc<envoy_accesslog::FileSink>> = Vec::new();
        for p in paths {
            sinks.push(Arc::new(
                envoy_accesslog::FileSink::new(p.clone())
                    .await
                    .expect("open sink"),
            ));
        }
        let config = h2_hcm_config_with_access_log(sinks).await;

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
        let mut body = resp.into_body();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            let _ = body.flow_control().release_capacity(chunk.len());
        }

        // Brief settle: the access-log emit happens inside the
        // server's per-stream `tokio::spawn` (see
        // `serve_h2_connection`), which only writes to the FileSink
        // AFTER `send_envoy_response` returns. 200ms is enough headroom
        // for the spawn to drain + flush its line on a loaded macOS
        // runner; mirrors the H1 test's `sleep(50ms)` posture inflated
        // for the extra H2 codec turn.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let mut result: Vec<Vec<String>> = Vec::new();
        for p in paths {
            let contents = tokio::fs::read_to_string(p).await.unwrap_or_default();
            let lines: Vec<String> = contents.lines().map(str::to_owned).collect();
            result.push(lines);
        }
        result
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_h2_with_file_access_log_writes_one_line_per_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let lines_per_sink =
            serve_one_h2_request_with_access_log(std::slice::from_ref(&path)).await;
        assert_eq!(lines_per_sink.len(), 1);
        assert_eq!(lines_per_sink[0].len(), 1);
        let line = &lines_per_sink[0][0];
        assert!(line.contains("\"GET / HTTP/2\""), "line: {}", line);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_h2_records_protocol_as_http2_on_h2_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let lines_per_sink =
            serve_one_h2_request_with_access_log(std::slice::from_ref(&path)).await;
        let line = &lines_per_sink[0][0];
        assert!(line.contains("HTTP/2"), "line: {}", line);
        assert!(
            !line.contains("HTTP/1.1"),
            "line should not contain HTTP/1.1: {}",
            line
        );
    }

    // ── 06.3 D15.3.c: H2-path upstream_rq_total / upstream_rq_5xx tests ──

    /// 06.3 D15.3.c: H2 proxy path increments `upstream_rq_total` once when
    /// the upstream returns 200. Uses an H2 upstream returning 200 (mirrors
    /// `h2_proxy_outcome_dispatches_to_upstream`). The registry is shared so
    /// the test can re-register the counter to get the same Arc.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_hcm_increments_upstream_rq_total_on_200() {
        let (upstream_addr, _upstream_handle) = spawn_upstream_h2_server(b"ok").await;
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = {
            let yaml = format!(
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
            );
            let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("parse");
            Arc::new(
                envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
                    .await
                    .expect("from_bootstrap"),
            )
        };

        // Re-register by name to get the same Arc (idempotent same-kind contract).
        let rq_total = registry
            .register_counter("cluster.backend.upstream_rq_total")
            .expect("counter registers");
        let rq_5xx = registry
            .register_counter("cluster.backend.upstream_rq_5xx")
            .expect("counter registers");
        assert_eq!(rq_total.value(), 0, "starts at zero");
        assert_eq!(rq_5xx.value(), 0, "starts at zero");

        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config_proxy(cluster_mgr).await).await;
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

        // Drain body to let the stream complete.
        let (_parts, mut body) = resp.into_parts();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            let _ = body.flow_control().release_capacity(chunk.len());
        }

        // Brief settle so the spawned handle_one_stream task's increment is visible.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(rq_total.value(), 1, "upstream_rq_total must be 1 after 200");
        assert_eq!(rq_5xx.value(), 0, "upstream_rq_5xx must stay 0 for 200");
    }

    /// 06.3 D15.3.c: H2 proxy path increments both `upstream_rq_total` and
    /// `upstream_rq_5xx` when the upstream returns 503. Uses a minimal H1
    /// upstream (raw TCP) returning 503 to exercise the 5xx conditional,
    /// wired as an H1-protocol cluster so we can control the response status.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_hcm_increments_upstream_rq_5xx_on_503() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        // Minimal H1 upstream returning 503.
        let _upstream_handle = tokio::spawn(async move {
            loop {
                if let Ok((mut tcp, _)) = upstream_listener.accept().await {
                    let mut buf = [0u8; 4096];
                    let _ = tcp.read(&mut buf).await;
                    let _ = tcp
                        .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    let _ = tcp.shutdown().await;
                }
            }
        });

        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = {
            let yaml = format!(
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
            );
            let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("parse");
            Arc::new(
                envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
                    .await
                    .expect("from_bootstrap"),
            )
        };

        let rq_total = registry
            .register_counter("cluster.backend.upstream_rq_total")
            .expect("counter registers");
        let rq_5xx = registry
            .register_counter("cluster.backend.upstream_rq_5xx")
            .expect("counter registers");
        assert_eq!(rq_total.value(), 0, "starts at zero");
        assert_eq!(rq_5xx.value(), 0, "starts at zero");

        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config_proxy(cluster_mgr).await).await;
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
        assert_eq!(resp.status().as_u16(), 503);

        // Drain body.
        let (_parts, mut body) = resp.into_parts();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            let _ = body.flow_control().release_capacity(chunk.len());
        }

        // Brief settle.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(rq_total.value(), 1, "upstream_rq_total must be 1 after 503");
        assert_eq!(rq_5xx.value(), 1, "upstream_rq_5xx must be 1 after 503");
    }
}
