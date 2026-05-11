//! HTTP connection manager: per-listener config, per-connection state machine,
//! route walker, hardcoded router-filter call site.

use crate::client::Client;
use crate::codec::{Http1Codec, HttpVersion, Request};
use crate::date::format_imf_fixdate;
use crate::error::Http1Error;
use crate::headers::{self, find_header};
use crate::response::{Http1Response, Response};

use bytes::{Buf, Bytes, BytesMut};
use envoy_config::{
    DataSource, DirectResponse, HttpConnectionManagerConfig, Route, RouteAction, RouteAction_Route,
    RouteConfiguration, RouteMatch, VirtualHost,
};
use envoy_listener::{BoxFuture, ConnectionHandler};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

const DEFAULT_SERVER_NAME: &str = "envoy-rust";
const DEFAULT_CONTENT_TYPE: &str = "text/plain";
const IDLE_READ_TIMEOUT: Duration = Duration::from_secs(5);
const READ_BUFFER_INITIAL_CAPACITY: usize = 8192;

/// 06.1 D4.c: per-HCM counters registered against the global StatsRegistry.
/// Names use the configured `stat_prefix` from `HCMConfig`. Currently
/// carries the single representative counter `downstream_rq_total`; future
/// HCM stats (downstream_rq_2xx, downstream_rq_4xx, etc.) join this struct
/// in 06.3 per parent SPEC §3 D14.3.
///
/// The struct lives in `envoy-http1` because that's the `HCMConfig` owner
/// per cross-sub-phase rule 2; `envoy-http2`'s HCM reaches the same Arc
/// via the re-exported `HCMConfig` type alias.
#[derive(Debug)]
pub struct HCMStats {
    /// `http.<stat_prefix>.downstream_rq_total` — incremented once per
    /// HCM-handled request (any response code; any method) at the entry
    /// path per SPEC §6 signpost 5. Counts attempts including malformed
    /// requests (the increment fires after request-head parsing succeeds
    /// but BEFORE the route walk dispatches to direct_response or proxy).
    pub downstream_rq_total: Arc<envoy_stats::Counter>,
}

impl HCMStats {
    /// Register the HCM's counters under `http.{stat_prefix}.…` against
    /// the supplied registry. Idempotent for same-kind re-registration
    /// (Task 5 contract); two HCMs with the same `stat_prefix` (a config
    /// error in production but possible in tests) share the same counter
    /// Arc.
    pub fn register(
        registry: &envoy_stats::StatsRegistry,
        stat_prefix: &str,
    ) -> Result<Self, envoy_stats::StatsError> {
        Ok(Self {
            downstream_rq_total: registry
                .register_counter(&format!("http.{stat_prefix}.downstream_rq_total"))?,
        })
    }
}

/// Unified HCM configuration consumed by both the H1 and H2 dispatch paths,
/// per cross-sub-phase architectural rule 2 (one config struct, two codec
/// edges). Built once at startup via `from_config(...)` and shared via
/// `Arc<HCMConfig>` across all connections, regardless of which codec the
/// listener wires up.
///
/// Fields specific to one codec path (e.g., `http2_protocol_options`) are
/// inert on the other path — see per-field doc-comments for the asymmetric
/// consumption rules. The struct lives in `envoy-http1` for historical
/// reasons (it predates the H2 split); `envoy-http2` re-exports it as a
/// type alias so cross-crate readers don't need to understand the layering.
#[derive(Debug)]
pub struct HCMConfig {
    pub stat_prefix: String,
    pub route_config: Arc<RouteConfiguration>,
    pub cluster_mgr: Arc<envoy_cluster::ClusterManager>,
    /// 05.2 NEW: listener-side HTTP/2 protocol options. Ignored on the H1
    /// dispatch path (envoy-http1's HCM doesn't read this); consumed on the
    /// H2 dispatch path (envoy-http2's HCM reads it at handshake time).
    pub http2_protocol_options: Option<envoy_config::Http2ProtocolOptions>,
    /// 06.1 D4.c: per-HCM stats handles. Registered at `from_config` time
    /// and shared across H1 + H2 dispatch (the H2 HCM consumes this same
    /// HCMConfig type-alias per cross-sub-phase rule 2). The increment
    /// site is at the per-request entry path (signpost 5).
    pub stats: Arc<HCMStats>,
}

impl HCMConfig {
    pub fn from_config(
        cfg: &HttpConnectionManagerConfig,
        cluster_mgr: Arc<envoy_cluster::ClusterManager>,
        registry: Arc<envoy_stats::StatsRegistry>,
    ) -> Result<Self, Http1Error> {
        // The validator (envoy-config Task 2) has already enforced shape.
        // This constructor is `Result<>` for forward-compat with 04.3's
        // cluster lookup; the 06.1 stats-registration path is the second
        // failure surface (registry name-collision across kinds).
        let stats = Arc::new(
            HCMStats::register(&registry, &cfg.stat_prefix).map_err(|e| {
                Http1Error::StatsRegistration {
                    stat_prefix: cfg.stat_prefix.clone(),
                    message: e.to_string(),
                }
            })?,
        );
        Ok(Self {
            stat_prefix: cfg.stat_prefix.clone(),
            route_config: Arc::new(clone_route_config(&cfg.route_config)),
            cluster_mgr,
            http2_protocol_options: cfg.http2_protocol_options.clone(),
            stats,
        })
    }
}

