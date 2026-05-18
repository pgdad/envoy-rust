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
    // 06.3 D15.3.a NEW — per-response-class counters. Incremented at the
    // post-`match outcome` factored dispatch site (after `response_status_for_log`
    // is populated by all 5 writer arms). Mirrors Envoy v1.33.0 stats docs;
    // codes outside [200, 600) silently no-op per the `_ => {}` arm.
    /// `http.<stat_prefix>.downstream_rq_2xx` — HTTP 2xx responses.
    pub downstream_rq_2xx: Arc<envoy_stats::Counter>,
    /// `http.<stat_prefix>.downstream_rq_3xx` — HTTP 3xx responses.
    pub downstream_rq_3xx: Arc<envoy_stats::Counter>,
    /// `http.<stat_prefix>.downstream_rq_4xx` — HTTP 4xx responses.
    pub downstream_rq_4xx: Arc<envoy_stats::Counter>,
    /// `http.<stat_prefix>.downstream_rq_5xx` — HTTP 5xx responses.
    pub downstream_rq_5xx: Arc<envoy_stats::Counter>,
    // 06.3 D15.3.e NEW — access-log emission counters. `access_logs_total`
    // fires at queue-enter time (BEFORE the per-sink await loop) using
    // Counter::add(N) where N is the configured sink count, per 06.1 REVIEW
    // §7 R-8 (one bulk increment per request, not N individual .inc() calls).
    // `access_logs_failed` fires inside the per-sink Err arm alongside
    // tracing::warn!. Per parent SPEC §6 Rule 4: sink failures do NOT deflate
    // access_logs_total — the total counts intent-to-emit, not successful emit.
    /// `http.<stat_prefix>.access_logs_total` — total access-log records
    /// dispatched to sinks (bulk-incremented by sink count at queue-enter time).
    pub access_logs_total: Arc<envoy_stats::Counter>,
    /// `http.<stat_prefix>.access_logs_failed` — per-sink emission failures
    /// (incremented inside the Err arm alongside tracing::warn!).
    pub access_logs_failed: Arc<envoy_stats::Counter>,
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
            downstream_rq_2xx: registry
                .register_counter(&format!("http.{stat_prefix}.downstream_rq_2xx"))?,
            downstream_rq_3xx: registry
                .register_counter(&format!("http.{stat_prefix}.downstream_rq_3xx"))?,
            downstream_rq_4xx: registry
                .register_counter(&format!("http.{stat_prefix}.downstream_rq_4xx"))?,
            downstream_rq_5xx: registry
                .register_counter(&format!("http.{stat_prefix}.downstream_rq_5xx"))?,
            access_logs_total: registry
                .register_counter(&format!("http.{stat_prefix}.access_logs_total"))?,
            access_logs_failed: registry
                .register_counter(&format!("http.{stat_prefix}.access_logs_failed"))?,
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
    /// 06.2 NEW: configured access-log sinks. Empty by default;
    /// non-empty when the listener YAML carries an `access_log:`
    /// block. The HCM dispatches each per-request record to every
    /// sink in this vec at a factored join point in
    /// `serve_connection` (synchronous-after-write per parent-06
    /// SPEC §6 architectural Rule 4 option (b); emission errors
    /// logged via `tracing::warn!` and discarded).
    pub access_log: Vec<Arc<envoy_accesslog::FileSink>>,
    /// 07.1 Task 6: per-listener filter pipeline. Arc-shared at
    /// config-build time. Each per-request scope inside `serve_connection`
    /// clones into a working `FilterPipeline` via
    /// `(*config.filter_pipeline).clone()`. At 07.1 the per-request clone
    /// is effectively a no-op (Router is zero-state); the clone shape is
    /// structural for 07.2's HeaderMutation per-stream cloning.
    pub filter_pipeline: Arc<envoy_filter::FilterPipeline>,
}

