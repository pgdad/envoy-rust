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

/// 13.2 D6 (lock-in #2): H2 HCMConfig wraps the H1 HCMConfig (the actual
/// config blob carrying routes/filters/listener-side H2 protocol options)
/// and adds an optional H2 pool manager. The earlier-phase
/// `pub type HCMConfig = Http1HCMConfig` alias is REPLACED by this struct
/// at 13.2 because the `h2_pool_mgr` type lives in envoy-http2 — adding
/// the field directly to `envoy_http1::HCMConfig` would invert the
/// existing envoy-http2 → envoy-http1 dep direction.
///
/// The H2 HCM's `serve_h2_connection` + `handle_one_stream` access
/// `config.inner.<H1 field>` for the H1-side data + `config.h2_pool_mgr`
/// for the new field. Test paths construct via `HCMConfig::wrap(inner,
/// None)` (no pool); production paths wire via
/// `HCMConfig::wrap(inner, Some(Arc::clone(&h2_pool_mgr)))` at envoy-bin.
pub struct HCMConfig {
    pub inner: Arc<Http1HCMConfig>,
    pub h2_pool_mgr: Option<Arc<crate::pool::H2PoolManager>>,
}

impl HCMConfig {
    /// Wrap an existing H1 HCMConfig with an optional H2 pool manager.
    /// `h2_pool_mgr` is `None` on test paths (the test constructs the
    /// HCMConfig wrapper directly without pool wiring) and `Some(...)`
    /// on production paths (envoy-bin always wires the pool manager).
    pub fn wrap(
        inner: Arc<Http1HCMConfig>,
        h2_pool_mgr: Option<Arc<crate::pool::H2PoolManager>>,
    ) -> Self {
        Self { inner, h2_pool_mgr }
    }
}

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
    let mut h2_conn = build_h2_server(config.inner.http2_protocol_options.as_ref())
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

/// Per-stream dispatch path (H2 mirror of envoy-http1's `RequestPath`,
/// landed at Task 6 via commit `84d68c1`). Per signpost 10 the two HCMs
/// hold separate types rather than a unified one at the framework layer.
///
/// `Match` — the H2 stream's translated request went through
/// `pipeline.decode_headers` and hit the writer-arm path via
/// `build_response`.
///
/// `SynthFromDecode` — a decode-side filter short-circuited the request
/// with `StopAndSend`; the response goes directly to `finalize_h2_stream`.
/// Unreachable under the Router-only 07.1 chain; lit by 07.2's
/// HeaderMutation.
#[allow(dead_code)] // SynthFromDecode unused under 07.1 Router-only chain; 07.2 lights it up.
enum H2RequestPath {
    Match(BuildOutcome),
    SynthFromDecode(Response),
}

/// 16 Task 5: outcome of one upstream attempt inside the H2 retry loop.
///
/// H2-local mirror of envoy-http1's `AttemptResult` (envoy-http2 must NOT
/// depend on envoy-http1's internal struct; signpost 10 keeps the two HCM
/// types separate). Pure data carrier returned by [`run_h2_attempt`]; the
/// caller (`handle_one_stream`'s proxy arm) drives all counters /
/// `record_response` / retry classification from these fields so the
/// per-attempt lifecycle (pick → dispatch [H1-or-H2 fork] → receive →
/// classify) lives in one place.
struct H2AttemptResult {
    /// The downstream response (proxied or synth) produced by this attempt.
    response: Response,
    /// The endpoint this attempt picked, if any. `None` ONLY on the
    /// `pick() -> None` path (no endpoint to attribute): the caller then skips
    /// both `record_response` and the `%UPSTREAM_HOST%` log capture. `Some` on
    /// every path that reached an endpoint (connect-fail, send-fail (Reset),
    /// overflow, real response).
    endpoint: Option<std::net::SocketAddr>,
    /// This attempt's classifiable outcome for the retry decision. `Some` for
    /// the picked-endpoint paths (upstream Response, connect-failure →
    /// ConnectFailure, send/reset → Reset); `None` for `pick() -> None` and
    /// pool-overflow synth paths (terminal, not retriable — mirrors the H1
    /// carve-out).
    outcome: Option<envoy_config::AttemptOutcome>,
    /// `true` iff a real upstream RESPONSE was received (gates the per-attempt
    /// `upstream_rq_total` tick — lock-in #5). Connect-fail / send-fail and
    /// overflow synths leave this `false`.
    upstream_response: bool,
}