fn clone_route_config(rc: &RouteConfiguration) -> RouteConfiguration {
    // envoy-config's RouteConfiguration is not Clone; hand-clone so HCM can
    // hold the data inside an Arc without coupling envoy-config's deriving.
    // (If envoy-config later derives Clone on these types, this helper retires.)
    RouteConfiguration {
        name: rc.name.clone(),
        virtual_hosts: rc
            .virtual_hosts
            .iter()
            .map(|vh| VirtualHost {
                name: vh.name.clone(),
                domains: vh.domains.clone(),
                routes: vh
                    .routes
                    .iter()
                    .map(|r| Route {
                        r#match: RouteMatch {
                            prefix: r.r#match.prefix.clone(),
                            path: r.r#match.path.clone(),
                            headers: r.r#match.headers.clone(),
                        },
                        action: clone_route_action(&r.action),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn clone_route_action(a: &RouteAction) -> RouteAction {
    match a {
        RouteAction::DirectResponse(dr) => RouteAction::DirectResponse(DirectResponse {
            status: dr.status,
            body: DataSource {
                filename: dr.body.filename.clone(),
                inline_string: dr.body.inline_string.clone(),
            },
        }),
        RouteAction::Route(ar) => RouteAction::Route(RouteAction_Route {
            cluster: ar.cluster.clone(),
        }),
    }
}

pub struct HCM {
    pub config: Arc<HCMConfig>,
}

impl ConnectionHandler for HCM {
    fn handle(
        &self,
        downstream: TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let config = self.config.clone();
        Box::pin(async move {
            serve_connection(config, downstream)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

async fn serve_connection(
    config: Arc<HCMConfig>,
    mut downstream: TcpStream,
) -> Result<(), Http1Error> {
    let mut buf = BytesMut::with_capacity(READ_BUFFER_INITIAL_CAPACITY);
    loop {
        // 1. Try parsing what's already in the buffer (for keep-alive
        //    second-and-later requests where bytes from the previous read
        //    may already contain the next request's headers).
        let req = match Http1Codec::parse_request(&buf)? {
            Some(req) => req,
            None => {
                // 2. Need more bytes. Read with idle timeout.
                match tokio::time::timeout(IDLE_READ_TIMEOUT, downstream.read_buf(&mut buf)).await {
                    Ok(Ok(0)) => {
                        // peer closed; clean exit if the buffer is empty.
                        if buf.is_empty() {
                            return Ok(());
                        }
                        return Err(Http1Error::UnexpectedEof);
                    }
                    Ok(Ok(_)) => continue, // re-parse
                    Ok(Err(source)) => return Err(Http1Error::Io { source }),
                    Err(_elapsed) => return Ok(()), // idle timeout → clean close
                }
            }
        };

        // 06.1 D4.c: per-request entry-path counter. Per SPEC §6 signpost 5,
        // the increment fires at the entry of the per-request handler — the
        // first action after request-head parsing succeeds, BEFORE the route
        // walk. Counts attempts including malformed bodies (chunked-rejected,
        // synthesized 4xx). Mirrors the H2-side increment in
        // envoy-http2::hcm::handle_one_stream.
        config.stats.downstream_rq_total.inc();

        // 3. Determine close/keep-alive decision before any move.
        let close = req.headers.iter().any(|(n, v)| {
            n.eq_ignore_ascii_case(headers::CONNECTION) && v.eq_ignore_ascii_case("close")
        }) || req.version == HttpVersion::Http10;

        // 4. Compute body length (for drain) before consuming.
        let body_len = parse_content_length(&req.headers)?;
        let chunked = req.headers.iter().any(|(n, v)| {
            n.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked")
        });

        // 5. Build response (handles 400 / 404 / 501 / 200 internally) or
        //    decide to proxy upstream.
        let outcome = if chunked {
            tracing::warn!(
                method = %req.method,
                path = %req.path,
                "request rejected: Transfer-Encoding: chunked not supported (501)"
            );
            BuildOutcome::Synth(synth_501(close))
        } else {
            build_response(&config, &req, close)
        };

        // 6. Advance the buffer past the consumed request + body.
        let consumed = req.bytes_consumed;
        buf.advance(consumed);
        // 7. Drain body bytes (read_exact-style; up to body_len).
        let drained_so_far = buf.len().min(body_len);
        buf.advance(drained_so_far);
        let mut remaining = body_len - drained_so_far;
        while remaining > 0 {
            let mut throwaway = [0u8; 4096];
            let to_read = throwaway.len().min(remaining);
            let n = match tokio::time::timeout(
                IDLE_READ_TIMEOUT,
                downstream.read(&mut throwaway[..to_read]),
            )
            .await
            {
                Ok(Ok(0)) => return Err(Http1Error::UnexpectedEof),
                Ok(Ok(n)) => n,
                Ok(Err(source)) => return Err(Http1Error::Io { source }),
                Err(_elapsed) => return Ok(()),
            };
            remaining -= n;
        }

        // 8. Dispatch the outcome to the wire.
        match outcome {
            BuildOutcome::Synth(resp) => {
                Http1Response::write_to(&resp, &mut downstream).await?;
            }
            BuildOutcome::Proxy {
                cluster: cluster_name,
            } => {
                // The validator (envoy-config Task 2) ensures every cluster
                // name referenced from a RouteAction::Route exists in the
                // bootstrap; the .expect() is defense-in-depth.
                let cluster = config
                    .cluster_mgr
                    .get(&cluster_name)
                    .expect("validator ensures cluster present");

                let endpoint = match cluster.pick_endpoint() {
                    Some(ep) => ep,
                    None => {
                        tracing::warn!(
                            cluster = %cluster.name(),
                            "no healthy endpoint for cluster — returning 503",
                        );
                        let resp = synth_status(503, close);
                        Http1Response::write_to(&resp, &mut downstream).await?;
                        if close {
                            return Ok(());
                        }
                        continue;
                    }
                };

                let host_header = find_header(&req.headers, headers::HOST)
                    .expect("build_response rejected missing/empty Host before BuildOutcome::Proxy")
                    .to_owned();

                // Strip Connection: per SPEC §3 D1 (one-shot upstream connection)
                // and Transfer-Encoding: per RFC 7230 §3.3.3 — the outgoing body
                // is forced to CL: 0, mirroring the response-side strip in
                // `router::write_proxied_response`.
                let mut out_headers = req.headers.clone();
                out_headers.retain(|(n, _)| {
                    !n.eq_ignore_ascii_case(headers::CONNECTION)
                        && !n.eq_ignore_ascii_case(headers::TRANSFER_ENCODING)
                });
                let out_req = Request {
                    method: req.method.clone(),
                    path: req.path.clone(),
                    version: HttpVersion::Http11,
                    headers: out_headers,
                    bytes_consumed: 0,
                    // Chunked-request-body forwarding is a SPEC §4 non-goal.
                    body: Some(Bytes::new()),
                };

                let start = std::time::Instant::now();
                let mut client_stream = match Client::connect(endpoint, &host_header).await {
                    Ok(s) => {
                        // 06.1 D4.b: per-cluster `upstream_cx_total`
                        // counter incremented once per established
                        // upstream TCP connection. Fires only on the
                        // success arm (a refused-connect path returns 502
                        // without incrementing).
                        cluster.cx_total().inc();
                        s
                    }
                    Err(source) => {
                        tracing::warn!(
                            cluster = %cluster.name(),
                            addr = %endpoint,
                            error = ?source,
                            "upstream connect failed — returning 502",
                        );
                        let resp = synth_status(502, close);
                        Http1Response::write_to(&resp, &mut downstream).await?;
                        if close {
                            return Ok(());
                        }
                        continue;
                    }
                };
                let upstream_response = match client_stream.send_request(out_req).await {
                    Ok(r) => r,
                    Err(source) => {
                        tracing::warn!(
                            cluster = %cluster.name(),
                            addr = %endpoint,
                            error = ?source,
                            "upstream request failed — returning 502",
                        );
                        let resp = synth_status(502, close);
                        Http1Response::write_to(&resp, &mut downstream).await?;
                        if close {
                            return Ok(());
                        }
                        continue;
                    }
                };
                let elapsed_ms = start.elapsed().as_millis();

                crate::router::write_proxied_response(
                    &mut downstream,
                    upstream_response,
                    elapsed_ms,
                    close,
                )
                .await?;
            }
        }

        // 9. Connection lifecycle.
        if close {
            return Ok(());
        }
        // Loop back; the buffer may contain pipelined bytes already, or
        // may need another read.
    }
}

fn parse_content_length(headers: &[(String, String)]) -> Result<usize, Http1Error> {
    match find_header(headers, headers::CONTENT_LENGTH) {
        Some(v) => v.parse::<usize>().map_err(|_| Http1Error::MalformedHeader),
        None => Ok(0),
    }
}

/// Outcome of the route walk: either a fully-synthesized downstream response
/// (DirectResponse arm or any 4xx/5xx synth path), or a directive to proxy
/// the request to the named cluster. The caller (serve_connection) writes
/// Synth via `Http1Response::write_to` and dispatches Proxy via
/// `cluster_mgr` → `pick_endpoint` → `Client::connect` → `send_request`.
pub enum BuildOutcome {
    Synth(Response),
    Proxy { cluster: String },
}

pub fn build_response(config: &HCMConfig, req: &Request, close: bool) -> BuildOutcome {
    // Validate Host header presence and non-emptiness (HTTP/1.1 §5.4 — mandatory).
    // Treat empty Host (`Host: \r\n`) as the same RFC violation as missing Host.
    let host_raw = match find_header(&req.headers, headers::HOST) {
        Some(h) if !h.is_empty() => h,
        _ => {
            tracing::warn!(
                method = %req.method,
                path = %req.path,
                "request rejected: missing or empty Host header"
            );
            return BuildOutcome::Synth(synth_400(close));
        }
    };
    let host = strip_port(host_raw);

    // Walk virtual_hosts first-match-wins on Host.
    let vh = match config
        .route_config
        .virtual_hosts
        .iter()
        .find(|vh| vh_matches(vh, host))
    {
        Some(vh) => vh,
        None => {
            tracing::warn!(
                host = %host,
                method = %req.method,
                path = %req.path,
                "request rejected: no matching virtual_host"
            );
            return BuildOutcome::Synth(synth_404(close));
        }
    };

    // Walk routes first-match-wins on path + headers.
    let route = match vh
        .routes
        .iter()
        .find(|r| route_matches(r, &req.path, &req.headers))
    {
        Some(r) => r,
        None => {
            tracing::warn!(
                host = %host,
                method = %req.method,
                path = %req.path,
                "request rejected: no matching route"
            );
            return BuildOutcome::Synth(synth_404(close));
        }
    };

    // Hardcoded router-filter call site.
    match &route.action {
        RouteAction::DirectResponse(dr) => BuildOutcome::Synth(synth_direct_response(dr, close)),
        RouteAction::Route(ar) => BuildOutcome::Proxy {
            cluster: ar.cluster.clone(),
        },
    }
}

fn strip_port(host: &str) -> &str {
    match host.rfind(':') {
        Some(i) => &host[..i],
        None => host,
    }
}

fn vh_matches(vh: &VirtualHost, host: &str) -> bool {
    vh.domains.iter().any(|d| {
        if d == "*" {
            true
        } else {
            d.eq_ignore_ascii_case(host)
        }
    })
}

fn route_matches(r: &Route, path: &str, headers: &[(String, String)]) -> bool {
    let path_match = match (&r.r#match.prefix, &r.r#match.path) {
        (Some(p), None) => path.starts_with(p),
        (None, Some(p)) => path == p,
        _ => false, // validator rejects (Some, Some) and (None, None).
    };
    if !path_match {
        return false;
    }
    // 04.2: AND-combine HeaderMatchers per Envoy default headers_match_options: ALL.
    r.r#match.headers.iter().all(|m| m.matches(headers))
}

fn now_imf_fixdate() -> String {
    format_imf_fixdate(SystemTime::now())
}

fn connection_value(close: bool) -> &'static str {
    if close { "close" } else { "keep-alive" }
}

fn synth_direct_response(dr: &DirectResponse, close: bool) -> Response {
    let body_str = dr.body.inline_string.as_deref().unwrap_or("");
    let body = Bytes::copy_from_slice(body_str.as_bytes());
    Response {
        status: dr.status,
        reason: None,
        headers: vec![
            (headers::SERVER.to_string(), DEFAULT_SERVER_NAME.to_string()),
            (headers::DATE.to_string(), now_imf_fixdate()),
            (headers::CONTENT_LENGTH.to_string(), body.len().to_string()),
            (
                headers::CONTENT_TYPE.to_string(),
                DEFAULT_CONTENT_TYPE.to_string(),
            ),
            (
                headers::CONNECTION.to_string(),
                connection_value(close).to_string(),
            ),
        ],
        body,
    }
}

fn synth_status(status: u16, close: bool) -> Response {
    let body = Bytes::new();
    Response {
        status,
        reason: None,
        headers: vec![
            (headers::SERVER.to_string(), DEFAULT_SERVER_NAME.to_string()),
            (headers::DATE.to_string(), now_imf_fixdate()),
            (headers::CONTENT_LENGTH.to_string(), "0".to_string()),
            (
                headers::CONTENT_TYPE.to_string(),
                DEFAULT_CONTENT_TYPE.to_string(),
            ),
            (
                headers::CONNECTION.to_string(),
                connection_value(close).to_string(),
            ),
        ],
        body,
    }
}

fn synth_400(close: bool) -> Response {
    synth_status(400, close)
}
fn synth_404(close: bool) -> Response {
    synth_status(404, close)
}
fn synth_501(close: bool) -> Response {
    synth_status(501, close)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Build a ClusterManager with a single static cluster `name` whose only
    /// endpoint is `127.0.0.1:<port>`. Reused by the 04.3 Task 9 router-proxy
    /// arm tests.
    async fn cluster_mgr_with_endpoint(
        name: &str,
        port: u16,
    ) -> Arc<envoy_cluster::ClusterManager> {
        let yaml = format!(
            r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
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
              - endpoint: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: {port} }} }} }}
"#
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("bootstrap parses");
        Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
                .await
                .expect("cluster mgr"),
        )
    }

    /// Build an empty ClusterManager (no clusters). Used by the existing
    /// 04.1/04.2 tests whose RouteAction is always DirectResponse and never
    /// reaches the cluster lookup.
    async fn cluster_mgr_empty() -> Arc<envoy_cluster::ClusterManager> {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners: []
  clusters: []
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("bootstrap parses");
        Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
                .await
                .expect("cluster mgr"),
        )
    }

    /// 06.1 NEW: register an HCMStats handle against a fresh registry
    /// under the given `stat_prefix`. Mirrors the production
    /// `HCMConfig::from_config` registration but lets tests construct
    /// HCMConfig as a struct-literal without going through `from_config`.
    fn mk_stats(stat_prefix: &str) -> Arc<HCMStats> {
        let registry = envoy_stats::StatsRegistry::new();
        Arc::new(HCMStats::register(&registry, stat_prefix).expect("HCMStats register"))
    }

    /// Build a minimal HCMConfig with a single VH `domains: ["*"]`,
    /// configurable routes.
    async fn hcm_config_single_route(prefix: &str, status: u16, body: &str) -> Arc<HCMConfig> {
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            route_config: Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some(prefix.to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status,
                            body: DataSource {
                                filename: None,
                                inline_string: Some(body.to_string()),
                            },
                        }),
                    }],
                }],
            }),
        })
    }

    /// Drive a single request through serve_connection over an in-process pair.
    /// Returns the response bytes.
    async fn drive(config: Arc<HCMConfig>, req_bytes: &[u8]) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let _ = serve_connection(config, sock).await;
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(req_bytes).await.unwrap();
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).await.unwrap();
        // Drop client to ensure server's loop exits.
        drop(client);
        let _ = server.await;
        buf
    }

    #[tokio::test]
    async fn direct_response_returns_status_and_body() {
        let config = hcm_config_single_route("/", 200, "ok\n").await;
        let req = b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 200 OK\r\n"),
            "status: {resp_str}"
        );
        assert!(
            resp_str.contains("server: envoy-rust\r\n"),
            "server: {resp_str}"
        );
        assert!(resp_str.contains("date: "), "date: {resp_str}");
        assert!(resp_str.contains("content-length: 3\r\n"), "cl: {resp_str}");
        assert!(
            resp_str.contains("content-type: text/plain\r\n"),
            "ct: {resp_str}"
        );
        assert!(
            resp_str.contains("connection: close\r\n"),
            "conn: {resp_str}"
        );
        assert!(resp_str.ends_with("\r\nok\n"), "body: {resp_str}");
    }

    #[tokio::test]
    async fn host_match_strips_port() {
        let config = Arc::new(HCMConfig {
            stat_prefix: "x".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("x"),
            route_config: Arc::new(RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "specific".to_string(),
                    domains: vec!["foo.example.com".to_string()],
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
                                inline_string: Some("hit\n".to_string()),
                            },
                        }),
                    }],
                }],
            }),
        });
        let req = b"GET / HTTP/1.1\r\nHost: foo.example.com:8080\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 200 OK\r\n"),
            "expected 200, got: {resp_str}"
        );
        assert!(resp_str.ends_with("\r\nhit\n"));
    }

    #[tokio::test]
    async fn first_match_wins_on_routes() {
        let config = Arc::new(HCMConfig {
            stat_prefix: "x".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("x"),
            route_config: Arc::new(RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    routes: vec![
                        Route {
                            r#match: RouteMatch {
                                prefix: Some("/healthz".to_string()),
                                path: None,
                                headers: vec![],
                            },
                            action: RouteAction::DirectResponse(DirectResponse {
                                status: 200,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("first\n".to_string()),
                                },
                            }),
                        },
                        Route {
                            r#match: RouteMatch {
                                prefix: Some("/".to_string()),
                                path: None,
                                headers: vec![],
                            },
                            action: RouteAction::DirectResponse(DirectResponse {
                                status: 500,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("never\n".to_string()),
                                },
                            }),
                        },
                    ],
                }],
            }),
        });
        let req = b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 200 OK\r\n"),
            "first match must win: {resp_str}"
        );
        assert!(resp_str.ends_with("\r\nfirst\n"));
    }

    #[tokio::test]
    async fn missing_host_returns_400() {
        let config = hcm_config_single_route("/", 200, "ok\n").await;
        let req = b"GET / HTTP/1.1\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 400 Bad Request\r\n"),
            "got: {resp_str}"
        );
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let config = Arc::new(HCMConfig {
            stat_prefix: "x".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("x"),
            route_config: Arc::new(RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "specific".to_string(),
                    domains: vec!["only.example.com".to_string()],
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
            }),
        });
        // Host doesn't match any VH → 404.
        let req = b"GET / HTTP/1.1\r\nHost: other.example.com\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 404 Not Found\r\n"),
            "got: {resp_str}"
        );
    }

    #[tokio::test]
    async fn connection_close_closes_socket() {
        let config = hcm_config_single_route("/", 200, "ok\n").await;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.contains("connection: close\r\n"),
            "got: {resp_str}"
        );
        // drive() called read_to_end which returns 0 once server closes — no
        // additional check needed beyond that drive returned at all.
    }

    #[tokio::test]
    async fn keep_alive_serves_two_requests() {
        let config = hcm_config_single_route("/", 200, "ok\n").await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let _ = serve_connection(config, sock).await;
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        // Request 1: keep-alive (HTTP/1.1 default).
        client
            .write_all(b"GET /a HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        // Request 2: explicit close so server returns Ok and client sees EOF.
        client
            .write_all(b"GET /b HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).await.unwrap();
        drop(client);
        let _ = server.await;
        let s = String::from_utf8_lossy(&buf);
        // Two responses concatenated. Each starts with "HTTP/1.1 200 OK".
        let count_200 = s.matches("HTTP/1.1 200 OK\r\n").count();
        assert_eq!(count_200, 2, "expected 2 responses, got: {s}");
    }

    /// Build a minimal HCMConfig with a single VH `domains: ["*"]` and the
    /// given routes. Used by 04.2 header-matcher tests.
    async fn build_test_config(routes: Vec<Route>) -> Arc<HCMConfig> {
        Arc::new(HCMConfig {
            stat_prefix: "test".into(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("test"),
            route_config: Arc::new(RouteConfiguration {
                name: "test_rc".into(),
                virtual_hosts: vec![VirtualHost {
                    name: "test_vh".into(),
                    domains: vec!["*".into()],
                    routes,
                }],
            }),
        })
    }

    // ── 04.2 header-matcher HCM integration tests ────────────────────────────

    #[tokio::test]
    async fn route_with_no_headers_matches_unchanged() {
        // Regression: a route with empty headers Vec still matches on path only.
        let cfg = build_test_config(vec![Route {
            r#match: RouteMatch {
                prefix: Some("/".into()),
                path: None,
                headers: vec![],
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok\n".into()),
                },
            }),
        }])
        .await;
        let req = b"GET /healthz HTTP/1.1\r\nHost: x.test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        assert!(std::str::from_utf8(&resp).unwrap().contains("200 OK"));
    }

    #[tokio::test]
    async fn single_header_matcher_route_selected_when_match() {
        let matcher_route = Route {
            r#match: RouteMatch {
                prefix: Some("/api/".into()),
                path: None,
                headers: vec![envoy_config::HeaderMatcher {
                    name: "x-foo".into(),
                    mode: envoy_config::HeaderMatcherMode::ExactMatch("bar".into()),
                    invert_match: false,
                }],
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 418,
                body: DataSource {
                    filename: None,
                    inline_string: Some("teapot\n".into()),
                },
            }),
        };
        let default_route = Route {
            r#match: RouteMatch {
                prefix: Some("/".into()),
                path: None,
                headers: vec![],
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok\n".into()),
                },
            }),
        };
        let cfg = build_test_config(vec![matcher_route, default_route]).await;
        let req =
            b"GET /api/widgets HTTP/1.1\r\nHost: x.test\r\nX-Foo: bar\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = std::str::from_utf8(&resp).unwrap();
        assert!(s.contains("418"), "expected 418 teapot, got: {s}");
        assert!(s.contains("teapot\n"));
    }

    #[tokio::test]
    async fn single_header_matcher_route_skipped_when_no_match() {
        let matcher_route = Route {
            r#match: RouteMatch {
                prefix: Some("/api/".into()),
                path: None,
                headers: vec![envoy_config::HeaderMatcher {
                    name: "x-foo".into(),
                    mode: envoy_config::HeaderMatcherMode::ExactMatch("bar".into()),
                    invert_match: false,
                }],
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 418,
                body: DataSource {
                    filename: None,
                    inline_string: Some("teapot\n".into()),
                },
            }),
        };
        let default_route = Route {
            r#match: RouteMatch {
                prefix: Some("/".into()),
                path: None,
                headers: vec![],
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok\n".into()),
                },
            }),
        };
        let cfg = build_test_config(vec![matcher_route, default_route]).await;
        // /api/widgets but no X-Foo header → falls through to default 200.
        let req = b"GET /api/widgets HTTP/1.1\r\nHost: x.test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = std::str::from_utf8(&resp).unwrap();
        assert!(s.contains("200 OK"), "expected 200, got: {s}");
    }

    #[tokio::test]
    async fn multi_header_matcher_and_combination_all_match() {
        let matcher_route = Route {
            r#match: RouteMatch {
                prefix: Some("/".into()),
                path: None,
                headers: vec![
                    envoy_config::HeaderMatcher {
                        name: "x-a".into(),
                        mode: envoy_config::HeaderMatcherMode::ExactMatch("1".into()),
                        invert_match: false,
                    },
                    envoy_config::HeaderMatcher {
                        name: "x-b".into(),
                        mode: envoy_config::HeaderMatcherMode::ExactMatch("2".into()),
                        invert_match: false,
                    },
                ],
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 418,
                body: DataSource {
                    filename: None,
                    inline_string: Some("teapot\n".into()),
                },
            }),
        };
        let cfg = build_test_config(vec![matcher_route]).await;
        let req =
            b"GET / HTTP/1.1\r\nHost: x.test\r\nX-A: 1\r\nX-B: 2\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        assert!(std::str::from_utf8(&resp).unwrap().contains("418"));
    }

    #[tokio::test]
    async fn multi_header_matcher_and_combination_one_fails() {
        let matcher_route = Route {
            r#match: RouteMatch {
                prefix: Some("/api/".into()),
                path: None,
                headers: vec![
                    envoy_config::HeaderMatcher {
                        name: "x-a".into(),
                        mode: envoy_config::HeaderMatcherMode::ExactMatch("1".into()),
                        invert_match: false,
                    },
                    envoy_config::HeaderMatcher {
                        name: "x-b".into(),
                        mode: envoy_config::HeaderMatcherMode::ExactMatch("2".into()),
                        invert_match: false,
                    },
                ],
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 418,
                body: DataSource {
                    filename: None,
                    inline_string: Some("teapot\n".into()),
                },
            }),
        };
        let default_route = Route {
            r#match: RouteMatch {
                prefix: Some("/".into()),
                path: None,
                headers: vec![],
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok\n".into()),
                },
            }),
        };
        let cfg = build_test_config(vec![matcher_route, default_route]).await;
        // X-A matches, X-B does not → matcher route fails, fall through to default.
        let req = b"GET /api/widgets HTTP/1.1\r\nHost: x.test\r\nX-A: 1\r\nX-B: WRONG\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        assert!(std::str::from_utf8(&resp).unwrap().contains("200 OK"));
    }

    #[tokio::test]
    async fn chunked_request_rejected_with_501() {
        let config = hcm_config_single_route("/", 200, "ok\n").await;
        let req = b"POST /up HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n0\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 501 Not Implemented\r\n"),
            "got: {resp_str}"
        );
    }

    // ── 04.3 Task 9 router-proxy arm tests ────────────────────────────────────

    /// Build a minimal HCMConfig with a single VH `domains: ["*"]`,
    /// a configurable route prefix + action, and a caller-supplied
    /// cluster_mgr. Used by the Task-9 Route-arm tests.
    fn hcm_config_with_cluster(
        prefix: &str,
        action: RouteAction,
        cluster_mgr: Arc<envoy_cluster::ClusterManager>,
    ) -> Arc<HCMConfig> {
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            route_config: Arc::new(RouteConfiguration {
                name: "rc".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some(prefix.to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action,
                    }],
                }],
            }),
        })
    }

    /// Spawn an in-process upstream HTTP/1.1 acceptor on an ephemeral port.
    /// The acceptor reads (then ignores) the incoming request bytes and
    /// writes the supplied response. Returns the bound port.
    async fn spawn_in_process_upstream(response: &'static [u8]) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = vec![0u8; 4096];
                let _ = tokio::time::timeout(Duration::from_millis(500), sock.read(&mut buf)).await;
                let _ = sock.write_all(response).await;
                let _ = sock.shutdown().await;
            }
        });
        port
    }

    // NOTE: route_walk_returns_no_healthy_endpoint_when_cluster_empty is
    // documented-only and intentionally not landed here. The Route arm uses
    // .expect("validator ensures cluster present") on cluster_mgr.get(), so
    // a missing cluster panics rather than surfacing as 503; constructing a
    // present cluster with zero endpoints requires bypassing
    // envoy-cluster::from_bootstrap's EmptyCluster rejection. The test
    // moves into the upstream-robustness family alongside health checking.
    // See PROGRESS Task 9 + PLAN Task 9 Step 2 NOTE.

    #[tokio::test(flavor = "multi_thread")]
    async fn route_walk_dispatches_direct_response_unchanged() {
        // Regression: the 04.1 + 04.2 DirectResponse path is unchanged after
        // the Task-9 RouteAction restructure.
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok\n".into()),
                },
            }),
            cluster_mgr_empty().await,
        );
        let req = b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "got: {s}");
        assert!(s.ends_with("\r\nok\n"), "body: {s}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn route_walk_dispatches_route_action_to_client_connect() {
        let upstream_response: &'static [u8] =
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 12\r\n\r\nhello, world";
        let upstream_port = spawn_in_process_upstream(upstream_response).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
            }),
            cluster_mgr,
        );
        let req = b"GET /any HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "got: {s}");
        assert!(
            s.contains("server: envoy-rust\r\n"),
            "server overwrite: {s}"
        );
        assert!(
            s.contains("x-envoy-upstream-service-time: "),
            "x-envoy-upstream-service-time present: {s}"
        );
        assert!(
            s.contains("content-type: text/plain\r\n"),
            "ct passthrough: {s}"
        );
        assert!(s.contains("content-length: 12\r\n"), "cl passthrough: {s}");
        assert!(s.ends_with("hello, world"), "body passthrough: {s}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn route_walk_returns_upstream_connect_on_refused_port() {
        // Cluster's single endpoint is 127.0.0.1:1 (kernel-refused). HCM's
        // Route arm should propagate the connect failure as a 502 Bad Gateway
        // downstream response.
        let cluster_mgr = cluster_mgr_with_endpoint("backend", 1).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
            }),
            cluster_mgr,
        );
        let req = b"GET /any HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 502 Bad Gateway\r\n"),
            "expected 502 on UpstreamConnect, got: {s}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxied_response_carries_x_envoy_upstream_service_time() {
        // Don't pin the exact value (timing-dependent); assert presence +
        // parseability as integer ms.
        let upstream_response: &'static [u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let upstream_port = spawn_in_process_upstream(upstream_response).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
            }),
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        let line = s
            .lines()
            .find(|l| l.starts_with("x-envoy-upstream-service-time: "))
            .expect("x-envoy-upstream-service-time present");
        let value = line
            .trim_start_matches("x-envoy-upstream-service-time: ")
            .trim();
        let _ms: u128 = value.parse().expect("integer ms");
    }

    /// In-process upstream that captures the wire bytes it received and
    /// returns them via a JoinHandle. Mirrors `client::tests::capturing_acceptor`
    /// (Task 6) — reproduced inline because that helper is `mod tests`-private
    /// to client.rs, and one test doesn't justify hoisting to a shared
    /// `pub(crate)` test helper.
    async fn spawn_capturing_upstream(
        response: &'static [u8],
    ) -> (u16, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let h = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = tokio::time::timeout(Duration::from_millis(500), sock.read(&mut buf))
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or(0);
            buf.truncate(n);
            let _ = sock.write_all(response).await;
            let _ = sock.shutdown().await;
            buf
        });
        (port, h)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxy_strips_transfer_encoding_from_outgoing_request() {
        // RFC 7230 §3.3.3: a request must not carry both Transfer-Encoding
        // and Content-Length (CL: 0 is forced by the Proxy arm because
        // chunked-request-body forwarding is a SPEC §4 non-goal). The
        // chunked-request 501-reject at hcm.rs only matches T-E: chunked,
        // so a downstream T-E: identity passes through to the Proxy arm
        // and exercises the strip.
        let upstream_response: &'static [u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let (upstream_port, capture) = spawn_capturing_upstream(upstream_response).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
            }),
            cluster_mgr,
        );
        let req = b"GET /any HTTP/1.1\r\nHost: x.test\r\nTransfer-Encoding: identity\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _resp = drive(cfg, req).await;
        let captured = capture.await.unwrap();
        let s = String::from_utf8_lossy(&captured);
        assert!(
            !s.to_ascii_lowercase().contains("transfer-encoding:"),
            "outgoing upstream request must not carry Transfer-Encoding (RFC 7230 §3.3.3): {s}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn proxied_response_overwrites_upstream_server_header() {
        let upstream_response: &'static [u8] =
            b"HTTP/1.1 200 OK\r\nServer: nginx/1.x\r\nContent-Length: 0\r\n\r\n";
        let upstream_port = spawn_in_process_upstream(upstream_response).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
            }),
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.contains("server: envoy-rust\r\n"),
            "server overwrite: {s}"
        );
        assert!(
            !s.contains("nginx/1.x"),
            "upstream Server must not pass through: {s}"
        );
    }

    /// 06.1 D4.c: per-HCM `downstream_rq_total` counter increments once
    /// per HCM-handled request. Drives one direct-response request through
    /// `serve_connection` and asserts the counter reads `1`. Test uses a
    /// dedicated registry + stat_prefix so the counter Arc returned by
    /// `register_counter` is the same one the HCMConfig increments
    /// (Task 5 idempotent-same-kind contract).
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm1_increments_downstream_rq_total_on_request() {
        // Build the HCMConfig via the production constructor so the
        // increment site is exercised end-to-end (rather than via the
        // struct-literal helpers above, which manufacture HCMStats from a
        // throwaway registry and would not be observable here).
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = cluster_mgr_empty().await;
        let envoy_cfg = envoy_config::HttpConnectionManagerConfig {
            stat_prefix: "ingress_http".to_string(),
            codec_type: envoy_config::CodecType::HTTP1,
            http2_protocol_options: None,
            // 06.2 Task 5: field added to the schema; access-log wiring
            // lands in Task 6 (H1) / Task 7 (H2). Empty here.
            access_log: vec![],
            route_config: RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
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
            http_filters: vec![envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            }],
        };
        let hcm_config = Arc::new(
            HCMConfig::from_config(&envoy_cfg, cluster_mgr, Arc::clone(&registry))
                .expect("HCMConfig builds"),
        );

        // Re-register the counter to capture the same Arc the HCM holds.
        let cx_counter = registry
            .register_counter("http.ingress_http.downstream_rq_total")
            .expect("counter registers");
        assert_eq!(cx_counter.value(), 0);

        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let _resp = drive(hcm_config, req).await;

        assert_eq!(
            cx_counter.value(),
            1,
            "expected exactly one downstream_rq_total increment per HCM-handled request",
        );
    }
}