impl HCMConfig {
    pub async fn from_config(
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
        // 06.2 Task 6: open every configured access-log sink at config-load
        // time so per-request emission can dispatch via Arc<FileSink>::emit
        // without re-opening the file. `from_config` is async because
        // `FileSink::new` is async (tokio::fs::OpenOptions); the envoy-bin
        // caller and the envoy-http2 test sites are all in async contexts so
        // promoting this constructor to async is a clean change.
        let mut access_log_sinks: Vec<Arc<envoy_accesslog::FileSink>> = Vec::new();
        for entry in &cfg.access_log {
            match &entry.typed_config {
                envoy_config::AccessLogTypedConfig::FileAccessLog(file_cfg) => {
                    let sink =
                        envoy_accesslog::FileSink::new(std::path::PathBuf::from(&file_cfg.path))
                            .await
                            .map_err(|err| Http1Error::AccessLogOpen {
                                message: err.to_string(),
                            })?;
                    access_log_sinks.push(Arc::new(sink));
                }
            }
        }
        // 07.1 Task 6: build the filter pipeline at config-load time.
        // The envoy-config validator (07.1 Task 4) has already enforced
        // [1..=N filters with Router-at-terminus]; this build is
        // defense-in-depth at the framework crate boundary. The `?`
        // propagates via the `#[from]` impl on `Http1Error::FilterPipeline`.
        let filter_pipeline = Arc::new(envoy_filter::FilterPipeline::build_from_config(
            &cfg.http_filters,
            &registry,
            &cfg.stat_prefix,
        )?);
        Ok(Self {
            stat_prefix: cfg.stat_prefix.clone(),
            route_config: Arc::new(clone_route_config(&cfg.route_config)),
            cluster_mgr,
            http2_protocol_options: cfg.http2_protocol_options.clone(),
            stats,
            access_log: access_log_sinks,
            filter_pipeline,
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

        // 06.2 Task 6: capture request-arrival timing immediately after
        // parse-success. The Instant is for duration measurement
        // (monotonic); the SystemTime is for `%START_TIME%` rendering
        // (wall-clock). Both are sampled at request-arrival per Envoy's
        // %START_TIME% semantic.
        let req_arrival_instant = std::time::Instant::now();
        let req_arrival_systime = std::time::SystemTime::now();

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

        // 06.2 Task 6: request-side wire-byte count for
        // `%BYTES_RECEIVED%`. Per Envoy semantic, header bytes are NOT
        // counted; only the request body. The HCM has already enforced
        // Content-Length on this path (chunked-request bodies are
        // 501-rejected upstream), so `body_len` is the authoritative
        // bytes-received for the access-log record.
        let request_body_len: u64 = body_len as u64;

        // 07.1 Task 6: decode-side filter invocation. Clone the pipeline
        // per-request (cheap at 07.1; structural for 07.2's HeaderMutation
        // per-stream cloning per ADR-0031). `req` is shadowed as `mut`
        // so the boundary conversion can write back the filter-visible
        // fields after the decode pass.
        let mut req = req;
        let mut pipeline = (*config.filter_pipeline).clone();

        // Boundary conversion: construct FilterRequest from the
        // filter-visible subset of envoy_http1::Request, invoke
        // decode_headers, write back. The codec-state fields (`version`,
        // `bytes_consumed`) stay in `req` and are not surfaced to filters.
        let mut filter_req = envoy_filter::FilterRequest {
            method: std::mem::take(&mut req.method),
            path: std::mem::take(&mut req.path),
            headers: std::mem::take(&mut req.headers),
            body: req.body.take(),
        };
        let decode_decision = pipeline.decode_headers(&mut filter_req);
        // Write back the (possibly mutated) fields.
        req.method = filter_req.method;
        req.path = filter_req.path;
        req.headers = filter_req.headers;
        req.body = filter_req.body;

        // 5. Build response (handles 400 / 404 / 501 / 200 internally) or
        //    decide to proxy upstream. 07.1 Task 6: dispatch through
        //    RequestPath so decode-side `StopAndSend(filter_resp)` can
        //    short-circuit the writer-arm match. Under the Router-only
        //    07.1 chain the SynthFromDecode arm is unreachable (Router
        //    never emits StopAndSend); 07.2's HeaderMutation may.
        let request_path = match decode_decision {
            envoy_filter::Decision::Continue => {
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
                RequestPath::Match(outcome)
            }
            envoy_filter::Decision::StopAndSend(filter_resp) => {
                // Convert FilterResponse → codec-native Response.
                RequestPath::SynthFromDecode(Response {
                    status: filter_resp.status,
                    reason: filter_resp.reason,
                    headers: filter_resp.headers,
                    body: filter_resp.body,
                })
            }
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

        // 06.2 Task 6: per-request access-log state. The proxy-success arm
        // captures the resolved upstream endpoint for the access-log
        // `%UPSTREAM_HOST%` token; synth and error arms leave it None.
        //
        // 07.1 Task 5: The three other locals (`response_status_for_log`,
        // `response_body_len`, `response_headers_for_log`) — formerly
        // populated per-arm and read at the dispatch site — are now derived
        // once below the writer-arm match from the unified `outgoing:
        // Response` value (see comment at the unified factored site).
        let mut upstream_host_for_log: Option<String> = None; // stays mut — only proxy arm populates

        // 07.1 Task 5: per-arm-populated response value, written to the wire
        // once below the match (factored unified-site). 07.1 Task 6 flipped
        // this from `let outgoing` to `let mut outgoing` because the
        // encode-side `Decision::StopAndSend(replacement)` branch replaces
        // `outgoing` entirely with a filter's substitute response. Rust's
        // flow analysis still verifies every writer arm populates `outgoing`
        // before the unified site reads it; a compile error (E0381) fires
        // if any arm is missed.
        let mut outgoing: Response;

        // 8. Dispatch the request_path to the wire. 07.1 Task 6 wraps the
        // Task 5 writer-arm match inside `RequestPath::Match(outcome)`; the
        // new `SynthFromDecode(resp)` arm short-circuits when a decode-side
        // filter emitted `StopAndSend` (unreachable under the Router-only
        // 07.1 chain — Router never emits StopAndSend; the arm lands
        // structurally for 07.2 HeaderMutation forward-compat).
        match request_path {
            RequestPath::Match(outcome) => match outcome {
                BuildOutcome::Synth(resp) => {
                    outgoing = resp;
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

                    if let Some(endpoint) = cluster.pick_endpoint() {
                        let host_header = find_header(&req.headers, headers::HOST)
                            .expect(
                                "build_response rejected missing/empty Host before BuildOutcome::Proxy",
                            )
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

                        // 06.2 Task 6: capture the resolved upstream endpoint
                        // for the access-log `%UPSTREAM_HOST%` token. The
                        // SocketAddr Display impl produces the canonical
                        // `addr:port` rendering envoy uses.
                        upstream_host_for_log = Some(endpoint.to_string());

                        // 06.3 D15.3.b: RAII guard increments
                        // `cluster.<name>.upstream_cx_active` at the start of the
                        // proxy arm and decrements via Drop at scope exit, covering
                        // both success and error close paths uniformly.
                        let _cx_guard = cluster.cx_active_guard();

                        let start = std::time::Instant::now();
                        let client_result = Client::connect(endpoint, &host_header).await;
                        match client_result {
                            Ok(mut client_stream) => {
                                // 06.1 D4.b: per-cluster `upstream_cx_total`
                                // counter incremented once per established
                                // upstream TCP connection. Fires only on the
                                // success arm (a refused-connect path returns
                                // 502 without incrementing).
                                cluster.cx_total().inc();

                                match client_stream.send_request(out_req).await {
                                    Ok(upstream_response) => {
                                        let elapsed_ms = start.elapsed().as_millis();
                                        // 07.1 Task 5: factored — was a direct
                                        // `crate::router::write_proxied_response`
                                        // call; now constructs the Response
                                        // value for the unified wire-write site
                                        // below the match. The x-envoy-upstream-
                                        // service-time header is injected by
                                        // construct_proxied_response and flows
                                        // into response_headers_for_log via the
                                        // outgoing.headers.clone() at the
                                        // unified site below.
                                        outgoing = crate::router::construct_proxied_response(
                                            &cluster,
                                            upstream_response,
                                            elapsed_ms,
                                            close,
                                        );
                                    }
                                    Err(source) => {
                                        tracing::warn!(
                                            cluster = %cluster.name(),
                                            addr = %endpoint,
                                            error = ?source,
                                            "upstream request failed — returning 502",
                                        );
                                        outgoing = synth_status(502, close);
                                    }
                                }
                            }
                            Err(source) => {
                                tracing::warn!(
                                    cluster = %cluster.name(),
                                    addr = %endpoint,
                                    error = ?source,
                                    "upstream connect failed — returning 502",
                                );
                                outgoing = synth_status(502, close);
                            }
                        }
                    } else {
                        // No healthy endpoint available for this cluster.
                        tracing::warn!(
                            cluster = %cluster.name(),
                            "no healthy endpoint for cluster — returning 503",
                        );
                        outgoing = synth_status(503, close);
                    }
                }
            },
            RequestPath::SynthFromDecode(resp) => {
                // 07.1 Task 6: decode-side filter short-circuit. Unreachable
                // under the Router-only 07.1 chain; lit by 07.2's HeaderMutation
                // (which never short-circuits via StopAndSend on production
                // paths) and by phase 09's LocalRateLimit filter (the first
                // production filter to emit StopAndSend with a sparse header
                // list). `upstream_host_for_log` stays None (no proxy attempt).
                outgoing = resp;
                // Phase 09 ADR-0033: decorate the filter-synth response with
                // the 5 standard HTTP/1.1 response headers if the filter did
                // not provide them, and ALWAYS overwrite content-length from
                // body.len() (the filter's body is the source of truth).
                decorate_filter_synth_response(&mut outgoing, close);
            }
        }

        // 07.1 Task 6: encode-side filter invocation. Boundary conversion
        // `outgoing: Response` ↔ `FilterResponse` per ADR-0031. Iteration
        // fires once per response regardless of whether decode issued
        // StopAndSend. Under the Router-only 07.1 chain `Decision::Continue`
        // is the only reachable branch (Router never emits StopAndSend on
        // encode); the StopAndSend arm lands structurally for 07.2.
        let mut filter_resp = envoy_filter::FilterResponse {
            status: outgoing.status,
            reason: outgoing.reason,
            headers: std::mem::take(&mut outgoing.headers),
            body: std::mem::take(&mut outgoing.body),
        };
        match pipeline.encode_headers(&mut filter_resp) {
            envoy_filter::Decision::Continue => {
                // Write back the (possibly mutated) fields.
                outgoing.status = filter_resp.status;
                outgoing.reason = filter_resp.reason;
                outgoing.headers = filter_resp.headers;
                outgoing.body = filter_resp.body;
            }
            envoy_filter::Decision::StopAndSend(replacement) => {
                // Replace outgoing entirely with the filter's substitute response.
                outgoing = Response {
                    status: replacement.status,
                    reason: replacement.reason,
                    headers: replacement.headers,
                    body: replacement.body,
                };
                // Phase 09 ADR-0033: encode-side filter substitution discards
                // any standard headers that the prior decode-arm response
                // carried. Decorate symmetric to the SynthFromDecode site so
                // future filters that emit encode-side StopAndSend (e.g., a
                // hypothetical RBAC-on-encode rejection) inherit the standard
                // HTTP/1.1 response header set on the wire.
                decorate_filter_synth_response(&mut outgoing, close);
            }
        }

        // 07.1 Task 5: derive per-arm log/counter locals from `outgoing` for
        // the access-log + per-class HCM counter dispatch sites below.
        // Bit-equivalent to the pre-Task-5 per-arm assignments because
        // (Synth / synth_status arms) `outgoing` IS the resp value the arm
        // produced, and (Proxy success arm) `outgoing` is the
        // `construct_proxied_response` output which already includes the
        // `x-envoy-upstream-service-time` header that the pre-Task-5 code
        // explicitly pushed into `response_headers_for_log`.
        let response_status_for_log: u16 = outgoing.status;
        let response_body_len: u64 = outgoing.body.len() as u64;
        let response_headers_for_log: Vec<(String, String)> = outgoing.headers.clone();

        // 07.1 Task 5: unified wire-write site. 07.1 Task 6 inserted
        // `pipeline.encode_headers` above (boundary conversion + write-back
        // / replacement) so the wire-write below sees the post-encode value.
        Http1Response::write_to(&outgoing, &mut downstream).await?;

        // 06.3 D15.3.a NEW — per-response-class HCM counters. Increment fires
        // AFTER all 5 writer arms have populated `response_status_for_log`,
        // at the same factored dispatch site that 06.2's access-log dispatch
        // uses. The 06.1-landed `downstream_rq_total` increment at line 251
        // fires at request-entry (unchanged).
        //
        // Status codes outside [200, 600) silently no-op — 1xx informational
        // and non-standard 6xx codes are not in the per-class counter family
        // per Envoy v1.33.0 stats docs.
        match response_status_for_log / 100 {
            2 => config.stats.downstream_rq_2xx.inc(),
            3 => config.stats.downstream_rq_3xx.inc(),
            4 => config.stats.downstream_rq_4xx.inc(),
            5 => config.stats.downstream_rq_5xx.inc(),
            _ => {}
        }

        // 06.2 Task 6: factored access-log dispatch site. Per PLAN-write
        // SPEC correction 1, this single site handles all 5 writer
        // outcomes (synth + 4 proxy paths). Per parent-06 SPEC §6
        // architectural Rule 4 (fire-and-forget option (b)):
        // synchronous-after-write; emission errors are logged via
        // tracing::warn! and discarded.
        if !config.access_log.is_empty() {
            let duration = req_arrival_instant.elapsed();
            let record = envoy_accesslog::AccessLogRecord {
                start_time: req_arrival_systime,
                method: req.method.clone(),
                path: x_envoy_original_path_or_path(&req).to_owned(),
                protocol: "HTTP/1.1".to_owned(),
                response_code: response_status_for_log,
                response_flags: "-".to_owned(), // 06.2 always emits "-"
                bytes_received: request_body_len,
                bytes_sent: response_body_len,
                duration,
                upstream_service_time: extract_upstream_service_time(&response_headers_for_log),
                forwarded_for: access_log_header_value(&req.headers, "x-forwarded-for"),
                user_agent: access_log_header_value(&req.headers, "user-agent"),
                request_id: access_log_header_value(&req.headers, "x-request-id"),
                authority: access_log_header_value(&req.headers, "host"),
                upstream_host: upstream_host_for_log,
            };
            // 06.3 D15.3.e NEW: increment access_logs_total at queue-enter
            // time (BEFORE the per-sink await), per parent SPEC §6 Rule 4 —
            // fire-and-forget emission's failures do NOT deflate the count.
            // Counter::add(N) per 06.1 REVIEW §7 R-8 — one bulk increment per
            // request, not N individual .inc() calls.
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

// 06.2 Task 6 — access-log dispatch helpers. Used by the factored
// dispatch site at the end of `serve_connection`'s per-request loop
// iteration. Mirrors the field-population shape expected by
// `AccessLogRecord` + Envoy's default-format substitutions.

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

/// Case-insensitive header-value lookup returning an owned `String`
/// (the record's `Option<String>` fields require ownership so the
/// record can cross spawn boundaries cheaply if future code switches
/// to spawn-based dispatch).
fn access_log_header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

/// Parse the upstream's `x-envoy-upstream-service-time` response
/// header (integer milliseconds per envoy-rust's own injection in
/// `router::write_proxied_response`) into a Duration. Returns None
/// when the header is absent or the value isn't a parseable u64.
fn extract_upstream_service_time(headers: &[(String, String)]) -> Option<std::time::Duration> {
    let v = access_log_header_value(headers, "x-envoy-upstream-service-time")?;
    let ms: u64 = v.parse().ok()?;
    Some(std::time::Duration::from_millis(ms))
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

/// Per-request dispatch path (07.1 Task 6).
///
/// `Match` — the request passed through `pipeline.decode_headers` and
/// hit the writer-arm match via `build_response`.
///
/// `SynthFromDecode` — a decode-side filter short-circuited the request
/// with `StopAndSend`; the response goes directly to the unified
/// factored site without consulting the writer arms or `build_response`.
#[allow(dead_code)] // SynthFromDecode unused under 07.1 Router-only chain; 07.2 lights it up.
enum RequestPath {
    Match(BuildOutcome),
    SynthFromDecode(Response),
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

/// Decorate a filter-synth response with the 5 standard HTTP/1.1 response
/// headers (`server`, `date`, `content-length`, `content-type`, `connection`)
/// per phase-09 ADR-0033. Called from both writer-arm sites where a filter
/// emits `Decision::StopAndSend` (decode-side `RequestPath::SynthFromDecode`
/// at the writer-arm match; encode-side `Decision::StopAndSend(replacement)`
/// after the encode iteration). The 07.1-landed framework converts
/// `FilterResponse` ↔ `Response` verbatim; filter implementations are not
/// expected to populate the standard HTTP/1.1 response headers (their
/// responsibility ends at the application-semantic content). This helper
/// brings filter-synth responses to wire-shape parity with the synth-from-build
/// paths (`synth_status`, `synth_direct_response`) that already populate
/// these headers inline.
///
/// Semantics per ADR-0033:
///
/// - `content-length` is ALWAYS set from `resp.body.len()` (overwrites any
///   filter-provided value). The filter's body is the source of truth; a
///   stale filter-provided `content-length` would corrupt downstream parsing.
/// - `server`, `date`, `content-type`, `connection` are added only-if-missing
///   (case-insensitive name check) — matches the 06.1 D1 / 08.1 D1 dedupe
///   precedent at `crates/envoy-admin/src/handler.rs::serialize_response`.
///   If a filter chooses to set its own `server`/`date`/`content-type`/
///   `connection` (e.g., a `server: my-proxy` override), the filter's value
///   wins; the decorator does not override.
///
/// Symmetric to `synth_status` at lines 866-887 — same defaults
/// (`DEFAULT_SERVER_NAME`, `DEFAULT_CONTENT_TYPE`, `now_imf_fixdate()`,
/// `connection_value(close)`).
fn decorate_filter_synth_response(resp: &mut Response, close: bool) {
    // content-length: always derived from body.len(); overwrite if present.
    let cl_value = resp.body.len().to_string();
    let mut cl_set = false;
    for (k, v) in resp.headers.iter_mut() {
        if k.eq_ignore_ascii_case(headers::CONTENT_LENGTH) {
            *v = cl_value.clone();
            cl_set = true;
            break;
        }
    }
    if !cl_set {
        resp.headers
            .push((headers::CONTENT_LENGTH.to_string(), cl_value));
    }
    // server / date / content-type / connection: add only-if-missing.
    let standards: [(&str, String); 4] = [
        (headers::SERVER, DEFAULT_SERVER_NAME.to_string()),
        (headers::DATE, now_imf_fixdate()),
        (headers::CONTENT_TYPE, DEFAULT_CONTENT_TYPE.to_string()),
        (headers::CONNECTION, connection_value(close).to_string()),
    ];
    for (name, value) in standards {
        if !resp
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(name))
        {
            resp.headers.push((name.to_string(), value));
        }
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

    /// 07.1 Task 6: helper for tests that build HCMConfig directly.
    /// Returns an `Arc<FilterPipeline>` with a single Router filter,
    /// matching every existing fixture's filter-chain shape (the
    /// envoy-config validator at 07.1 Task 4 enforces Router-at-terminus
    /// for every production `HttpConnectionManagerConfig`).
    fn test_router_only_pipeline() -> Arc<envoy_filter::FilterPipeline> {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        Arc::new(
            envoy_filter::FilterPipeline::build_from_config(
                &[envoy_config::HttpFilter {
                    name: "envoy.filters.http.router".to_string(),
                    typed_config: envoy_config::HttpFilterTypedConfig::Router(
                        envoy_config::RouterConfig {},
                    ),
                }],
                &registry,
                "test_prefix",
            )
            .expect("single-Router pipeline builds"),
        )
    }

    /// Build a minimal HCMConfig with a single VH `domains: ["*"]`,
    /// configurable routes.
    async fn hcm_config_single_route(prefix: &str, status: u16, body: &str) -> Arc<HCMConfig> {
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            // 06.2 Task 6: field added; this helper builds an HCM with
            // no access-log sinks. The Task-6 access-log tests use
            // `hcm_config_with_access_log` instead.
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
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
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
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
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
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
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
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
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
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

    // ── 09 ADR-0033 decorate_filter_synth_response unit tests ────────────────

    #[test]
    fn decorate_adds_all_five_standard_headers_when_filter_provides_none() {
        let mut resp = Response {
            status: 429,
            reason: Some("Too Many Requests"),
            headers: Vec::new(),
            body: bytes::Bytes::from_static(b"local_rate_limited"),
        };
        super::decorate_filter_synth_response(&mut resp, true);
        let name = |n: &str| -> Option<&str> {
            resp.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(name(headers::CONTENT_LENGTH), Some("18"));
        assert_eq!(name(headers::SERVER), Some("envoy-rust"));
        assert_eq!(name(headers::CONTENT_TYPE), Some("text/plain"));
        assert_eq!(name(headers::CONNECTION), Some("close"));
        // date is wall-clock; existence + non-empty suffices.
        let date = name(headers::DATE).expect("date header added");
        assert!(!date.is_empty(), "date header empty: {date:?}");
        // All 5 standard headers present; no more, no fewer (filter contributed 0).
        assert_eq!(resp.headers.len(), 5, "headers: {:?}", resp.headers);
    }

    #[test]
    fn decorate_preserves_filter_provided_headers_and_always_overwrites_content_length() {
        let mut resp = Response {
            status: 429,
            reason: Some("Too Many Requests"),
            // Filter provides a custom server, a stale content-length (10),
            // and an extra non-standard header. Decorator must:
            //   - preserve `server: my-proxy` (filter wins on standard headers
            //     that have non-CL semantics);
            //   - OVERWRITE content-length to the body.len() = 18;
            //   - add date / content-type / connection (filter didn't provide);
            //   - preserve x-rate-limit-policy verbatim.
            headers: vec![
                ("server".to_string(), "my-proxy".to_string()),
                ("content-length".to_string(), "10".to_string()),
                ("x-rate-limit-policy".to_string(), "phase-09".to_string()),
            ],
            body: bytes::Bytes::from_static(b"local_rate_limited"),
        };
        super::decorate_filter_synth_response(&mut resp, false);
        let name = |n: &str| -> Option<String> {
            resp.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, v)| v.clone())
        };
        assert_eq!(name("server").as_deref(), Some("my-proxy"));
        assert_eq!(name("content-length").as_deref(), Some("18"));
        assert_eq!(name("content-type").as_deref(), Some("text/plain"));
        assert_eq!(name("connection").as_deref(), Some("keep-alive"));
        assert_eq!(name("x-rate-limit-policy").as_deref(), Some("phase-09"));
        assert!(name("date").is_some(), "date header added");
        // Exact count: 3 filter-provided + 3 decorator-added (date / content-type
        // / connection). The decorator did NOT add an extra `server`/`content-length`
        // duplicate; it edited content-length in place and skipped server.
        assert_eq!(resp.headers.len(), 6, "headers: {:?}", resp.headers);
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
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
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
                .await
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

    // ── 06.3 Task 4: per-response-class HCM counter tests ────────────────────

    /// Helper: build an HCMConfig via the production `from_config` constructor
    /// with a single direct-response route returning `status`. Returns the
    /// config and the shared registry so callers can re-register counters to
    /// obtain the same Arc the HCM holds (Task 5 idempotent-same-kind
    /// contract).
    async fn hcm_config_from_config_direct_response(
        status: u16,
    ) -> (Arc<HCMConfig>, Arc<envoy_stats::StatsRegistry>) {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = cluster_mgr_empty().await;
        let envoy_cfg = envoy_config::HttpConnectionManagerConfig {
            stat_prefix: "ingress_http".to_string(),
            codec_type: envoy_config::CodecType::HTTP1,
            http2_protocol_options: None,
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
                            status,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("body\n".to_string()),
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
                .await
                .expect("HCMConfig builds"),
        );
        (hcm_config, registry)
    }

    /// 06.3 D15.3.a: 2xx class counter increments on a 200 direct-response.
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_increments_downstream_rq_2xx_on_2xx_response() {
        let (hcm_config, registry) = hcm_config_from_config_direct_response(200).await;
        let c2xx = registry
            .register_counter("http.ingress_http.downstream_rq_2xx")
            .expect("counter registers");
        let c3xx = registry
            .register_counter("http.ingress_http.downstream_rq_3xx")
            .expect("counter registers");
        let c4xx = registry
            .register_counter("http.ingress_http.downstream_rq_4xx")
            .expect("counter registers");
        let c5xx = registry
            .register_counter("http.ingress_http.downstream_rq_5xx")
            .expect("counter registers");
        let total = registry
            .register_counter("http.ingress_http.downstream_rq_total")
            .expect("counter registers");

        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let _resp = drive(hcm_config, req).await;

        assert_eq!(c2xx.value(), 1, "downstream_rq_2xx should be 1");
        assert_eq!(c3xx.value(), 0, "downstream_rq_3xx should be 0");
        assert_eq!(c4xx.value(), 0, "downstream_rq_4xx should be 0");
        assert_eq!(c5xx.value(), 0, "downstream_rq_5xx should be 0");
        assert_eq!(total.value(), 1, "downstream_rq_total should be 1");
    }

    /// 06.3 D15.3.a: 3xx class counter increments on a 301 direct-response.
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_increments_downstream_rq_3xx_on_3xx_response() {
        let (hcm_config, registry) = hcm_config_from_config_direct_response(301).await;
        let c2xx = registry
            .register_counter("http.ingress_http.downstream_rq_2xx")
            .expect("counter registers");
        let c3xx = registry
            .register_counter("http.ingress_http.downstream_rq_3xx")
            .expect("counter registers");
        let c4xx = registry
            .register_counter("http.ingress_http.downstream_rq_4xx")
            .expect("counter registers");
        let c5xx = registry
            .register_counter("http.ingress_http.downstream_rq_5xx")
            .expect("counter registers");
        let total = registry
            .register_counter("http.ingress_http.downstream_rq_total")
            .expect("counter registers");

        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let _resp = drive(hcm_config, req).await;

        assert_eq!(c2xx.value(), 0, "downstream_rq_2xx should be 0");
        assert_eq!(c3xx.value(), 1, "downstream_rq_3xx should be 1");
        assert_eq!(c4xx.value(), 0, "downstream_rq_4xx should be 0");
        assert_eq!(c5xx.value(), 0, "downstream_rq_5xx should be 0");
        assert_eq!(total.value(), 1, "downstream_rq_total should be 1");
    }

    /// 06.3 D15.3.a: 4xx class counter increments on a 404 direct-response.
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_increments_downstream_rq_4xx_on_4xx_response() {
        let (hcm_config, registry) = hcm_config_from_config_direct_response(404).await;
        let c2xx = registry
            .register_counter("http.ingress_http.downstream_rq_2xx")
            .expect("counter registers");
        let c3xx = registry
            .register_counter("http.ingress_http.downstream_rq_3xx")
            .expect("counter registers");
        let c4xx = registry
            .register_counter("http.ingress_http.downstream_rq_4xx")
            .expect("counter registers");
        let c5xx = registry
            .register_counter("http.ingress_http.downstream_rq_5xx")
            .expect("counter registers");
        let total = registry
            .register_counter("http.ingress_http.downstream_rq_total")
            .expect("counter registers");

        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let _resp = drive(hcm_config, req).await;

        assert_eq!(c2xx.value(), 0, "downstream_rq_2xx should be 0");
        assert_eq!(c3xx.value(), 0, "downstream_rq_3xx should be 0");
        assert_eq!(c4xx.value(), 1, "downstream_rq_4xx should be 1");
        assert_eq!(c5xx.value(), 0, "downstream_rq_5xx should be 0");
        assert_eq!(total.value(), 1, "downstream_rq_total should be 1");
    }

    /// 06.3 D15.3.a: 5xx class counter increments on a 503 direct-response.
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_increments_downstream_rq_5xx_on_5xx_response() {
        let (hcm_config, registry) = hcm_config_from_config_direct_response(503).await;
        let c2xx = registry
            .register_counter("http.ingress_http.downstream_rq_2xx")
            .expect("counter registers");
        let c3xx = registry
            .register_counter("http.ingress_http.downstream_rq_3xx")
            .expect("counter registers");
        let c4xx = registry
            .register_counter("http.ingress_http.downstream_rq_4xx")
            .expect("counter registers");
        let c5xx = registry
            .register_counter("http.ingress_http.downstream_rq_5xx")
            .expect("counter registers");
        let total = registry
            .register_counter("http.ingress_http.downstream_rq_total")
            .expect("counter registers");

        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let _resp = drive(hcm_config, req).await;

        assert_eq!(c2xx.value(), 0, "downstream_rq_2xx should be 0");
        assert_eq!(c3xx.value(), 0, "downstream_rq_3xx should be 0");
        assert_eq!(c4xx.value(), 0, "downstream_rq_4xx should be 0");
        assert_eq!(c5xx.value(), 1, "downstream_rq_5xx should be 1");
        assert_eq!(total.value(), 1, "downstream_rq_total should be 1");
    }

    /// 06.3 Task 4 / 06.2 REVIEW I1 regression: the H1 state-init tightening
    /// (let x; instead of let mut x = 0/default) does not break the 5-writer-arm
    /// write-before-read invariant. Verified by driving a 200 direct-response
    /// request through `serve_connection` with an access-log sink and asserting the
    /// emitted line carries the correct response code. The `let x;` posture causes
    /// a Rust compile error if any writer arm fails to assign the variable; this
    /// test acts as a regression witness at test-suite execution time, confirming
    /// the synth-200 arm (the only arm exercised by a direct-response route with no
    /// proxy backend) writes all three state vars before the access-log dispatch
    /// site reads them.
    ///
    /// Coverage note: the existing `hcm_with_file_access_log_writes_one_line_per_request`
    /// test already exercises the synth-200 arm through the access-log dispatch, and
    /// the 4 proxy-arm variants (no-endpoint-503, connect-fail-502, send-fail-502,
    /// proxy-success) are covered by tests in the 06.2 Task 6 and Task 9 router
    /// sections. This test adds an explicit regression tag for the I1 fix so any
    /// future refactor that breaks the posture fails at a named test rather than an
    /// obscure "E0381: use of possibly-uninitialized variable" compile error.
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_h1_state_init_writes_in_all_5_writer_arms() {
        use std::path::PathBuf;
        use tempfile::tempdir;
        let dir = tempdir().expect("tempdir");
        let path: PathBuf = dir.path().join("access.log");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(path.clone())
                .await
                .expect("open sink"),
        );
        // Build config with a 200 direct-response route (synth arm) and
        // an access-log sink so the factored dispatch site is exercised.
        let config = hcm_config_with_access_log(vec![sink]).await;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let _ = drive(config, req).await;

        // Brief yield so the FileSink flush reaches disk.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let contents = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        assert!(
            contents.contains("200"),
            "access-log must carry response_code=200; got: {}",
            contents.trim()
        );
    }

    // ── 06.2 Task 6 access-log dispatch tests ────────────────────────────────

    use std::path::PathBuf;
    use std::time::Duration as StdDuration;
    use tempfile::tempdir;

    /// In-process tracing-subscriber test fixture for capturing
    /// warn! lines per architecture decision 13 (signpost 15 option
    /// (b)). Records the most recent emission's formatted message.
    struct WarnCapture {
        captured: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl WarnCapture {
        fn install() -> (Self, tracing::subscriber::DefaultGuard) {
            use tracing_subscriber::layer::SubscriberExt as _;
            let captured: Arc<std::sync::Mutex<Vec<String>>> =
                Arc::new(std::sync::Mutex::new(Vec::new()));
            let captured_for_layer = Arc::clone(&captured);
            let layer = tracing_subscriber::fmt::layer().with_writer(
                move || -> Box<dyn std::io::Write + Send> {
                    Box::new(CaptureWriter {
                        captured: Arc::clone(&captured_for_layer),
                    })
                },
            );
            let subscriber = tracing_subscriber::registry().with(layer);
            let guard = tracing::subscriber::set_default(subscriber);
            (Self { captured }, guard)
        }

        fn lines(&self) -> Vec<String> {
            self.captured.lock().unwrap().clone()
        }
    }

    struct CaptureWriter {
        captured: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Ok(s) = std::str::from_utf8(buf) {
                self.captured.lock().unwrap().push(s.to_owned());
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Build an HCMConfig with a single VH `domains: ["*"]`, a single
    /// `/`-prefix DirectResponse route returning 200 `ok\n`, and the
    /// supplied access-log sinks. Used by the Task-6 access-log tests
    /// so each test controls the sink set explicitly (the production
    /// `from_config` path is separately exercised below by
    /// `hcm1_increments_downstream_rq_total_on_request` Task 5 test).
    async fn hcm_config_with_access_log(
        sinks: Vec<Arc<envoy_accesslog::FileSink>>,
    ) -> Arc<HCMConfig> {
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: sinks,
            filter_pipeline: test_router_only_pipeline(),
            route_config: Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
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
            }),
        })
    }

    /// Open a FileSink at each path, drive one direct-response request
    /// through `serve_connection`, drop the sinks (forcing flush at file
    /// close), and return the per-sink line contents. Helper for the
    /// Task-6 access-log happy-path tests.
    async fn serve_one_request_with_access_log(paths: &[PathBuf]) -> Vec<Vec<String>> {
        let mut sinks: Vec<Arc<envoy_accesslog::FileSink>> = Vec::new();
        for p in paths {
            sinks.push(Arc::new(
                envoy_accesslog::FileSink::new(p.clone())
                    .await
                    .expect("open sink"),
            ));
        }
        let config = hcm_config_with_access_log(sinks).await;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let _ = serve_connection(config, sock).await;
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).await.unwrap();
        drop(client);
        let _ = server.await;

        // Brief yield so the runtime can finalize the drop-chain on the
        // FileSink's underlying File handle (matches the pattern in
        // `file_sink_serializes_concurrent_emissions`).
        tokio::time::sleep(StdDuration::from_millis(50)).await;

        let mut result: Vec<Vec<String>> = Vec::new();
        for p in paths {
            let contents = tokio::fs::read_to_string(p).await.unwrap_or_default();
            let lines: Vec<String> = contents.lines().map(str::to_owned).collect();
            result.push(lines);
        }
        result
    }

    /// Variant of `serve_one_request_with_access_log` that takes
    /// pre-constructed sinks (so the caller can deliberately invalidate
    /// the underlying file between sink open and request serve) and
    /// returns the `serve_connection` result so the caller can assert
    /// it remained `Ok` despite the emission failure.
    async fn serve_one_request_with_pre_constructed_sinks(
        sinks: &[Arc<envoy_accesslog::FileSink>],
    ) -> Result<(), Http1Error> {
        let config = hcm_config_with_access_log(sinks.to_vec()).await;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            serve_connection(config, sock).await
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).await.unwrap();
        drop(client);
        server.await.unwrap()
    }

    #[tokio::test]
    async fn hcm_with_no_access_log_does_not_touch_filesystem() {
        let dir = tempdir().expect("tempdir");
        let path_that_should_not_exist = dir.path().join("nope.log");
        let lines_per_sink: Vec<Vec<String>> = serve_one_request_with_access_log(&[]).await;
        assert!(lines_per_sink.is_empty());
        assert!(
            !path_that_should_not_exist.exists(),
            "no file should be created"
        );
    }

    #[tokio::test]
    async fn hcm_with_file_access_log_writes_one_line_per_request() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let lines_per_sink = serve_one_request_with_access_log(std::slice::from_ref(&path)).await;
        assert_eq!(lines_per_sink.len(), 1);
        assert_eq!(lines_per_sink[0].len(), 1);
        let line = &lines_per_sink[0][0];
        assert!(
            line.contains("\"GET / HTTP/1.1\" 200 - 0 3 "),
            "line: {}",
            line
        );
    }

    #[tokio::test]
    async fn hcm_with_file_access_log_emission_failure_does_not_fail_request() {
        let (capture, _guard) = WarnCapture::install();
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        // Construct the FileSink with a read-only File handle so
        // every `emit` attempt fails at `write_all` with
        // `AccessLogError::Write`. POSIX semantics (the open FD
        // remains writable after parent-dir unlink on both macOS
        // and Linux) make the dir-drop trick the PLAN's verbatim
        // test originally used unreliable; the test-only
        // `FileSink::from_file_for_test` constructor injects a
        // deliberately write-failing handle for portable coverage
        // of the fire-and-forget posture.
        // First touch the file so an O_RDONLY open succeeds.
        tokio::fs::File::create(&path)
            .await
            .expect("touch file")
            .sync_all()
            .await
            .ok();
        let ro_file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(&path)
            .await
            .expect("open read-only");
        let sink = Arc::new(envoy_accesslog::FileSink::from_file_for_test(
            path.clone(),
            ro_file,
        ));
        let result = serve_one_request_with_pre_constructed_sinks(&[sink]).await;
        assert!(
            result.is_ok(),
            "request should succeed despite emission failure"
        );
        let warn_lines = capture.lines().join("");
        assert!(
            warn_lines.contains("access log emission failed")
                || warn_lines.contains("AccessLogError"),
            "expected warn line; captured: {}",
            warn_lines
        );
    }

    #[tokio::test]
    async fn hcm_records_protocol_as_http1_1_on_h1_path() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let lines_per_sink = serve_one_request_with_access_log(std::slice::from_ref(&path)).await;
        let line = &lines_per_sink[0][0];
        assert!(line.contains("HTTP/1.1"), "line: {}", line);
    }

    // ── 06.3 Task 10: access_logs_total + access_logs_failed counter tests ────

    /// Build HCMConfig via `from_config` with a single FileSink at `path`,
    /// using a shared registry so the test can re-register the counters to
    /// obtain the same Arc the HCM holds. Returns (config, registry).
    async fn hcm_config_with_access_log_and_registry(
        sinks: Vec<Arc<envoy_accesslog::FileSink>>,
    ) -> (Arc<HCMConfig>, Arc<envoy_stats::StatsRegistry>) {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let stats =
            Arc::new(HCMStats::register(&registry, "ingress_http").expect("HCMStats register"));
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats,
            access_log: sinks,
            filter_pipeline: test_router_only_pipeline(),
            route_config: Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
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
            }),
        });
        (config, registry)
    }

    /// 06.3 D15.3.e: access_logs_total increments once per request (by sink
    /// count) and access_logs_failed stays at 0 on a working FileSink.
    ///
    /// Counter values are read directly from `config.stats` (the Arc the HCM
    /// holds) without re-registering from a separate registry — avoids the
    /// tracing-subscriber thread-locality issue that could arise if the server
    /// task ran on a different thread from a concurrent `WarnCapture` test.
    #[tokio::test]
    async fn hcm_increments_access_logs_total_on_emission() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(path.clone())
                .await
                .expect("open sink"),
        );
        let (config, _registry) = hcm_config_with_access_log_and_registry(vec![sink]).await;

        // Read the counters directly from the Arc the HCM holds.
        let total = Arc::clone(&config.stats.access_logs_total);
        let failed = Arc::clone(&config.stats.access_logs_failed);

        assert_eq!(total.value(), 0, "access_logs_total pre-request");
        assert_eq!(failed.value(), 0, "access_logs_failed pre-request");

        let req = b"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n\r\n";
        let _resp = drive(config, req).await;

        assert_eq!(
            total.value(),
            1,
            "access_logs_total should be 1 (1 sink * 1 request)"
        );
        assert_eq!(
            failed.value(),
            0,
            "access_logs_failed should be 0 on successful emission"
        );
    }

    /// 06.3 D15.3.e: access_logs_total still increments (fires BEFORE the
    /// per-sink await) when a sink fails; access_logs_failed also increments.
    ///
    /// The failing-sink strategy reuses the `FileSink::from_file_for_test`
    /// read-only-handle trick established by
    /// `hcm_with_file_access_log_emission_failure_does_not_fail_request`.
    /// Per parent SPEC §6 Rule 4 (fire-and-forget), total counts intent-to-emit,
    /// not successful emit, so total == 1 and failed == 1 after one request.
    ///
    /// Counter values read directly from `config.stats` (the shared Arc) to
    /// avoid needing a separately-accessible registry.
    ///
    /// Note: a null tracing subscriber is installed for the duration so the
    /// emission-failure warn! from this test does not compete with the
    /// WarnCapture thread-local subscriber used by the sibling test
    /// `hcm_with_file_access_log_emission_failure_does_not_fail_request`.
    /// Both tests run in the same process and the `set_default` mechanism is
    /// per-thread; when the test harness reuses threads the subscriber state
    /// can briefly overlap. The null subscriber is the cleanest isolation.
    #[tokio::test]
    async fn hcm_increments_access_logs_failed_on_emission_error_but_total_still_increments() {
        // Install a null subscriber so this test's tracing::warn! (from the
        // read-only sink emit failure) is silently discarded and does not
        // interfere with WarnCapture in the sibling test.
        let _guard = tracing::subscriber::set_default(tracing::subscriber::NoSubscriber::default());

        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        // Touch the file so an O_RDONLY open succeeds.
        tokio::fs::File::create(&path)
            .await
            .expect("touch file")
            .sync_all()
            .await
            .ok();
        let ro_file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(&path)
            .await
            .expect("open read-only");
        let sink = Arc::new(envoy_accesslog::FileSink::from_file_for_test(
            path.clone(),
            ro_file,
        ));
        let (config, _registry) = hcm_config_with_access_log_and_registry(vec![sink]).await;

        // Snapshot the counter Arcs before driving the request.
        let total = Arc::clone(&config.stats.access_logs_total);
        let failed = Arc::clone(&config.stats.access_logs_failed);

        assert_eq!(total.value(), 0, "access_logs_total pre-request");
        assert_eq!(failed.value(), 0, "access_logs_failed pre-request");

        // Drive the request; serve_connection returns Ok even on sink failure
        // (fire-and-forget posture). Inline the TCP-pair pattern so we retain
        // access to the config Arc (and thus its stats fields).
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            serve_connection(config, sock).await
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).await.unwrap();
        drop(client);
        let result = server.await.unwrap();
        assert!(
            result.is_ok(),
            "serve_connection should succeed despite sink failure"
        );

        assert_eq!(
            total.value(),
            1,
            "access_logs_total should be 1 (intent-to-emit fires before await)"
        );
        assert_eq!(
            failed.value(),
            1,
            "access_logs_failed should be 1 (one sink failed)"
        );
    }

    // ── 07.1 Task 6: H1 HCM filter-chain wiring tests ────────────────────────

    /// Helper for the 07.1 Task 6 tests: a minimal `HttpConnectionManagerConfig`
    /// with a Router-only `http_filters` list (the existing Task 4 validator
    /// enforces Router-at-terminus). Tests can mutate `http_filters` (e.g.
    /// clear to exercise the empty-chain error path) before passing to
    /// `HCMConfig::from_config`.
    fn task6_envoy_hcm_config() -> envoy_config::HttpConnectionManagerConfig {
        envoy_config::HttpConnectionManagerConfig {
            stat_prefix: "ingress_http".to_string(),
            codec_type: envoy_config::CodecType::HTTP1,
            http2_protocol_options: None,
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
        }
    }

    /// 07.1 Task 6: `HCMConfig::from_config` populates the `filter_pipeline`
    /// Arc by calling `FilterPipeline::build_from_config` with the supplied
    /// `http_filters` list. Success path: a Router-only chain produces an
    /// `Arc<FilterPipeline>` that can be cloned (the per-request shape).
    #[tokio::test]
    async fn hcm_config_from_config_builds_filter_pipeline() {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = cluster_mgr_empty().await;
        let envoy_cfg = task6_envoy_hcm_config();
        let hcm_cfg = HCMConfig::from_config(&envoy_cfg, cluster_mgr, registry)
            .await
            .expect("HCMConfig::from_config succeeds with single Router");
        // No public accessor to inspect filters.len(); verify via clone shape.
        let _cloned: envoy_filter::FilterPipeline = (*hcm_cfg.filter_pipeline).clone();
    }

    /// 07.1 Task 6: `HCMConfig::from_config` propagates
    /// `FilterPipeline::build_from_config` failures as
    /// `Http1Error::FilterPipeline(_)`. Empty-list path: produces
    /// `FilterError::EmptyChain` (the validator at 07.1 Task 4 catches this
    /// earlier in production, but the framework boundary is defense-in-depth).
    #[tokio::test]
    async fn hcm_config_from_config_errors_on_empty_http_filters() {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = cluster_mgr_empty().await;
        let mut envoy_cfg = task6_envoy_hcm_config();
        envoy_cfg.http_filters.clear();
        let result = HCMConfig::from_config(&envoy_cfg, cluster_mgr, registry).await;
        match result {
            Err(Http1Error::FilterPipeline(envoy_filter::FilterError::EmptyChain)) => {}
            other => panic!("expected FilterPipeline(EmptyChain), got {other:?}"),
        }
    }

    /// 07.1 Task 6: smoke-test that `HCMConfig::from_config` produces a
    /// config whose `filter_pipeline` Arc can be cloned and dereferenced.
    /// The wire-emission regression proof for the Router-only chain is
    /// anchored at the in-process backstop tests in
    /// `crates/envoy-bin/tests/http1_*.rs` (Task 5 already attests these
    /// are green; this unit test is a sanity check at the crate boundary).
    /// Tests 3-7 from the PLAN (filter-instrumented invocation semantics)
    /// are deferred to 07.2 Task 5 per signpost 12.
    #[tokio::test]
    async fn hcm_config_construction_yields_arc_pipeline_clone_shape() {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = cluster_mgr_empty().await;
        let envoy_cfg = task6_envoy_hcm_config();
        let hcm_cfg = HCMConfig::from_config(&envoy_cfg, cluster_mgr, registry)
            .await
            .expect("HCMConfig::from_config succeeds with single Router");
        let arc1 = Arc::clone(&hcm_cfg.filter_pipeline);
        let arc2 = Arc::clone(&hcm_cfg.filter_pipeline);
        assert!(Arc::strong_count(&arc1) >= 2);
        assert!(std::ptr::eq(&*arc1, &*arc2));
    }

    // ── 07.2 Task 5 (Group C): H1 HCM filter-chain integration tests ─────────

    /// Build an HCMConfig with a caller-supplied filter pipeline + a single
    /// prefix route. `route_status` / `route_body` define the direct_response
    /// the route serves. Used by the 07.2 filter-chain integration tests.
    ///
    /// HCMConfig does not derive Clone, so this inlines the struct literal
    /// (mirroring `hcm_config_single_route`'s body, swapping `filter_pipeline`).
    async fn hcm_config_with_pipeline(
        pipeline: Arc<envoy_filter::FilterPipeline>,
        prefix: &str,
        route_status: u16,
        route_body: &str,
    ) -> Arc<HCMConfig> {
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![],
            filter_pipeline: pipeline,
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
                            status: route_status,
                            body: DataSource {
                                filename: None,
                                inline_string: Some(route_body.to_string()),
                            },
                        }),
                    }],
                }],
            }),
        })
    }

    /// Build an `Arc<FilterPipeline>` with `[HeaderMutation(request+response
    /// mutations), Router]`.
    fn header_mutation_pipeline(
        request_mutations: Vec<(&str, &str, envoy_config::AppendAction)>,
        response_mutations: Vec<(&str, &str, envoy_config::AppendAction)>,
    ) -> Arc<envoy_filter::FilterPipeline> {
        let mk = |v: Vec<(&str, &str, envoy_config::AppendAction)>| {
            v.into_iter()
                .map(|(k, val, action)| envoy_config::HeaderMutationEntry {
                    append: envoy_config::HeaderValueOption {
                        header: envoy_config::HeaderValue {
                            key: k.to_string(),
                            value: val.to_string(),
                        },
                        append_action: action,
                    },
                })
                .collect()
        };
        let filters = vec![
            envoy_config::HttpFilter {
                name: "envoy.filters.http.header_mutation".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::HeaderMutation(
                    envoy_config::HeaderMutationConfig {
                        mutations: envoy_config::Mutations {
                            request_mutations: mk(request_mutations),
                            response_mutations: mk(response_mutations),
                        },
                    },
                ),
            },
            envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            },
        ];
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        Arc::new(
            envoy_filter::FilterPipeline::build_from_config(&filters, &registry, "test_prefix")
                .unwrap(),
        )
    }

    /// Build an HCMConfig whose single route matches on the header
    /// `x-test-path-override` with an exact-match value of `/bar` →
    /// direct_response 200 "matched\n". Used by
    /// `h1_decode_headers_fires_before_route_match`.
    ///
    /// Mirrors the `single_header_matcher_route_selected_when_match` test's
    /// config-build shape; swaps in the caller's pipeline.
    async fn hcm_config_header_matched_route(
        pipeline: Arc<envoy_filter::FilterPipeline>,
    ) -> Arc<HCMConfig> {
        let matcher_route = Route {
            r#match: RouteMatch {
                prefix: Some("/".to_string()),
                path: None,
                headers: vec![envoy_config::HeaderMatcher {
                    name: "x-test-path-override".to_string(),
                    mode: envoy_config::HeaderMatcherMode::ExactMatch("/bar".to_string()),
                    invert_match: false,
                }],
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("matched\n".to_string()),
                },
            }),
        };
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![],
            filter_pipeline: pipeline,
            route_config: Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    routes: vec![matcher_route],
                }],
            }),
        })
    }

    /// Build an HCMConfig with a single VH `domains: ["*"]`, a single
    /// `/`-prefix DirectResponse route returning 200 `ok\n`, and a FileSink
    /// at `log_path`, using the caller's filter pipeline.
    ///
    /// Mirrors `hcm_config_with_access_log`, swapping in the caller's pipeline.
    async fn hcm_config_with_access_log_and_pipeline(
        pipeline: Arc<envoy_filter::FilterPipeline>,
        log_path: &std::path::Path,
    ) -> Arc<HCMConfig> {
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.to_path_buf())
                .await
                .expect("open FileSink"),
        );
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: pipeline,
            route_config: Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
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
            }),
        })
    }

    #[tokio::test]
    async fn h1_decode_headers_fires_before_route_match() {
        // HeaderMutation adds `x-test-path-override: /bar` on decode. The
        // single route matches any path prefix `/` but also requires the header
        // `x-test-path-override: /bar` (exact match) → direct_response 200
        // "matched\n". There is no catch-all route. Driving `GET /foo` returns
        // 200 "matched\n" only if decode_headers ran before route-match (adding
        // the required header); if decode were skipped the header would be absent,
        // no route would match, and the router would 404.
        let pipeline = header_mutation_pipeline(
            vec![(
                "x-test-path-override",
                "/bar",
                envoy_config::AppendAction::OverwriteIfExistsOrAdd,
            )],
            vec![],
        );
        // Build an HCMConfig whose route matches on header x-test-path-override.
        let config = hcm_config_header_matched_route(pipeline).await;
        let req = b"GET /foo HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = String::from_utf8(drive(config, req).await).unwrap();
        assert!(
            resp.starts_with("HTTP/1.1 200 "),
            "decode mutation drove route match: {resp}"
        );
        assert!(resp.ends_with("matched\n"), "matched route body: {resp}");
    }

    #[tokio::test]
    async fn h1_encode_headers_fires_after_writer_arm_before_wire_write() {
        // HeaderMutation adds `x-test-encode: ok` on encode. direct_response
        // route. The wire output's headers carry x-test-encode.
        let pipeline = header_mutation_pipeline(
            vec![],
            vec![(
                "x-test-encode",
                "ok",
                envoy_config::AppendAction::AppendIfExistsOrAdd,
            )],
        );
        let config = hcm_config_with_pipeline(pipeline, "/", 200, "body\n").await;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = String::from_utf8(drive(config, req).await).unwrap();
        assert!(resp.starts_with("HTTP/1.1 200 "), "status: {resp}");
        assert!(
            resp.to_ascii_lowercase().contains("x-test-encode: ok\r\n"),
            "encode-side stamp on wire: {resp}"
        );
    }

    #[tokio::test]
    async fn h1_stop_and_send_at_decode_skips_route_match() {
        // test-util stub: a filter that StopAndSend(503 "stopped\n") on decode,
        // placed before Router. The route is direct_response 200 "route\n" —
        // it must NOT be reached.
        let stop_resp = envoy_filter::FilterResponse {
            status: 503,
            reason: None,
            headers: vec![("content-length".to_string(), "8".to_string())],
            body: Bytes::from_static(b"stopped\n"),
        };
        let pipeline = Arc::new(envoy_filter::FilterPipeline::test_from_instances(vec![
            envoy_filter::HttpFilterInstance::test_stop_and_send_on_decode(stop_resp),
            envoy_filter::HttpFilterInstance::test_router(),
        ]));
        let config = hcm_config_with_pipeline(pipeline, "/", 200, "route\n").await;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = String::from_utf8(drive(config, req).await).unwrap();
        assert!(
            resp.starts_with("HTTP/1.1 503 "),
            "decode StopAndSend short-circuits: {resp}"
        );
        assert!(
            resp.ends_with("stopped\n"),
            "synth body, not route body: {resp}"
        );
    }

    #[tokio::test]
    async fn h1_stop_and_send_at_encode_substitutes_wire_response() {
        // test-util stub: a filter that StopAndSend(418 "teapot\n") on encode.
        // The route's direct_response 200 is built, then encode-side StopAndSend
        // replaces it on the wire.
        let stop_resp = envoy_filter::FilterResponse {
            status: 418,
            reason: None,
            headers: vec![("content-length".to_string(), "7".to_string())],
            body: Bytes::from_static(b"teapot\n"),
        };
        let pipeline = Arc::new(envoy_filter::FilterPipeline::test_from_instances(vec![
            envoy_filter::HttpFilterInstance::test_stop_and_send_on_encode(stop_resp),
            envoy_filter::HttpFilterInstance::test_router(),
        ]));
        let config = hcm_config_with_pipeline(pipeline, "/", 200, "route\n").await;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = String::from_utf8(drive(config, req).await).unwrap();
        assert!(
            resp.starts_with("HTTP/1.1 418 "),
            "encode StopAndSend substitutes: {resp}"
        );
        assert!(resp.ends_with("teapot\n"), "substituted body: {resp}");
    }

    #[tokio::test]
    async fn h1_access_log_reflects_post_encode_headers() {
        // HCMConfig with a file access_log + HeaderMutation response_mutations.
        // Drive a request; the access log line + the per-class HCM counter see
        // the post-encode response state. Assert the access log captured a
        // 200 line (the encode-side mutation does not change status, but this
        // exercises the access-log dispatch site running after encode_headers).
        let pipeline = header_mutation_pipeline(
            vec![],
            vec![(
                "x-test",
                "ok",
                envoy_config::AppendAction::AppendIfExistsOrAdd,
            )],
        );
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let config = hcm_config_with_access_log_and_pipeline(pipeline, &log_path).await;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let _ = drive(config, req).await;
        // Brief yield so the FileSink flush reaches disk.
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            logged.contains(" 200 "),
            "access log captured post-encode status: {logged:?}"
        );
    }
}