/// 16 Task 5: run ONE upstream attempt on the H2 path — pick an endpoint,
/// dispatch over the cluster's upstream protocol (H1-or-H2 fork lives INSIDE
/// here so the retry loop stays protocol-agnostic), and translate the upstream
/// response into a downstream `Response`. Pure of all counter side effects
/// EXCEPT the `cluster.cx_total().inc()` per-call/connect-on-miss ticks (which
/// have always lived on those connect boundaries); every other counter and the
/// `record_response` hook are driven by the caller from [`H2AttemptResult`].
///
/// Mirrors envoy-http1's `run_attempt`. The H1 hop-by-hop strip + outbound
/// request rebuild happen per attempt (the prior attempt's `out_req` was moved
/// into `send_request`).
async fn run_h2_attempt(
    config: &HCMConfig,
    cluster: &envoy_cluster::ClusterHandle,
    cluster_name: &str,
    envoy_req: &Request,
    host_header: &str,
) -> H2AttemptResult {
    // Re-pick the endpoint each attempt — Envoy re-runs LB on every retry.
    // On `pick() -> None`, no endpoint is attributable: emit the H2 synth-502
    // (preserving the pre-phase-16 H2 pick-none shape) and return (not
    // retriable; no record_response).
    let Some(endpoint) = cluster.pick_endpoint() else {
        tracing::warn!(cluster = %cluster.name(), "no healthy endpoint — emitting 502");
        return H2AttemptResult {
            response: synth_h2_502(),
            endpoint: None,
            outcome: None,
            upstream_response: false,
        };
    };

    // Build the outbound request: strip H1 hop-by-hop headers (Connection,
    // Transfer-Encoding) mirroring envoy-http1's run_attempt. The H2 request
    // body is a buffered `Bytes` — replay is free, each attempt re-clones.
    let mut out_headers = envoy_req.headers.clone();
    out_headers.retain(|(n, _)| {
        !n.eq_ignore_ascii_case("connection") && !n.eq_ignore_ascii_case("transfer-encoding")
    });
    let out_req = envoy_http1::codec::Request {
        method: envoy_req.method.clone(),
        path: envoy_req.path.clone(),
        version: envoy_http1::codec::HttpVersion::Http11,
        headers: out_headers,
        bytes_consumed: 0,
        body: envoy_req.body.clone(),
    };

    // 13.2 D6 lock-in #8: the outer cx_active guard fires only on the H1 fork
    // (per-call connect); the H2 fork's PoolGuard owns its own ConnGaugeGuard.
    // Declared HERE so it drops AFTER the per-attempt stream closes. (See the
    // pre-Task-5 comment block for the full rationale — unchanged.)
    let _cx_guard: Option<envoy_cluster::ConnGaugeGuard> = match cluster.upstream_protocol() {
        envoy_cluster::UpstreamProtocol::Http1 => Some(cluster.cx_active_guard()),
        envoy_cluster::UpstreamProtocol::Http2 => None,
    };

    let start = Instant::now();

    // Per-attempt dispatch. The H1-or-H2 fork is INSIDE the attempt; the retry
    // loop above is protocol-agnostic.
    //
    // 16: connect failures and send/recv failures classify DISTINCTLY, mirroring
    // envoy-http1's `run_attempt` `AcquireOutcome` split. A connect failure (TCP
    // connect / pool connect error, BEFORE any request bytes left) →
    // `AttemptOutcome::ConnectFailure`; a post-connect send/recv failure →
    // `AttemptOutcome::Reset`. Collapsing both into Reset (the pre-16 H2 shape)
    // made `retry_on: connect-failure` (without `reset`) retry on H1 but NOT on
    // H2 — an observable cross-protocol asymmetry. The synth response shape on
    // every failure path is unchanged (synth_h2_502 / synth_h2_overflow).
    //
    // `Sent` carries the post-acquire send_request result (Ok = real response;
    // Err = send/recv failure → Reset). `ConnectFailure` is the connect-boundary
    // failure → ConnectFailure. `Overflow` is the byte-exact overflow-503 synth
    // (terminal, not retriable — mirrors the H1 overflow carve-out).
    enum AcquireOutcome {
        // The upstream connected and send_request resolved (Ok = real response;
        // Err = post-connect send/recv failure to be classified as Reset).
        Sent(Result<envoy_http1::Response, String>),
        // Connect-boundary failure (no request bytes left) → ConnectFailure.
        ConnectFailure,
        // Pool cap / pending overflow — terminal synth-503 (not retriable).
        Overflow(Response),
    }

    let acquire: AcquireOutcome = match cluster.upstream_protocol() {
        envoy_cluster::UpstreamProtocol::Http1 => {
            match envoy_http1::Client::connect(endpoint, host_header).await {
                Ok(mut s) => {
                    // 06.1 D4.b: per-cluster upstream_cx_total increment on
                    // successful upstream H1 connect (unchanged).
                    cluster.cx_total().inc();
                    AcquireOutcome::Sent(s.send_request(out_req).await.map_err(|e| format!("{e}")))
                }
                Err(source) => {
                    tracing::warn!(
                        cluster = %cluster.name(),
                        addr = %endpoint,
                        error = ?source,
                        "upstream connect failed (H1 fork) — returning 502",
                    );
                    AcquireOutcome::ConnectFailure
                }
            }
        }
        envoy_cluster::UpstreamProtocol::Http2 => {
            match config
                .h2_pool_mgr
                .as_ref()
                .and_then(|m| m.get(cluster_name))
            {
                Some(pool) => match pool.acquire(endpoint, host_header).await {
                    Ok(mut guard) => AcquireOutcome::Sent(
                        guard
                            .client_stream_mut()
                            .send_request(out_req)
                            .await
                            .map_err(|e| format!("{e}")),
                    ),
                    Err(crate::pool::PoolError::Connect(source)) => {
                        tracing::warn!(
                            cluster = %cluster.name(),
                            addr = %endpoint,
                            error = ?source,
                            "H2 pool connect failed — returning 502",
                        );
                        AcquireOutcome::ConnectFailure
                    }
                    // 15 D5 (lock-in #10 / C-1): cap-overflow — NO connect was
                    // attempted, so cx_total intentionally does not fire (it
                    // lives inside the pool's connect-on-miss path). Terminal
                    // overflow-503 (not retriable).
                    Err(crate::pool::PoolError::Overflow { cluster: cl, max }) => {
                        tracing::warn!(cluster = %cl, max = %max, "H2 pool overflow — emitting 503");
                        AcquireOutcome::Overflow(synth_h2_overflow())
                    }
                    // 15 D5 (lock-in #10): max_pending_requests:0 reject — like
                    // cap-overflow, no connect attempted; cx_total does not fire.
                    Err(crate::pool::PoolError::PendingOverflow { cluster: cl }) => {
                        tracing::warn!(
                            cluster = %cl,
                            "H2 pending-request overflow (max_pending_requests:0) — emitting 503",
                        );
                        AcquireOutcome::Overflow(synth_h2_overflow())
                    }
                },
                None => {
                    // No pool wired (test paths). Per-call connect + per-call
                    // cx_total.inc() preserves the pre-13.2 behavior.
                    match crate::Client::connect(endpoint, host_header).await {
                        Ok(mut s) => {
                            cluster.cx_total().inc();
                            AcquireOutcome::Sent(
                                s.send_request(out_req).await.map_err(|e| format!("{e}")),
                            )
                        }
                        Err(source) => {
                            tracing::warn!(
                                cluster = %cluster.name(),
                                addr = %endpoint,
                                error = ?source,
                                "upstream connect failed (per-call) — returning 502",
                            );
                            AcquireOutcome::ConnectFailure
                        }
                    }
                }
            }
        }
    };

    match acquire {
        AcquireOutcome::Sent(Ok(upstream_resp)) => {
            // Build the downstream response: mirror the pre-Task-5 inline header
            // policy — replace upstream `server` with `server: envoy-rust`;
            // replace or inject `date`; append x-envoy-upstream-service-time.
            let elapsed_ms = start.elapsed().as_millis();
            let now_date = envoy_http1::date::format_imf_fixdate(SystemTime::now());
            let status = upstream_resp.status;
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
            H2AttemptResult {
                response: Response {
                    status,
                    reason: upstream_resp.reason,
                    headers,
                    body: upstream_resp.body,
                },
                endpoint: Some(endpoint),
                outcome: Some(envoy_config::AttemptOutcome::Response),
                upstream_response: true,
            }
        }
        AcquireOutcome::Sent(Err(e)) => {
            // Post-connect send/recv failure → classify as Reset (the upstream
            // connected but did not deliver a complete response). The H2
            // synth-502 preserves the pre-phase-16 dispatch-failure shape.
            tracing::warn!(error = %e, "H2 listener: upstream dispatch failed — emitting 502");
            H2AttemptResult {
                response: synth_h2_502(),
                endpoint: Some(endpoint),
                outcome: Some(envoy_config::AttemptOutcome::Reset),
                upstream_response: false,
            }
        }
        AcquireOutcome::ConnectFailure => {
            // Connect-boundary failure (no request bytes left) → classify as
            // ConnectFailure, NOT Reset (mirrors envoy-http1's `run_attempt`).
            // The H2 synth-502 preserves the pre-phase-16 connect-failure shape.
            H2AttemptResult {
                response: synth_h2_502(),
                endpoint: Some(endpoint),
                outcome: Some(envoy_config::AttemptOutcome::ConnectFailure),
                upstream_response: false,
            }
        }
        AcquireOutcome::Overflow(response) => {
            // Terminal: not retriable in this phase. No upstream_rq_total tick
            // (no connect reached); the picked endpoint still gets a
            // record_response (driven by the caller), mirroring H1.
            H2AttemptResult {
                response,
                endpoint: Some(endpoint),
                outcome: None,
                upstream_response: false,
            }
        }
    }
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
    config.inner.stats.downstream_rq_total.inc();

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
    let mut envoy_req = http_to_envoy_request(req_with_body)?;

    // 07.1 Task 7: decode-side filter invocation. Clone the pipeline
    // per-stream (cheap at 07.1; structural for 07.2's HeaderMutation
    // per-stream cloning per ADR-0031). Boundary conversion
    // `envoy_http1::codec::Request` ↔ `envoy_filter::FilterRequest` via
    // `mem::take` + write-back (same shape as Task 6 at envoy-http1).
    let mut pipeline = (*config.inner.filter_pipeline).clone();
    let mut filter_req = envoy_filter::FilterRequest {
        method: std::mem::take(&mut envoy_req.method),
        path: std::mem::take(&mut envoy_req.path),
        headers: std::mem::take(&mut envoy_req.headers),
        body: envoy_req.body.take(),
    };
    let decode_decision = pipeline.decode_headers(&mut filter_req);
    // Write back the (possibly mutated) fields. Codec-state fields
    // (`version`, `bytes_consumed`) stay on envoy_req unchanged per
    // ADR-0031.
    envoy_req.method = filter_req.method;
    envoy_req.path = filter_req.path;
    envoy_req.headers = filter_req.headers;
    envoy_req.body = filter_req.body;

    // Hand to the existing 04.x route-walk. close=false because H2 has its
    // own connection lifecycle; the close flag is only meaningful for H1.
    // On `Continue`: dispatch to `build_response` and wrap in
    // `H2RequestPath::Match`. On `StopAndSend(filter_resp)`: convert
    // FilterResponse → codec-native Response, wrap in
    // `H2RequestPath::SynthFromDecode`.
    let request_path = match decode_decision {
        envoy_filter::Decision::Continue => {
            H2RequestPath::Match(build_response(
                &config.inner,
                &envoy_req,
                /* close = */ false,
            ))
        }
        envoy_filter::Decision::StopAndSend(filter_resp) => {
            H2RequestPath::SynthFromDecode(Response {
                status: filter_resp.status,
                reason: filter_resp.reason,
                headers: filter_resp.headers,
                body: filter_resp.body,
            })
        }
    };

    // 06.2 Task 7: per-stream access-log state. Populated below as the
    // build/proxy dispatch resolves the final downstream response. The
    // `upstream_host_for_log_h2` variable is `None` on synth + picker-None
    // paths and is set to the resolved endpoint on the successful proxy
    // path before any upstream IO is attempted.
    let mut upstream_host_for_log_h2: Option<String> = None;

    let resp: Response = match request_path {
        H2RequestPath::Match(outcome) => match outcome {
            BuildOutcome::Synth(r) => r,
            BuildOutcome::Proxy {
                cluster: cluster_name,
                // 16 Task 5: consume the retry policy + attempt-count flag that
                // Task 4 added to BuildOutcome::Proxy (the H1 path landed them
                // first; this is the H2 mirror).
                retry_config,
                include_attempt_count_in_response,
            } => {
                // SPEC §3 D4 H2-side: symmetric H1-or-H2 dispatch keyed on
                // cluster.upstream_protocol() (the fork now lives inside
                // `run_h2_attempt`). The validator ensures every cluster name
                // referenced from a RouteAction::Route exists in the bootstrap;
                // the .expect() is defense-in-depth (mirrors
                // envoy-http1/src/hcm.rs).
                let cluster = config
                    .inner
                    .cluster_mgr
                    .get(&cluster_name)
                    .expect("validator ensures cluster present");

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

                // 17 D5 (ADR-0046/ADR-0047): the request-budget gate
                // (max_requests). Acquired ONCE per downstream request; the
                // guard spans the ENTIRE retry loop (L9b) — bound here so it
                // lives until the final response is built, then released on
                // drop. Fires BEFORE the retry loop and BEFORE any pool/backend
                // contact (L9a gate ordering: the request breaker fires before
                // the retry breaker). On `Rejected` the request never reaches
                // the retry loop: we build the overflow synth-503 and fall
                // through to `finalize_h2_stream` (where downstream_rq_5xx +
                // access log fire, identical to the pool overflow arm).
                let request_acquire = cluster.try_acquire_request();
                // `_request_guard` holds the slot (when Acquired) for the whole
                // Proxy arm — through the entire retry loop and until `outgoing`
                // is built, then released on arm exit before the wire write
                // (constraint v: one request = one slot for its whole lifetime).
                let _request_guard: Option<envoy_cluster::RequestBudgetGuard>;
                if let envoy_cluster::BudgetAcquisition::Rejected = request_acquire {
                    _request_guard = None;
                    // The failed acquire already ticked
                    // upstream_rq_pending_overflow (§5.3 — single source of
                    // truth). L3 (ADR-0047): Envoy's overflow local reply ALSO
                    // ticks upstream_rq_5xx — mirror it here (the ONLY synth
                    // path that ticks it; the phase-16 completing-response gate
                    // for every OTHER path is untouched). upstream_rq_total is
                    // NOT ticked (constraint iv — no attempt ever dispatches).
                    cluster.upstream_rq_5xx().inc();
                    // The overflow synth-503 (81-byte body + x-envoy-overloaded)
                    // — the SAME helper the pool PendingOverflow arm uses.
                    let mut overflow_resp = synth_h2_overflow();
                    // L11: the overflow local reply carries
                    // x-envoy-attempt-count: 1 when the vhost flag is set (only
                    // the would-be first attempt; none ever dispatched).
                    if include_attempt_count_in_response {
                        overflow_resp
                            .headers
                            .push(("x-envoy-attempt-count".to_string(), "1".to_string()));
                    }
                    // Fall through to finalize_h2_stream (no pool contact,
                    // no retry loop).
                    overflow_resp
                } else {
                    // `Unlimited` (no circuit_breakers — constraint vi,
                    // byte-identical to phase-16) → None; `Acquired` → hold the
                    // slot across the whole retry loop (L9b).
                    _request_guard = match request_acquire {
                        envoy_cluster::BudgetAcquisition::Acquired(g) => Some(g),
                        _ => None, // only Unlimited reaches here; Rejected is gated above
                    };

                    // 16 Task 5: H2 retry loop (mirror of the H1 Task 4 loop). With
                    // `retry_config: None`, `max_retries == 0` so the loop runs
                    // exactly once and the path is byte-identical to the
                    // pre-phase-16 single-attempt dispatch (no retry counters tick,
                    // no x-envoy-attempt-count).
                    let max_retries = retry_config.as_ref().map_or(0, |r| r.num_retries);
                    let mut attempts: u32 = 0;
                    // Whether the FINAL attempt (the one we broke out on) was itself
                    // retriable. Assigned on every break path; read post-loop to
                    // split retry_success vs limit_exceeded (L4).
                    #[allow(unused_assignments)]
                    let mut final_retriable = false;
                    // 17 D4 (ADR-0047): retry-budget gate state. `retry_guard_slot`
                    // holds the budget slot acquired for the IN-FLIGHT retry; it is
                    // declared here so its lifetime spans the back-off + the next
                    // attempt (constraint iii). Each retry's guard is parked here and
                    // dropped at the NEXT retry's assignment (after its back-off) or at
                    // loop exit — so the slot is held across the back-off sleep and the
                    // in-flight retried attempt (constraint iii). `Unlimited` (no
                    // circuit_breakers) leaves it `None` forever (constraint iv —
                    // byte-identical to phase-16).
                    let mut retry_guard_slot: Option<envoy_cluster::RetryBudgetGuard> = None;
                    // Set true only on a budget `Rejected` exit: suppresses the
                    // post-loop success/limit-exceeded split (L7 exclusivity — the
                    // overflow counter already ticked inside `try_acquire_retry`).
                    let mut retry_budget_blocked = false;

                    let (final_response, completing_upstream_response): (Response, bool) = loop {
                        attempts += 1;

                        // Run one attempt: pick → dispatch (H1-or-H2 fork inside) →
                        // receive. All counter side effects (except the per-call /
                        // connect-on-miss cx_total ticks that live inside) are
                        // driven HERE from the returned `H2AttemptResult`.
                        let attempt = run_h2_attempt(
                            &config,
                            &cluster,
                            &cluster_name,
                            &envoy_req,
                            &host_header,
                        )
                        .await;

                        if let Some(endpoint) = attempt.endpoint {
                            // 06.2 Task 7: capture the resolved upstream endpoint for
                            // the access-log `%UPSTREAM_HOST%` token (last attempt's
                            // endpoint wins). Skipped on pick()->None.
                            upstream_host_for_log_h2 = Some(endpoint.to_string());
                        }

                        // L5: per-attempt upstream_rq_total — only for received
                        // upstream responses (single source of truth).
                        if attempt.upstream_response {
                            cluster.upstream_rq_total().inc();
                        }

                        // 14.2 D4 / lock-in #9 (L8): response-receipt hook PER
                        // ATTEMPT — each attempt feeds outlier detection. Records
                        // against the picked endpoint for Response, connect-fail,
                        // send-fail (Reset), and overflow paths (every path that
                        // reached a pick()). Skipped on pick()->None (no endpoint to
                        // attribute, lock-in #8). Inert without outlier_detection.
                        if let Some(endpoint) = attempt.endpoint {
                            cluster.record_response(endpoint, attempt.response.status);
                        }

                        // Retry decision. `final_retriable` mirrors whether THIS
                        // (final-so-far) attempt is retriable — used post-loop to
                        // split retry_success vs limit_exceeded (L4).
                        final_retriable = match attempt.outcome {
                            Some(outcome) => retry_config
                                .as_ref()
                                .is_some_and(|r| r.is_retriable(attempt.response.status, outcome)),
                            None => false,
                        };
                        if final_retriable && attempts <= max_retries {
                            // 17 D4 (ADR-0047): the retry-budget gate. A retriable
                            // outcome with attempts remaining ADDITIONALLY requires a
                            // retry-budget slot (§5.5 — composes with, never replaces,
                            // the phase-16 condition above).
                            match cluster.try_acquire_retry() {
                                // No circuit_breakers configured: never gate, zero
                                // side-effects — byte-identical to phase-16.
                                envoy_cluster::BudgetAcquisition::Unlimited => {
                                    // L4: a retry is firing. Count it, back off, loop.
                                    cluster.upstream_rq_retry().inc();
                                    if let Some(d) = envoy_config::RetryConfig::backoff(attempts) {
                                        tokio::time::sleep(d).await;
                                    }
                                    continue;
                                }
                                // Slot held: drive the retry exactly as phase-16, but
                                // park the guard in the loop-scoped slot so it lives
                                // across the back-off + the next attempt (constraint
                                // iii). Reassigning drops the prior iteration's guard.
                                envoy_cluster::BudgetAcquisition::Acquired(retry_guard) => {
                                    cluster.upstream_rq_retry().inc();
                                    if let Some(d) = envoy_config::RetryConfig::backoff(attempts) {
                                        tokio::time::sleep(d).await;
                                    }
                                    retry_guard_slot = Some(retry_guard);
                                    continue;
                                }
                                // Budget exhausted: the would-be-retried response
                                // surfaces downstream VERBATIM (L6). The overflow
                                // counter already ticked inside `try_acquire_retry`
                                // (§5.3); do NOT tick upstream_rq_retry (the retry
                                // never happens) and mark the exit so the post-loop
                                // success/limit-exceeded split is bypassed (L7).
                                envoy_cluster::BudgetAcquisition::Rejected => {
                                    retry_budget_blocked = true;
                                }
                            }
                        }
                        break (attempt.response, attempt.upstream_response);
                    };

                    // Post-loop reconciliation (mirrors H1).
                    // L5: upstream_rq_5xx reflects the COMPLETING REAL upstream
                    // response only (retried-away 5xx attempts do NOT tick it).
                    // Gated on the completing attempt having received a real upstream
                    // response — synth local replies (the no-healthy-upstream synth-
                    // 503, connect-failure synth-502, reset synth-502, and overflow
                    // synth-503 paths) do NOT tick it, preserving the pre-phase-16
                    // baseline (they never did). Single source of truth.
                    if completing_upstream_response && final_response.status / 100 == 5 {
                        cluster.upstream_rq_5xx().inc();
                    }
                    // L4: retry outcome counters. Only when at least one retry fired
                    // (attempts > 1). If the final attempt was still retriable (we
                    // ran out of budget) → limit_exceeded; else → success.
                    // 17 D4 (L7 exclusivity): a budget-blocked exit bypasses this
                    // split entirely — the overflow counter already accounted for
                    // it, and the blocked retry never fired (so attempts==1 here in
                    // the L1 case; the guard is belt-and-braces for a >0-cap
                    // exhaustion after one or more retries already fired).
                    if attempts > 1 && !retry_budget_blocked {
                        if final_retriable {
                            cluster.upstream_rq_retry_limit_exceeded().inc();
                        } else {
                            cluster.upstream_rq_retry_success().inc();
                        }
                    }
                    // Release the retry-budget slot now, before building the outgoing response,
                    // so the slot (and its gauges) reflect completion rather than lingering
                    // until this stack frame unwinds.
                    drop(retry_guard_slot);

                    let mut outgoing = final_response;

                    // L6: x-envoy-attempt-count on the downstream response, ONLY when
                    // the vhost flag is set. Emitted on ALL outcomes that reached the
                    // proxy arm (proxied responses and synths), value = total
                    // attempts.
                    if include_attempt_count_in_response {
                        outgoing
                            .headers
                            .push(("x-envoy-attempt-count".to_string(), attempts.to_string()));
                    }
                    outgoing
                } // close the `else` (request-budget Acquired/Unlimited path)
            }
        },
        H2RequestPath::SynthFromDecode(mut r) => {
            // 07.1 Task 7: decode-side filter short-circuit. Phase 11 D6:
            // decorate the filter-synth response with the standard H2 response
            // headers (closes 09 REVIEW M2 implementation arm).
            // `upstream_host_for_log_h2` stays None (no proxy attempt).
            crate::response::decorate_filter_synth_response_h2(&mut r);
            r
        }
    };

    finalize_h2_stream(
        &config,
        &mut pipeline,
        send_response,
        resp,
        req_arrival_instant,
        req_arrival_systime,
        &envoy_req,
        request_body_len,
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
    pipeline: &mut envoy_filter::FilterPipeline,
    send_response: h2::server::SendResponse<Bytes>,
    mut resp: Response,
    req_arrival_instant: Instant,
    req_arrival_systime: SystemTime,
    envoy_req: &Request,
    request_body_len: u64,
    upstream_host_for_log_h2: Option<String>,
) -> Result<(), Http2Error> {
    // 07.1 Task 7: encode-side filter invocation. Boundary conversion
    // `envoy_http1::codec::Response` ↔ `envoy_filter::FilterResponse`
    // via `mem::take` + write-back / replace (same shape as Task 6 at
    // envoy-http1's unified factored site). Under the Router-only 07.1
    // chain `Decision::Continue` is the only reachable branch; the
    // StopAndSend arm lands structurally for 07.2 HeaderMutation.
    let mut filter_resp = envoy_filter::FilterResponse {
        status: resp.status,
        reason: resp.reason,
        headers: std::mem::take(&mut resp.headers),
        body: std::mem::take(&mut resp.body),
    };
    match pipeline.encode_headers(&mut filter_resp) {
        envoy_filter::Decision::Continue => {
            resp.status = filter_resp.status;
            resp.reason = filter_resp.reason;
            resp.headers = filter_resp.headers;
            resp.body = filter_resp.body;
        }
        envoy_filter::Decision::StopAndSend(replacement) => {
            resp = Response {
                status: replacement.status,
                reason: replacement.reason,
                headers: replacement.headers,
                body: replacement.body,
            };
            // Phase 11 D6: decorate the encode-side filter-synth replacement with
            // the standard H2 response headers (symmetric to the H1 helper's
            // encode-side wiring). No phase-11 filter takes this path, but future
            // encode-side-short-circuiting H2 filters inherit it.
            crate::response::decorate_filter_synth_response_h2(&mut resp);
        }
    }

    // 07.1 Task 7 (I1 cleaned): derive the post-encode log-locals from
    // `resp` so the per-class HCM counter site (06.3) + access-log
    // dispatch site (06.2) below reflect post-encode response state.
    // The slice contract for the access-log dispatch site is preserved by
    // binding an owned Vec then re-borrowing as a slice.
    let response_status_for_log: u16 = resp.status;
    let response_body_len: u64 = resp.body.len() as u64;
    let response_headers_for_log_owned: Vec<(String, String)> = resp.headers.clone();
    let response_headers_for_log: &[(String, String)] = &response_headers_for_log_owned;

    let send_result = send_envoy_response(send_response, resp).await;

    // 06.3 D15.3.a NEW — symmetric per-response-class HCM counter increment
    // on the H2 path. `response_status_for_log` is a local derived post-encode
    // from `resp` (see the comment block above); it reflects whichever branch
    // (Continue or StopAndSend) determined the final response. 13.2 D6: the
    // `HCMConfig` wrapper now hosts the H1 stats via `config.inner.stats`.
    match response_status_for_log / 100 {
        2 => config.inner.stats.downstream_rq_2xx.inc(),
        3 => config.inner.stats.downstream_rq_3xx.inc(),
        4 => config.inner.stats.downstream_rq_4xx.inc(),
        5 => config.inner.stats.downstream_rq_5xx.inc(),
        _ => {}
    }

    // 06.2: per-stream access-log dispatch on the H2 path. Mirrors the
    // H1 factored join-point per parent-06 SPEC §3 D3.2 + PLAN-write
    // SPEC correction 2. Lands AFTER send_envoy_response returns
    // (covering both empty-body and non-empty-body emit branches).
    if !config.inner.access_log.is_empty() {
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
            .inner
            .stats
            .access_logs_total
            .add(config.inner.access_log.len() as u64);
        for sink in &config.inner.access_log {
            if let Err(err) = sink.emit(&record).await {
                // 06.3 D15.3.e NEW: count emission failures alongside the warn.
                config.inner.stats.access_logs_failed.inc();
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

/// 15 D5 (lock-in #10 / C-1; ADR-0043 §6.2 finding 3): the `max_connections` /
/// `max_pending_requests:0` overflow synth-503 on the H2 path — the H2 sibling
/// of `envoy_http1::hcm::synth_overflow`. Body is the byte-exact 81-byte Envoy
/// local-reply `upstream connect error or disconnect/reset before headers.
/// reset reason: overflow` (no trailing newline), plus `content-length` +
/// `x-envoy-overloaded: true` (the wire surfacing of Envoy's access-log-only
/// `UO` response flag). Mirrors `synth_h2_502`'s header construction — the H2
/// synth convention OMITS the `connection` header (H2 has its own connection
/// lifecycle). Routed from BOTH the H2 pool cap-overflow arm AND the
/// pending-overflow arm; this CORRECTS the pre-15 502 (which funnelled through
/// `synth_h2_502`) to a 503.
fn synth_h2_overflow() -> Response {
    let body = Bytes::from_static(
        b"upstream connect error or disconnect/reset before headers. reset reason: overflow",
    );
    Response {
        status: 503,
        reason: None,
        headers: vec![
            ("server".to_string(), "envoy-rust".to_string()),
            ("content-type".to_string(), "text/plain".to_string()),
            ("content-length".to_string(), body.len().to_string()),
            ("x-envoy-overloaded".to_string(), "true".to_string()),
        ],
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{
        AppendAction, CodecType, DataSource, DirectResponse, HeaderMatcher, HeaderMatcherMode,
        HttpConnectionManagerConfig, HttpFilter, HttpFilterTypedConfig, Route, RouteAction,
        RouteAction_Route, RouteConfiguration, RouteMatch, RouterConfig, VirtualHost,
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
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
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
            rds: None,
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
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
        // 13.2 D6: wrap the H1 HCMConfig with the new envoy_http2 HCMConfig
        // (pool manager `None` on test paths — the H2 dispatch arm falls
        // back to per-call connect when no pool is wired).
        let hcm = HCM::new(Arc::new(HCMConfig::wrap(config, None)));
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
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: None,
                        }),
                    }],
                }],
            }),
            rds: None,
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
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
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![
                    VirtualHost {
                        name: "specific".to_string(),
                        domains: vec!["test.example".to_string()],
                        include_attempt_count_in_response: false,
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
                        include_attempt_count_in_response: false,
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
            }),
            rds: None,
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let config = Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
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

    /// 13.2 Task 2 (D6): drive N sequential requests through the H2 HCM
    /// configured with an `H2PoolManager`. Assert that `cluster.cx_total`
    /// increments only once (= one upstream conn for the whole sequence).
    /// The H2 pool's stream-multiplexing semantic acquires N stream slots
    /// on the single upstream conn; `cx_total` fires only at
    /// connect-on-miss (lock-in #6, Task 1).
    ///
    /// Mirrors the H1 pool integration test shape at
    /// `crates/envoy-http1/src/hcm.rs::tests` (the 13.1 sibling test) —
    /// exercises the wired pool dispatch path end-to-end through the H2
    /// HCM (`UpstreamProtocol::Http2` arm) rather than the pool unit-test
    /// surface alone.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_hcm_pool_reuses_upstream_conn_across_sequential_requests() {
        // Spawn an H2 upstream that handles many streams on one TCP conn.
        // `spawn_upstream_h2_server` accepts a single TCP connection and
        // loops `conn.accept()` over all streams on it — exactly the
        // multiplexing shape we need to assert pool reuse.
        let (upstream_addr, _upstream_handle) = spawn_upstream_h2_server(b"h2-pool-ok").await;

        // Build the cluster manager + shared registry from a YAML
        // bootstrap so the H2 pool manager can register cluster-side
        // stats (cx_destroy + cx_http2_total) against the same registry
        // the cluster manager already populated (single-bootstrap-per-
        // process invariant: see `H2PoolManager::for_bootstrap`).
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
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
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("parse bootstrap");
        let cluster_mgr = Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("from_bootstrap"),
        );

        // Build the H2 pool manager against the same bootstrap + registry.
        let token = tokio_util::sync::CancellationToken::new();
        let pool_mgr = crate::pool::H2PoolManager::for_bootstrap(
            &bootstrap,
            &cluster_mgr,
            Arc::clone(&registry),
            token.clone(),
        )
        .expect("H2PoolManager::for_bootstrap");

        // Build the inner H1 HCMConfig (proxies "/" to "backend") and
        // wrap it with the pool manager.
        let inner = synth_h2_hcm_config_proxy(Arc::clone(&cluster_mgr)).await;
        let hcm_config = Arc::new(HCMConfig::wrap(
            Arc::clone(&inner),
            Some(Arc::clone(&pool_mgr)),
        ));

        // Re-register cx_total against the shared registry (idempotent
        // same-kind contract: returns the same Arc).
        let cx_total = registry
            .register_counter("cluster.backend.upstream_cx_total")
            .expect("cx_total registers");
        assert_eq!(cx_total.value(), 0, "starts at zero");

        // Spawn the HCM accept loop manually (the existing `spawn_h2_hcm`
        // helper wraps with `pool: None`; we need to thread the wired
        // wrapper through `HCM::new`).
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hcm = HCM::new(hcm_config);
        let server_handle = tokio::spawn(async move {
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
        let _server = TestServer {
            handle: server_handle,
        };

        // Open ONE downstream H2 client connection and drive 3 sequential
        // requests through it. The HCM accepts each stream on a fresh
        // upstream pool acquire; pool reuse on the H2 dispatch path means
        // all 3 acquires share the SAME upstream connection (cx_total
        // fires exactly once).
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });

        for _ in 0..3 {
            let req = http::Request::builder()
                .method("GET")
                .uri("http://test.example/")
                .body(())
                .unwrap();
            let (response_fut, _) = send_request.send_request(req, true).unwrap();
            let resp = response_fut.await.expect("response");
            assert_eq!(resp.status().as_u16(), 200);
            // Drain body to let the stream fully complete + the pool
            // guard drop fire (returning the stream slot to the pool).
            let (_parts, mut body) = resp.into_parts();
            while let Some(chunk) = body.data().await {
                let chunk = chunk.unwrap();
                let _ = body.flow_control().release_capacity(chunk.len());
            }
        }

        // Brief settle so the spawned handle_one_stream tasks' pool
        // releases land before we read the counter. Mirrors the
        // 100ms posture of `h2_hcm_increments_upstream_rq_total_on_200`.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            cx_total.value(),
            1,
            "3 sequential requests through the wired H2 pool must share ONE upstream conn; \
             cx_total fires only at connect-on-miss",
        );

        // Drain the pool manager (clean shutdown of the idle sweeper task).
        token.cancel();
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
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
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
            rds: None,
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        let config = Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, Arc::clone(&registry), None)
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
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
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
            rds: None,
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mut built = Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
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

    /// 14.2 D4 (lock-in #9): the H2 router-proxy arm records the FINAL upstream
    /// response status against the picked endpoint's outlier-detection state
    /// (H2 mirror of the H1 test
    /// `envoy_http1::hcm::tests::h1_router_arm_records_response_and_ejects_after_threshold`).
    /// With `consecutive_5xx: 1` and an H1 backend (wired as an H1-protocol
    /// cluster so we control the status) that returns 500, a single proxied
    /// request crosses the threshold and ejects the endpoint — proving
    /// `cluster.record_response(endpoint, upstream_resp.status)` fired with the
    /// 500 on the H2 success arm.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_router_arm_records_response_and_ejects_after_threshold() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        // Minimal H1 upstream returning 500.
        let _upstream_handle = tokio::spawn(async move {
            loop {
                if let Ok((mut tcp, _)) = upstream_listener.accept().await {
                    let mut buf = [0u8; 4096];
                    let _ = tcp.read(&mut buf).await;
                    let _ = tcp
                        .write_all(
                            b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n",
                        )
                        .await;
                    let _ = tcp.shutdown().await;
                }
            }
        });

        // Build an H1-protocol cluster (omit the H2 protocol options) with
        // `outlier_detection { consecutive_5xx: 1 }` so a single 500 ejects.
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
      outlier_detection:
        consecutive_5xx: 1
        max_ejection_percent: 100
