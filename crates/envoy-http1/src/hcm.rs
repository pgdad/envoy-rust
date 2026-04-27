//! HTTP connection manager: per-listener config, per-connection state machine,
//! route walker, hardcoded router-filter call site.

use crate::codec::{Http1Codec, HttpVersion, Request};
use crate::date::format_imf_fixdate;
use crate::error::Http1Error;
use crate::headers::{self, find_header};
use crate::response::{Http1Response, Response};

use bytes::{Buf, Bytes, BytesMut};
use envoy_config::{
    DataSource, DirectResponse, HttpConnectionManagerConfig, Route, RouteConfiguration, RouteMatch,
    VirtualHost,
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

#[derive(Debug)]
pub struct HCMConfig {
    pub stat_prefix: String,
    pub route_config: Arc<RouteConfiguration>,
    // 04.3: pub cluster_mgr: Arc<envoy_cluster::ClusterManager>,
}

impl HCMConfig {
    pub fn from_config(cfg: &HttpConnectionManagerConfig) -> Result<Self, Http1Error> {
        // The validator (envoy-config Task 2) has already enforced shape.
        // This constructor is `Result<>` for forward-compat with 04.3's
        // cluster lookup; in 04.1 it never returns Err.
        Ok(Self {
            stat_prefix: cfg.stat_prefix.clone(),
            route_config: Arc::new(clone_route_config(&cfg.route_config)),
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
                        direct_response: DirectResponse {
                            status: r.direct_response.status,
                            body: DataSource {
                                filename: r.direct_response.body.filename.clone(),
                                inline_string: r.direct_response.body.inline_string.clone(),
                            },
                        },
                    })
                    .collect(),
            })
            .collect(),
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

        // 3. Determine close/keep-alive decision before any move.
        let close = req.headers.iter().any(|(n, v)| {
            n.eq_ignore_ascii_case(headers::CONNECTION) && v.eq_ignore_ascii_case("close")
        }) || req.version == HttpVersion::Http10;

        // 4. Compute body length (for drain) before consuming.
        let body_len = parse_content_length(&req.headers)?;
        let chunked = req.headers.iter().any(|(n, v)| {
            n.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked")
        });

        // 5. Build response (handles 400 / 404 / 501 / 200 internally).
        let resp = if chunked {
            tracing::warn!(
                method = %req.method,
                path = %req.path,
                "request rejected: Transfer-Encoding: chunked not supported (501)"
            );
            synth_501(close)
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

        // 8. Write response.
        Http1Response::write_to(&resp, &mut downstream).await?;

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

fn build_response(config: &HCMConfig, req: &Request, close: bool) -> Response {
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
            return synth_400(close);
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
            return synth_404(close);
        }
    };

    // Walk routes first-match-wins on path.
    let route = match vh.routes.iter().find(|r| route_matches(r, &req.path)) {
        Some(r) => r,
        None => {
            tracing::warn!(
                host = %host,
                method = %req.method,
                path = %req.path,
                "request rejected: no matching route"
            );
            return synth_404(close);
        }
    };

    // Hardcoded router-filter call site:
    //   match action { DirectResponse(dr) => synth_direct_response(req, dr) }
    // 04.3 will extend this match with a Route(_) arm.
    synth_direct_response(&route.direct_response, close)
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

fn route_matches(r: &Route, path: &str) -> bool {
    match (&r.r#match.prefix, &r.r#match.path) {
        (Some(p), None) => path.starts_with(p),
        (None, Some(p)) => path == p,
        _ => false, // validator rejects (Some, Some) and (None, None).
    }
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

    /// Build a minimal HCMConfig with a single VH `domains: ["*"]`,
    /// configurable routes.
    fn hcm_config_single_route(prefix: &str, status: u16, body: &str) -> Arc<HCMConfig> {
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
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
                        direct_response: DirectResponse {
                            status,
                            body: DataSource {
                                filename: None,
                                inline_string: Some(body.to_string()),
                            },
                        },
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
        let config = hcm_config_single_route("/", 200, "ok\n");
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
                        direct_response: DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("hit\n".to_string()),
                            },
                        },
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
                            direct_response: DirectResponse {
                                status: 200,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("first\n".to_string()),
                                },
                            },
                        },
                        Route {
                            r#match: RouteMatch {
                                prefix: Some("/".to_string()),
                                path: None,
                                headers: vec![],
                            },
                            direct_response: DirectResponse {
                                status: 500,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("never\n".to_string()),
                                },
                            },
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
        let config = hcm_config_single_route("/", 200, "ok\n");
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
                        direct_response: DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        },
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
        let config = hcm_config_single_route("/", 200, "ok\n");
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
        let config = hcm_config_single_route("/", 200, "ok\n");
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

    #[tokio::test]
    async fn chunked_request_rejected_with_501() {
        let config = hcm_config_single_route("/", 200, "ok\n");
        let req = b"POST /up HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n0\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 501 Not Implemented\r\n"),
            "got: {resp_str}"
        );
    }
}