"#,
                addr = upstream_addr.ip(),
                port = upstream_addr.port(),
            );
            let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("parse");
            Arc::new(
                envoy_cluster::from_bootstrap(
                    &bootstrap,
                    Arc::new(envoy_stats::StatsRegistry::new()),
                )
                .await
                .expect("from_bootstrap"),
            )
        };
        // Keep a handle to assert ejection after the request drives through.
        let cluster = cluster_mgr.get("backend").expect("backend cluster present");

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
        assert_eq!(resp.status().as_u16(), 500);

        // Drain body to let the stream complete.
        let (_parts, mut body) = resp.into_parts();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            let _ = body.flow_control().release_capacity(chunk.len());
        }

        // Brief settle so the spawned handle_one_stream task's record_response
        // (and the ejection it triggers) is visible from this thread.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            cluster.is_endpoint_ejected_for_test(0),
            "D4-H2: record_response(endpoint, 500) ejected the endpoint at threshold 1",
        );
    }

    // ── 07.2 Task 5 (Group D): H2 HCM filter-chain integration tests ─────────

    /// Build an H2 HCMConfig with `http_filters: [HeaderMutation, Router]` over
    /// a single direct_response route. `request_mutations` / `response_mutations`
    /// are `(key, value, AppendAction)` triples.
    async fn synth_h2_hcm_config_with_header_mutation(
        request_mutations: Vec<(&str, &str, AppendAction)>,
        response_mutations: Vec<(&str, &str, AppendAction)>,
        route_status: u16,
        route_body: &str,
    ) -> Arc<Http1HCMConfig> {
        use envoy_config::{
            HeaderMutationConfig, HeaderMutationEntry, HeaderValue, HeaderValueOption, Mutations,
        };
        let mk = |v: Vec<(&str, &str, AppendAction)>| -> Vec<HeaderMutationEntry> {
            v.into_iter()
                .map(|(k, val, action)| HeaderMutationEntry {
                    append: HeaderValueOption {
                        header: HeaderValue {
                            key: k.to_string(),
                            value: val.to_string(),
                        },
                        append_action: action,
                    },
                })
                .collect()
        };
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            access_log: vec![],
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
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
            rds: None,
            http_filters: vec![
                HttpFilter {
                    name: "envoy.filters.http.header_mutation".to_string(),
                    typed_config: HttpFilterTypedConfig::HeaderMutation(HeaderMutationConfig {
                        mutations: Mutations {
                            request_mutations: mk(request_mutations),
                            response_mutations: mk(response_mutations),
                        },
                    }),
                },
                HttpFilter {
                    name: "envoy.filters.http.router".to_string(),
                    typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
                },
            ],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
                .await
                .expect("build HCM config"),
        )
    }

    /// Build an H2 HCMConfig with a HeaderMutation filter that adds
    /// `x-h2-decode: seen` on decode, and a single route that matches on that
    /// header (exact-match `"seen"`) → direct_response 200 `"matched\n"`.
    ///
    /// Used by `h2_decode_headers_fires_before_route_match` to discriminate
    /// "decode ran before route-match" from "decode was skipped": without the
    /// mutation the header is absent and no route matches (router → 404).
    async fn synth_h2_hcm_config_header_mutation_matched_route() -> Arc<Http1HCMConfig> {
        use envoy_config::{
            HeaderMutationConfig, HeaderMutationEntry, HeaderValue, HeaderValueOption, Mutations,
        };
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            access_log: vec![],
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![HeaderMatcher {
                                name: "x-h2-decode".to_string(),
                                mode: HeaderMatcherMode::ExactMatch("seen".to_string()),
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
                    }],
                }],
            }),
            rds: None,
            http_filters: vec![
                HttpFilter {
                    name: "envoy.filters.http.header_mutation".to_string(),
                    typed_config: HttpFilterTypedConfig::HeaderMutation(HeaderMutationConfig {
                        mutations: Mutations {
                            request_mutations: vec![HeaderMutationEntry {
                                append: HeaderValueOption {
                                    header: HeaderValue {
                                        key: "x-h2-decode".to_string(),
                                        value: "seen".to_string(),
                                    },
                                    append_action: AppendAction::AppendIfExistsOrAdd,
                                },
                            }],
                            response_mutations: vec![],
                        },
                    }),
                },
                HttpFilter {
                    name: "envoy.filters.http.router".to_string(),
                    typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
                },
            ],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
                .await
                .expect("build HCM config"),
        )
    }

    /// Build an H2 HCMConfig (direct_response 200 "route\n" route) whose
    /// filter pipeline is the caller-supplied test-util pipeline.
    ///
    /// Http1HCMConfig does not derive Clone, so this inlines the struct literal
    /// (mirroring `synth_h2_hcm_config`'s body, swapping `filter_pipeline`).
    async fn synth_h2_hcm_config_with_pipeline(
        pipeline: Arc<envoy_filter::FilterPipeline>,
    ) -> Arc<Http1HCMConfig> {
        use envoy_http1::{HCMConfig, HCMStats};
        Arc::new(HCMConfig {
            stat_prefix: "test".to_string(),
            cluster_mgr: Arc::new(envoy_cluster::ClusterManager::empty()),
            http2_protocol_options: None,
            stats: Arc::new(
                HCMStats::register(&envoy_stats::StatsRegistry::new(), "test")
                    .expect("HCMStats register"),
            ),
            access_log: vec![],
            filter_pipeline: pipeline,
            pool_mgr: None,
            route_config: Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
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
                                // "route\n" intentionally differs from synth_h2_hcm_config's
                                // "ok\n": tests using this helper assert the stub response body
                                // (StopAndSend), not the route body, so the distinction is benign.
                                inline_string: Some("route\n".to_string()),
                            },
                        }),
                    }],
                }],
            }),
        })
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_decode_headers_fires_before_route_match() {
        // HeaderMutation adds `x-h2-decode: seen` on decode. The single route
        // requires that exact header via a HeaderMatcher — it only matches when
        // decode_headers ran before route-match. Without the decode mutation the
        // header is absent and no route matches, so the router would return a
        // non-200. A 200 "matched\n" response proves the mutation ran first.
        // Mirrors `h1_decode_headers_fires_before_route_match`.
        let config = synth_h2_hcm_config_header_mutation_matched_route().await;
        let (addr, _server) = spawn_h2_hcm(config).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/foo")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(
            resp.status().as_u16(),
            200,
            "decode mutation added x-h2-decode:seen, driving the header-matched route (decode ran before route-match)"
        );
        let mut body = resp.into_body();
        let mut buf = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            buf.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(
            &buf[..],
            b"matched\n",
            "route body confirms the header-matched route fired"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_encode_headers_fires_before_send_envoy_response() {
        // HeaderMutation adds x-h2-encode:ok on encode; the wire response
        // carries it.
        let config = synth_h2_hcm_config_with_header_mutation(
            vec![],
            vec![("x-h2-encode", "ok", AppendAction::AppendIfExistsOrAdd)],
            200,
            "ok\n",
        )
        .await;
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
        assert_eq!(resp.status().as_u16(), 200);
        assert_eq!(
            resp.headers().get("x-h2-encode").map(|v| v.as_bytes()),
            Some(b"ok".as_slice()),
            "encode-side stamp on the H2 wire response"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_stop_and_send_at_decode_skips_route_match() {
        let stop_resp = envoy_filter::FilterResponse {
            status: 503,
            reason: None,
            headers: vec![("content-length".to_string(), "8".to_string())],
            body: bytes::Bytes::from_static(b"stopped\n"),
        };
        let pipeline = Arc::new(envoy_filter::FilterPipeline::test_from_instances(vec![
            envoy_filter::HttpFilterInstance::test_stop_and_send_on_decode(stop_resp),
            envoy_filter::HttpFilterInstance::test_router(),
        ]));
        let config = synth_h2_hcm_config_with_pipeline(pipeline).await;
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
        assert_eq!(
            resp.status().as_u16(),
            503,
            "decode StopAndSend short-circuits route-match"
        );
        let mut body = resp.into_body();
        let mut buf = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            buf.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(&buf[..], b"stopped\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_stop_and_send_at_encode_substitutes_wire_response() {
        let stop_resp = envoy_filter::FilterResponse {
            status: 418,
            reason: None,
            headers: vec![("content-length".to_string(), "7".to_string())],
            body: bytes::Bytes::from_static(b"teapot\n"),
        };
        let pipeline = Arc::new(envoy_filter::FilterPipeline::test_from_instances(vec![
            envoy_filter::HttpFilterInstance::test_stop_and_send_on_encode(stop_resp),
            envoy_filter::HttpFilterInstance::test_router(),
        ]));
        let config = synth_h2_hcm_config_with_pipeline(pipeline).await;
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
        assert_eq!(
            resp.status().as_u16(),
            418,
            "encode StopAndSend substitutes the H2 response"
        );
        let mut body = resp.into_body();
        let mut buf = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            buf.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(&buf[..], b"teapot\n");
    }

    // ── 16 Task 5: H2 retry loop tests (mirror of the H1 Task 4 trio) ────────
    //
    // All three exercise the H1-protocol-upstream fork inside the H2 retry
    // loop (a per-request fresh-connection H1 backend whose status the test
    // controls — the simplest way to drive a stateful 503-then-200 / always-503
    // backend with a deterministic per-request count). The H2-protocol-upstream
    // fork is covered structurally: the retry loop wraps the existing
    // `match cluster.upstream_protocol()` dispatch (the fork is INSIDE the
    // per-attempt helper `run_h2_attempt`), and the pre-existing
    // `h2_proxy_outcome_dispatches_to_upstream` /
    // `h2_hcm_increments_upstream_rq_total_on_200` tests already exercise the
    // H2-upstream fork through that same loop with `max_retries == 0`.

    /// Build an H2 HCMConfig whose single route proxies "/" to the given
    /// cluster with the caller-supplied `retry_policy`, and the vhost
    /// `include_attempt_count_in_response` flag. H2 mirror of envoy-http1's
    /// `hcm_config_with_retry`.
    async fn h2_hcm_config_with_retry(
        retry_policy: Option<envoy_config::RetryPolicy>,
        include_attempt_count: bool,
        cluster_mgr: Arc<envoy_cluster::ClusterManager>,
    ) -> Arc<Http1HCMConfig> {
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test-retry".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            access_log: vec![],
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: include_attempt_count,
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy,
                        }),
                    }],
                }],
            }),
            rds: None,
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
                .await
                .expect("build HCM config"),
        )
    }

    /// Build a single-endpoint H1-protocol "backend" cluster, returning both
    /// the ClusterManager and the live ClusterHandle (so the test can read the
    /// retry counters after the request drives through). Mirrors the
    /// cluster-build shape used by `h2_router_arm_records_response_and_ejects_after_threshold`.
    async fn h1_backend_cluster(
        upstream_addr: SocketAddr,
    ) -> (
        Arc<envoy_cluster::ClusterManager>,
        envoy_cluster::ClusterHandle,
    ) {
        let cluster_mgr =
            build_cluster_mgr_with_upstream(upstream_addr, envoy_cluster::UpstreamProtocol::Http1)
                .await;
        let cluster = cluster_mgr.get("backend").expect("backend cluster present");
        (cluster_mgr, cluster)
    }

    /// Spawn a stateful in-process H1 upstream that returns `fail_status`
    /// (CL: 0) for its first `fail_count` requests, then 200 "ok" for all
    /// subsequent requests. Each request arrives on its own connection (the
    /// H2 HCM's per-call `Client::connect` H1 fallback path), so the accept
    /// loop counts attempts. Returns `(addr, request_counter)`. H2 mirror of
    /// envoy-http1's `spawn_fail_then_ok_upstream`.
    async fn spawn_fail_then_ok_h1_upstream(
        fail_status: u16,
        fail_count: usize,
    ) -> (SocketAddr, Arc<std::sync::atomic::AtomicUsize>) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_srv = Arc::clone(&counter);
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let n = counter_srv.fetch_add(1, Ordering::SeqCst);
                let mut buf = vec![0u8; 4096];
                let _ = tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    sock.read(&mut buf),
                )
                .await;
                let resp: Vec<u8> = if n < fail_count {
                    format!("HTTP/1.1 {fail_status} X\r\nContent-Length: 0\r\n\r\n").into_bytes()
                } else {
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\nok"
                        .to_vec()
                };
                let _ = sock.write_all(&resp).await;
                let _ = sock.shutdown().await;
            }
        });
        (addr, counter)
    }

    /// Drive a single GET / through an in-process H2 HCM; return the downstream
    /// response (status, headers as Vec, body Bytes). Used by the budget tests
    /// to assert on both header and body content.
    async fn drive_h2_once_with_body(
        config: Arc<Http1HCMConfig>,
    ) -> (u16, Vec<(String, String)>, bytes::Bytes) {
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
        let status = resp.status().as_u16();
        let headers: Vec<(String, String)> = resp
            .headers()
            .iter()
            .map(|(n, v)| {
                (
                    n.as_str().to_string(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect();
        let mut body_stream = resp.into_body();
        let mut body_bytes = bytes::BytesMut::new();
        while let Some(chunk) = body_stream.data().await {
            let chunk = chunk.unwrap();
            body_bytes.extend_from_slice(&chunk);
            let _ = body_stream.flow_control().release_capacity(chunk.len());
        }
        // Settle so the spawned handle_one_stream task's post-loop counter
        // increments are visible from this thread (mirrors the 100ms posture
        // of the other H2 counter tests).
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        (status, headers, body_bytes.freeze())
    }

    /// Drive a single GET / through an in-process H2 HCM wired with the given
    /// config; return the downstream response (status + collected headers as a
    /// `(name, value)` Vec). Helper for the retry tests.
    async fn drive_h2_once(config: Arc<Http1HCMConfig>) -> (u16, Vec<(String, String)>) {
        let (status, headers, _body) = drive_h2_once_with_body(config).await;
        (status, headers)
    }

    fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
        headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// 16 Task 5 (L4/L5/L6 success path): H1-upstream backend 503-then-200,
    /// retry_on 5xx, num_retries 1, vhost include_attempt_count true. Downstream
    /// 200, x-envoy-attempt-count: 2, retry=1 / retry_success=1 /
    /// limit_exceeded=0, upstream_rq_total=2, upstream_rq_5xx=0 (retried-away
    /// 503 doesn't tick). H2 mirror of `retry_success_path_503_then_200`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_retry_success_path_503_then_200() {
        let (upstream_addr, reqs) = spawn_fail_then_ok_h1_upstream(503, 1).await;
        let (cluster_mgr, cluster) = h1_backend_cluster(upstream_addr).await;
        let cfg = h2_hcm_config_with_retry(
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        )
        .await;
        let (status, headers) = drive_h2_once(cfg).await;
        assert_eq!(status, 200, "downstream must be 200 after retry");
        assert_eq!(
            header_value(&headers, "x-envoy-attempt-count"),
            Some("2"),
            "x-envoy-attempt-count: 2 expected: {headers:?}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "2 attempts"
        );
        assert_eq!(cluster.upstream_rq_retry().value(), 1, "retry");
        assert_eq!(
            cluster.upstream_rq_retry_success().value(),
            1,
            "retry_success"
        );
        assert_eq!(
            cluster.upstream_rq_retry_limit_exceeded().value(),
            0,
            "limit_exceeded"
        );
        assert_eq!(
            cluster.upstream_rq_total().value(),
            2,
            "rq_total per attempt"
        );
        assert_eq!(
            cluster.upstream_rq_5xx().value(),
            0,
            "5xx counts completing response only"
        );
    }

    /// 16 Task 5 (L4/L5/L6/L9 limit-exceeded path): always-503 H1 backend,
    /// retry_on 5xx, num_retries 1, vhost include_attempt_count true. Downstream
    /// 503 (verbatim last upstream), x-envoy-attempt-count: 2, retry=1 /
    /// retry_success=0 / limit_exceeded=1, upstream_rq_total=2, upstream_rq_5xx=1
    /// (completing 503 only). H2 mirror of `retry_limit_exceeded_path_always_503`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_retry_limit_exceeded_path_always_503() {
        let (upstream_addr, reqs) = spawn_fail_then_ok_h1_upstream(503, 1000).await;
        let (cluster_mgr, cluster) = h1_backend_cluster(upstream_addr).await;
        let cfg = h2_hcm_config_with_retry(
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        )
        .await;
        let (status, headers) = drive_h2_once(cfg).await;
        assert_eq!(status, 503, "downstream must be the last upstream 503");
        assert_eq!(
            header_value(&headers, "x-envoy-attempt-count"),
            Some("2"),
            "x-envoy-attempt-count: 2 expected: {headers:?}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "2 attempts"
        );
        assert_eq!(cluster.upstream_rq_retry().value(), 1, "retry");
        assert_eq!(
            cluster.upstream_rq_retry_success().value(),
            0,
            "retry_success"
        );
        assert_eq!(
            cluster.upstream_rq_retry_limit_exceeded().value(),
            1,
            "limit_exceeded"
        );
        assert_eq!(
            cluster.upstream_rq_total().value(),
            2,
            "rq_total per attempt"
        );
        assert_eq!(
            cluster.upstream_rq_5xx().value(),
            1,
            "5xx counts completing 503 only, not both attempts"
        );
    }

    /// 16 Task 5 (no-retry regression): NO retry_policy, H1 backend 503.
    /// Downstream 503, exactly 1 attempt, upstream_rq_total=1, upstream_rq_5xx=1,
    /// NO x-envoy-attempt-count header, all 3 retry counters 0. Proves the
    /// no-retry path is byte-identical to pre-phase-16 H2 behavior. H2 mirror
    /// of `retry_absent_no_retry_single_attempt`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_retry_absent_no_retry_single_attempt() {
        let (upstream_addr, reqs) = spawn_fail_then_ok_h1_upstream(503, 1000).await;
        let (cluster_mgr, cluster) = h1_backend_cluster(upstream_addr).await;
        let cfg = h2_hcm_config_with_retry(None, false, cluster_mgr).await;
        let (status, headers) = drive_h2_once(cfg).await;
        assert_eq!(status, 503, "downstream 503");
        assert!(
            header_value(&headers, "x-envoy-attempt-count").is_none(),
            "no attempt-count header without vhost flag: {headers:?}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "1 attempt"
        );
        assert_eq!(cluster.upstream_rq_total().value(), 1, "rq_total 1");
        assert_eq!(cluster.upstream_rq_5xx().value(), 1, "rq_5xx 1");
        assert_eq!(cluster.upstream_rq_retry().value(), 0, "retry 0");
        assert_eq!(cluster.upstream_rq_retry_success().value(), 0, "success 0");
        assert_eq!(
            cluster.upstream_rq_retry_limit_exceeded().value(),
            0,
            "limit_exceeded 0"
        );
    }

    /// 16 (connect-failure retry): H1-protocol cluster endpoint is 127.0.0.1:1
    /// (kernel-refused — a deterministic connect failure), retry_on
    /// "connect-failure", num_retries 1. The connect failure MUST classify as
    /// `AttemptOutcome::ConnectFailure` (NOT Reset) and therefore be retriable
    /// under `connect-failure` (without `reset`). Asserts: downstream synth-502,
    /// upstream_rq_retry=1 (the retry fired → ConnectFailure classification),
    /// limit_exceeded=1 (the retried attempt also refused), retry_success=0,
    /// upstream_rq_total=0 (no upstream RESPONSE was ever received). Sibling of
    /// H1's `connect_failure_retried_on_connect_failure_policy`. Pre-fix this
    /// test FAILS: H2 collapsed connect failures into Reset → not retriable
    /// under connect-failure → upstream_rq_retry stayed 0.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_connect_failure_retried_on_connect_failure_policy() {
        // 127.0.0.1:1 is kernel-refused — a deterministic connect failure.
        let refused_addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let (cluster_mgr, cluster) = h1_backend_cluster(refused_addr).await;
        let cfg = h2_hcm_config_with_retry(
            Some(envoy_config::RetryPolicy {
                retry_on: "connect-failure".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        )
        .await;
        let (status, _headers) = drive_h2_once(cfg).await;
        assert_eq!(
            status, 502,
            "downstream must be synth-502 after exhausting connect-failure retries"
        );
        assert_eq!(
            cluster.upstream_rq_retry().value(),
            1,
            "retry fired — connect failure classified as ConnectFailure (retriable under connect-failure)"
        );
        assert_eq!(
            cluster.upstream_rq_retry_limit_exceeded().value(),
            1,
            "limit_exceeded — retried attempt also refused"
        );
        assert_eq!(
            cluster.upstream_rq_retry_success().value(),
            0,
            "retry_success 0 — never succeeded"
        );
        assert_eq!(
            cluster.upstream_rq_total().value(),
            0,
            "rq_total 0 — no upstream response was ever received"
        );
    }

    /// 16 state-5 review fix: a connect-failure synth-502 with NO retry_policy
    /// (1 attempt) must NOT tick `upstream_rq_5xx`. The post-loop 5xx tick is
    /// gated on the completing attempt having received a REAL upstream response;
    /// the synth-502 (kernel-refused connect) never reached an upstream, so per
    /// ADR-0045 L5 the 1-attempt path is byte-identical to the pre-phase-16
    /// baseline. Sibling of H1's
    /// `connect_failure_synth_does_not_tick_upstream_rq_5xx`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_connect_failure_synth_does_not_tick_upstream_rq_5xx() {
        // 127.0.0.1:1 is kernel-refused — a deterministic connect failure.
        let refused_addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let (cluster_mgr, cluster) = h1_backend_cluster(refused_addr).await;
        let cfg = h2_hcm_config_with_retry(None, false, cluster_mgr).await;
        let (status, _headers) = drive_h2_once(cfg).await;
        assert_eq!(status, 502, "downstream must be connect-failure synth-502");
        assert_eq!(
            cluster.upstream_rq_5xx().value(),
            0,
            "rq_5xx 0 — synth-502 (no real upstream response) must not tick the completing-5xx counter"
        );
        assert_eq!(
            cluster.upstream_rq_total().value(),
            0,
            "rq_total 0 — no upstream response was ever received"
        );
    }

    // ── 17 Task 6: H2 retry-budget gate tests (ADR-0047) ─────────────────────

    /// 17 Task 6: like `h1_backend_cluster` but with a `circuit_breakers` block.
    /// `track_remaining` is always set so the remaining gauges register too
    /// (inert for these tests). When `max_retries` is `Some(n)` the explicit cap
    /// is emitted; `None` omits the `max_retries:` line so the default cap (3)
    /// applies. Used by the retry-budget gate tests to build a cluster whose
    /// `try_acquire_retry` actively gates. H2 mirror of H1's
    /// `cluster_mgr_with_endpoint_max_retries`.
    async fn h1_backend_cluster_with_max_retries(
        upstream_addr: SocketAddr,
        max_retries: Option<u32>,
    ) -> (
        Arc<envoy_cluster::ClusterManager>,
        envoy_cluster::ClusterHandle,
    ) {
        let max_retries_line = match max_retries {
            Some(n) => format!("            max_retries: {n}\n"),
            None => String::new(),
        };
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
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
{max_retries_line}            track_remaining: true
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
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("bootstrap parses");
        let cluster_mgr = Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
                .await
                .expect("cluster mgr"),
        );
        let cluster = cluster_mgr.get("backend").expect("backend cluster present");
        (cluster_mgr, cluster)
    }

    /// 17 Task 6 (a) budget-blocked retry (L1/L6/L7): always-503 H1 backend,
    /// retry_on 5xx, num_retries 1, but `circuit_breakers.thresholds[0]
    /// .max_retries: 0`. The FIRST attempt dispatches normally; the would-be
    /// retry is budget-rejected (L1). The downstream response is the backend's
    /// real 503 VERBATIM (L6 — not the overflow synth body, no
    /// `x-envoy-overloaded`). x-envoy-attempt-count: 1.
    /// upstream_rq_retry_overflow=1, upstream_rq_retry=0,
    /// upstream_rq_retry_limit_exceeded=0 (L7 exclusivity), upstream_rq_retry_
    /// success=0, upstream_rq_total=1, backend saw exactly 1 request.
    /// H2 mirror of H1's `budget_blocked_retry_max_retries_zero`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_budget_blocked_retry_max_retries_zero() {
        let (upstream_addr, reqs) = spawn_fail_then_ok_h1_upstream(503, 1000).await;
        let (cluster_mgr, cluster) =
            h1_backend_cluster_with_max_retries(upstream_addr, Some(0)).await;
        let cfg = h2_hcm_config_with_retry(
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        )
        .await;
        let (status, headers, body) = drive_h2_once_with_body(cfg).await;
        assert_eq!(
            status, 503,
            "downstream must be the backend's real 503 verbatim"
        );
        assert!(
            header_value(&headers, "x-envoy-overloaded").is_none(),
            "budget-blocked retry must NOT be the overflow synth (no x-envoy-overloaded): {headers:?}"
        );
        // The backend's real 503 body is empty (CL: 0 from spawn_fail_then_ok_h1_upstream).
        assert!(
            !body.starts_with(b"upstream connect error"),
            "must NOT be the 81-byte overflow synth body — the backend's real 503 surfaces verbatim (L6): {body:?}"
        );
        assert_eq!(
            header_value(&headers, "x-envoy-attempt-count"),
            Some("1"),
            "x-envoy-attempt-count: 1 — only the first attempt dispatched: {headers:?}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "backend saw exactly 1 request — the retry was budget-blocked"
        );
        assert_eq!(
            cluster.upstream_rq_retry_overflow().value(),
            1,
            "retry_overflow 1 — the would-be retry was rejected"
        );
        assert_eq!(
            cluster.upstream_rq_retry().value(),
            0,
            "retry 0 — never fired"
        );
        assert_eq!(
            cluster.upstream_rq_retry_limit_exceeded().value(),
            0,
            "limit_exceeded 0 — L7 exclusivity: only overflow ticks"
        );
        assert_eq!(
            cluster.upstream_rq_retry_success().value(),
            0,
            "retry_success 0"
        );
        assert_eq!(
            cluster.upstream_rq_total().value(),
            1,
            "rq_total 1 — only the first attempt dispatched"
        );
    }

    /// 17 Task 6 (b) budget-allowed control (L10): same shape as (a) but
    /// `max_retries` is the default (3 — never blocks a single sequential
    /// retry) and the backend is fail-once-then-succeed. The retry fires
    /// normally: downstream 200, x-envoy-attempt-count: 2, upstream_rq_retry=1,
    /// upstream_rq_retry_success=1, upstream_rq_retry_overflow=0,
    /// upstream_rq_total=2. H2 mirror of H1's `budget_allowed_retry_default_max_retries`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_budget_allowed_retry_default_max_retries() {
        let (upstream_addr, reqs) = spawn_fail_then_ok_h1_upstream(503, 1).await;
        // Default max_retries (3) — None omits the `max_retries:` line; the
        // circuit_breakers block still carries `track_remaining` so the budget
        // is configured (gating active) but the cap defaults to 3.
        let (cluster_mgr, cluster) = h1_backend_cluster_with_max_retries(upstream_addr, None).await;
        let cfg = h2_hcm_config_with_retry(
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        )
        .await;
        let (status, headers) = drive_h2_once(cfg).await;
        assert_eq!(
            status, 200,
            "downstream must be 200 after the budget-allowed retry"
        );
        assert_eq!(
            header_value(&headers, "x-envoy-attempt-count"),
            Some("2"),
            "x-envoy-attempt-count: 2 expected: {headers:?}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "2 attempts"
        );
        assert_eq!(cluster.upstream_rq_retry().value(), 1, "retry 1");
        assert_eq!(
            cluster.upstream_rq_retry_success().value(),
            1,
            "retry_success 1"
        );
        assert_eq!(
            cluster.upstream_rq_retry_overflow().value(),
            0,
            "retry_overflow 0 — default budget never blocks one sequential retry"
        );
        assert_eq!(
            cluster.upstream_rq_retry_limit_exceeded().value(),
            0,
            "limit_exceeded 0 — L7 exclusivity: only overflow ticks on budget-blocked exits"
        );
        assert_eq!(cluster.upstream_rq_total().value(), 2, "rq_total 2");
    }

    /// 17 Task 6 (c) regression (L10/iv): NO circuit_breakers at all + retry_
    /// policy + fail-once-then-succeed backend → identical retry behavior to (b)
    /// (200, x-envoy-attempt-count: 2, retry=1 / retry_success=1, rq_total=2).
    /// `try_acquire_retry` returns `Unlimited` → byte-identical to phase-16. No
    /// budget stats registered (the overflow counter is unconditional but inert
    /// at 0). H2 mirror of H1's `budget_absent_retry_unlimited_regression`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_budget_absent_retry_unlimited_regression() {
        let (upstream_addr, reqs) = spawn_fail_then_ok_h1_upstream(503, 1).await;
        let (cluster_mgr, cluster) = h1_backend_cluster(upstream_addr).await;
        let cfg = h2_hcm_config_with_retry(
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        )
        .await;
        let (status, headers) = drive_h2_once(cfg).await;
        assert_eq!(
            status, 200,
            "downstream must be 200 after retry (Unlimited budget)"
        );
        assert_eq!(
            header_value(&headers, "x-envoy-attempt-count"),
            Some("2"),
            "x-envoy-attempt-count: 2 expected: {headers:?}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "2 attempts"
        );
        assert_eq!(cluster.upstream_rq_retry().value(), 1, "retry 1");
        assert_eq!(
            cluster.upstream_rq_retry_success().value(),
            1,
            "retry_success 1"
        );
        assert_eq!(
            cluster.upstream_rq_retry_overflow().value(),
            0,
            "retry_overflow 0 — Unlimited (no circuit_breakers) never ticks it"
        );
        assert_eq!(
            cluster.upstream_rq_retry_limit_exceeded().value(),
            0,
            "limit_exceeded 0 — L7 exclusivity: only overflow ticks on budget-blocked exits"
        );
        assert_eq!(cluster.upstream_rq_total().value(), 2, "rq_total 2");
    }

    // ── 17 Task 7: H2 request-budget gate tests (max_requests; ADR-0047) ─────

    /// 17 Task 7: like `h1_backend_cluster_with_max_retries` but emits a
    /// `max_requests:` cap instead of `max_retries:`. Returns the shared
    /// `StatsRegistry` alongside the manager so tests can read the
    /// `cluster.<name>.upstream_rq_pending_overflow` counter (which has no
    /// public ClusterHandle accessor — it lives inside the BudgetState). The
    /// registry's `register_counter` is idempotent (returns the already-
    /// registered Arc), so re-registering by name reflects live ticks.
    ///
    /// The `(ClusterManager, StatsRegistry)` return shape mirrors H1's
    /// `cluster_mgr_with_endpoint_max_requests` (Task 5 docstring note).
    async fn h1_backend_cluster_with_max_requests(
        upstream_addr: SocketAddr,
        max_requests: u32,
    ) -> (
        Arc<envoy_cluster::ClusterManager>,
        Arc<envoy_stats::StatsRegistry>,
    ) {
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
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_requests: {max_requests}
            track_remaining: true
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
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("bootstrap parses");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("cluster mgr"),
        );
        (mgr, registry)
    }

    /// Read `cluster.<name>.upstream_rq_pending_overflow` from a shared
    /// registry (idempotent re-register returns the live Arc).
    fn h2_pending_overflow(registry: &envoy_stats::StatsRegistry, name: &str) -> u64 {
        registry
            .register_counter(&format!("cluster.{name}.upstream_rq_pending_overflow"))
            .expect("counter")
            .value()
    }

    /// 17 Task 7 (a) request-breaker overflow (L2/L3/L11): `max_requests: 0`
    /// (always-open request breaker), NO retry_policy, vhost
    /// include_attempt_count true. The FIRST downstream request is rejected at
    /// the dispatch entry BEFORE any backend contact → downstream 503 with the
    /// 81-byte overflow body + `x-envoy-overloaded: true` + `x-envoy-attempt-
    /// count: 1`. Backend NEVER contacted (request counter == 0).
    /// upstream_rq_pending_overflow=1, upstream_rq_5xx=1 (L3/ADR-0047 — the ONLY
    /// synth path that ticks it), upstream_rq_total=0, upstream_rq_retry_
    /// overflow=0 (L9a exclusivity). H2 mirror of H1's
    /// `request_budget_overflow_max_requests_zero`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_request_budget_overflow_max_requests_zero() {
        // Backend that records every accepted connection; never expected to fire.
        let (upstream_addr, reqs) = spawn_fail_then_ok_h1_upstream(200, 0).await;
        let (cluster_mgr, registry) = h1_backend_cluster_with_max_requests(upstream_addr, 0).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = h2_hcm_config_with_retry(None, true, cluster_mgr).await;
        let (status, headers, body) = drive_h2_once_with_body(cfg).await;
        assert_eq!(
            status, 503,
            "downstream must be the overflow synth-503: {headers:?}"
        );
        assert_eq!(
            header_value(&headers, "x-envoy-overloaded"),
            Some("true"),
            "overflow synth carries x-envoy-overloaded: true: {headers:?}"
        );
        assert_eq!(
            body.as_ref(),
            b"upstream connect error or disconnect/reset before headers. reset reason: overflow",
            "81-byte overflow body"
        );
        assert_eq!(
            header_value(&headers, "x-envoy-attempt-count"),
            Some("1"),
            "x-envoy-attempt-count: 1 (L11): {headers:?}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "backend NEVER contacted — the gate fires before pool contact"
        );
        assert_eq!(
            h2_pending_overflow(&registry, "backend"),
            1,
            "upstream_rq_pending_overflow 1"
        );
        assert_eq!(
            cluster.upstream_rq_5xx().value(),
            1,
            "upstream_rq_5xx 1 — L3/ADR-0047 reconciliation (the only synth path that ticks it)"
        );
        assert_eq!(
            cluster.upstream_rq_total().value(),
            0,
            "upstream_rq_total 0 — no attempt ever dispatched"
        );
        assert_eq!(
            cluster.upstream_rq_retry_overflow().value(),
            0,
            "upstream_rq_retry_overflow 0 — L9a exclusivity (retry budget never consulted)"
        );
    }

    /// 17 Task 7 (b) gate ordering (L9a): `max_requests: 0` AND a retry_policy.
    /// Same outcome as (a); the retry budget is never consulted (request breaker
    /// fires FIRST). upstream_rq_retry_overflow=0, upstream_rq_retry=0. H2 mirror
    /// of H1's `request_budget_gate_ordering_before_retry`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_request_budget_gate_ordering_before_retry() {
        let (upstream_addr, reqs) = spawn_fail_then_ok_h1_upstream(200, 0).await;
        let (cluster_mgr, registry) = h1_backend_cluster_with_max_requests(upstream_addr, 0).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = h2_hcm_config_with_retry(
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        )
        .await;
        let (status, headers, _body) = drive_h2_once_with_body(cfg).await;
        assert_eq!(
            status, 503,
            "same overflow synth-503 as (a) — request breaker fires before the retry breaker"
        );
        assert_eq!(
            header_value(&headers, "x-envoy-overloaded"),
            Some("true"),
            "x-envoy-overloaded: true: {headers:?}"
        );
        assert_eq!(
            header_value(&headers, "x-envoy-attempt-count"),
            Some("1"),
            "x-envoy-attempt-count: 1: {headers:?}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "backend NEVER contacted"
        );
        assert_eq!(
            h2_pending_overflow(&registry, "backend"),
            1,
            "pending_overflow 1"
        );
        assert_eq!(
            cluster.upstream_rq_5xx().value(),
            1,
            "upstream_rq_5xx 1 — same overflow path as (a), request breaker fires first"
        );
        assert_eq!(
            cluster.upstream_rq_retry_overflow().value(),
            0,
            "retry_overflow 0 — L9a: the retry budget is never consulted"
        );
        assert_eq!(
            cluster.upstream_rq_retry().value(),
            0,
            "retry 0 — no retry ever fired"
        );
    }

    /// 17 Task 7 (c) request-budget lifetime (L9b): `max_requests: 1` + retry_
    /// policy + fail-once-then-succeed backend. The request counts ONCE against
    /// the budget for its WHOLE lifetime (the guard spans the retry loop), so
    /// the sequential retry does NOT overflow `max_requests: 1`. Final 200,
    /// x-envoy-attempt-count: 2, upstream_rq_pending_overflow=0. H2 mirror of
    /// H1's `request_budget_lifetime_spans_retry`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_request_budget_lifetime_spans_retry() {
        let (upstream_addr, reqs) = spawn_fail_then_ok_h1_upstream(503, 1).await;
        let (cluster_mgr, registry) = h1_backend_cluster_with_max_requests(upstream_addr, 1).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = h2_hcm_config_with_retry(
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        )
        .await;
        let (status, headers, _body) = drive_h2_once_with_body(cfg).await;
        assert_eq!(
            status, 200,
            "final 200 — the sequential retry counts ONCE against max_requests:1 (L9b): {headers:?}"
        );
        assert_eq!(
            header_value(&headers, "x-envoy-attempt-count"),
            Some("2"),
            "x-envoy-attempt-count: 2: {headers:?}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "2 attempts — both fit under the single request-budget slot"
        );
        assert_eq!(
            h2_pending_overflow(&registry, "backend"),
            0,
            "pending_overflow 0 — one request = one slot for its whole lifetime"
        );
        assert_eq!(cluster.upstream_rq_total().value(), 2, "rq_total 2");
    }

    /// 17 Task 7 (d) regression (vi): NO circuit_breakers → no behavior change.
    /// A plain proxied request works; the request-budget gate returns Unlimited
    /// (zero side-effects). pending_overflow inert at 0. H2 mirror of H1's
    /// `request_budget_absent_unlimited_regression`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_request_budget_absent_unlimited_regression() {
        let (upstream_addr, reqs) = spawn_fail_then_ok_h1_upstream(200, 0).await;
        let (cluster_mgr, cluster) = h1_backend_cluster(upstream_addr).await;
        let cfg = h2_hcm_config_with_retry(None, false, cluster_mgr).await;
        let (status, headers, _body) = drive_h2_once_with_body(cfg).await;
        assert_eq!(
            status, 200,
            "plain proxied request unaffected by the absent request breaker: {headers:?}"
        );
        assert!(
            header_value(&headers, "x-envoy-overloaded").is_none(),
            "no overflow synth on the unlimited path: {headers:?}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "backend contacted exactly once"
        );
        assert_eq!(cluster.upstream_rq_total().value(), 1, "rq_total 1");
    }
}
