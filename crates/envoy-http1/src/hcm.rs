//! HTTP connection manager: per-listener config, per-connection state machine,
//! route walker, hardcoded router-filter call site.

use crate::client::{Client, ClientStream};
use crate::codec::{Http1Codec, HttpVersion, Request};
use crate::error::Http1Error;
use crate::headers::{self, find_header};
use crate::response::{Http1Response, Response};

use bytes::{Buf, Bytes, BytesMut};
use envoy_config::{
    AttemptOutcome, DirectResponse, HashPolicy, HttpConnectionManagerConfig, RedirectAction,
    RetryConfig, Route, RouteAction, RouteConfiguration, VirtualHost,
};
use envoy_listener::{BoxFuture, ConnectionHandler};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

const DEFAULT_SERVER_NAME: &str = "envoy-rust";
const DEFAULT_CONTENT_TYPE: &str = "text/plain";
pub(crate) const IDLE_READ_TIMEOUT: Duration = Duration::from_secs(5);
/// 25.2 M25.1-1: cap the UP-FRONT body-buffer reservation so an untrusted,
/// uncapped client `Content-Length` cannot trigger a proportional allocation
/// before any body byte arrives (a client sending only `Content-Length:
/// 4000000000` and no body would otherwise reserve ~4 GB). The buffer still
/// grows on demand via `extend_from_slice`, so the bytes actually buffered are
/// unchanged — this bounds the RESERVATION, not the read. A true per-request
/// cap tied to the buffer filter's effective limit is a deferred non-goal (the
/// effective limit is resolved later in the pipeline, not at this read site).
const INITIAL_BODY_BUF_CAP: usize = 64 * 1024;
/// 4 KiB initial capacity: typical proxied requests/responses are far below
/// this; BytesMut grows on demand for larger traffic. Halved from 8 KiB to cut
/// steady-state per-connection anon memory (2 buffers x N connections).
pub(crate) const READ_BUFFER_INITIAL_CAPACITY: usize = 4096;

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
    // fires once per record actually dispatched: a single `.inc()` inside the
    // per-sink emit loop's filter-gated branch, so a sink whose filter
    // suppresses the record does not tick. The `.inc()` precedes the emit
    // await, so per parent SPEC §6 Rule 4 sink failures do NOT deflate
    // access_logs_total — the total counts intent-to-emit, not successful emit.
    // `access_logs_failed` fires inside the per-sink Err arm alongside
    // tracing::warn!.
    /// `http.<stat_prefix>.access_logs_total` — total access-log records
    /// dispatched to sinks (one increment per accepting sink, per request).
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
    /// 26 Task 2: the route table is now a SWAPPABLE handle to support
    /// file-based RDS hot-reload (atomic pointer swap; route matching is
    /// per-request stateless). The inner `Arc<RouteConfiguration>` is the
    /// live table; an `RwLock` guards the cell so the RDS watcher (Task 4)
    /// can `store_route_config(new)` while concurrent request handlers read.
    ///
    /// No OUTER `Arc` is needed: `HCMConfig` itself is shared across all
    /// connections via a single `Arc<HCMConfig>` (it is never deep-cloned —
    /// `self.config.clone()` in the connection handlers is an `Arc::clone`
    /// pointer-bump), so every handler and the watcher reach this same
    /// `RwLock` cell, and a watcher `store` is visible to all of them.
    ///
    /// Readers MUST go through [`HCMConfig::current_route_config`], which takes
    /// the read lock, clones the inner `Arc`, and releases — yielding an OWNED
    /// snapshot. A per-request handler reads ONCE at entry and holds that clone
    /// for the request's lifetime, so a concurrent `store` does not affect an
    /// in-flight request (the §5.4 read-once guarantee).
    pub route_config: RwLock<Arc<RouteConfiguration>>,
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
    /// 13.1 Task 4: optional shared `H1PoolManager`. When `Some`, the H1
    /// proxy arm in `serve_connection` dispatches via `pool_mgr.get(cluster)`
    /// plus `H1Pool::acquire(endpoint, host)` — the production path; envoy-bin
    /// constructs the manager after `cluster_mgr` and threads it in here.
    /// When `None` — test sites that build HCMConfig as a struct-literal —
    /// the proxy arm falls back to a per-call `Client::connect`, preserving
    /// every pre-13.1 HCM unit test without a pool dependency.
    pub pool_mgr: Option<Arc<crate::pool::H1PoolManager>>,
    /// 109.1 (ADR-0176 D4): the boot runtime snapshot, built ONCE per proxy
    /// boot (`RuntimeSnapshot::from_bootstrap` in envoy-bin) and shared by
    /// Arc-clone. Read by `route_matches` to evaluate
    /// `RouteMatch.runtime_fraction` gates; `RuntimeSnapshot::default()` (the
    /// empty snapshot) makes every lookup fall back to `default_value`, which
    /// is exactly the no-`layered_runtime` semantics — the right value for
    /// test literals.
    pub runtime: Arc<envoy_config::runtime::RuntimeSnapshot>,
}

impl HCMConfig {
    pub async fn from_config(
        cfg: &HttpConnectionManagerConfig,
        cluster_mgr: Arc<envoy_cluster::ClusterManager>,
        registry: Arc<envoy_stats::StatsRegistry>,
        pool_mgr: Option<Arc<crate::pool::H1PoolManager>>,
        runtime: Arc<envoy_config::runtime::RuntimeSnapshot>,
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
            // Phase 70 Task 6: compile the optional per-record emission
            // predicate. `None` → the sink logs every record (pre-70 parity).
            let filter = entry.filter.as_ref().map(compile_access_log_filter);
            match &entry.typed_config {
                envoy_config::AccessLogTypedConfig::FileAccessLog(file_cfg) => {
                    let format = compiled_log_format(file_cfg)?;
                    let sink = envoy_accesslog::FileSink::new(
                        std::path::PathBuf::from(&file_cfg.path),
                        format,
                        filter,
                    )
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
            route_config: RwLock::new(Arc::new(
                cfg.route_config
                    .as_ref()
                    .expect("route_config populated post-load — §5.3 invariant")
                    .clone(),
            )),
            cluster_mgr,
            http2_protocol_options: cfg.http2_protocol_options.clone(),
            stats,
            access_log: access_log_sinks,
            filter_pipeline,
            pool_mgr,
            runtime,
        })
    }

    /// 26 Task 2: read the CURRENT route table — the §5.4 read-once accessor.
    /// Takes the read lock, clones the inner `Arc` (a cheap refcount bump),
    /// releases the lock, and returns the OWNED clone. A per-request handler
    /// calls this once at entry; holding the returned `Arc` snapshots the table
    /// for the request's lifetime, so a concurrent [`store_route_config`] swap
    /// does NOT affect the in-flight request.
    ///
    /// [`store_route_config`]: HCMConfig::store_route_config
    pub fn current_route_config(&self) -> Arc<RouteConfiguration> {
        // A poisoned lock means a writer panicked mid-store; the inner Arc is
        // never left in a torn state (Arc swap is a single move), so recover
        // the guard and read the (consistent) current table.
        self.route_config
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// 26 Task 2: atomically replace the live route table. Acquires the write
    /// lock and swaps the inner `Arc` for `rc`. In-flight readers that already
    /// hold an `Arc` clone from [`current_route_config`] keep their snapshot;
    /// the NEXT `current_route_config` observes `rc`. The RDS watcher (Task 4)
    /// drives this on a file change; it has no production caller this task.
    ///
    /// [`current_route_config`]: HCMConfig::current_route_config
    pub fn store_route_config(&self, rc: Arc<RouteConfiguration>) {
        // Poison-recovery (`unwrap_or_else(|p| p.into_inner())`) is safe ONLY
        // while this write critical section stays a single Arc move: a panic
        // mid-`*guard = rc` cannot tear the inner Arc (the move is atomic at the
        // pointer level), so a recovered guard always observes a consistent
        // table. The RDS reload pipeline (rds_watcher::reload) deliberately does
        // its reparse+revalidate OUTSIDE this lock so this section never widens.
        *self
            .route_config
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = rc;
    }
}

/// 28 Task 6 (ADR-0070): resolve a request's per-request hash key from the
/// matched route's `hash_policy`. MVP: use the FIRST `header` policy's
/// `header_name` (multi-policy combination + non-header sources are deferred
/// non-goals; config parse already rejects non-header sources). `lookup` returns
/// the header value bytes if the header is PRESENT (even with an EMPTY value),
/// or `None` if it is ABSENT.
///
/// THE LOAD-BEARING DISTINCTION: a PRESENT-but-EMPTY header hashes to
/// `xxh64(b"")` (`Some` — deterministic, NOT the fallback); an ABSENT header is
/// `None` (the random-host fallback). Do NOT collapse empty into absent: this is
/// exactly `lookup(..).map(hash_request_key)`, never
/// `lookup(..).filter(|v| !v.is_empty())`. An empty `hash_policy` (every
/// non-RING_HASH route) returns `None` without consulting `lookup`.
fn request_hash_key<'a>(
    policies: &[HashPolicy],
    lookup: impl Fn(&str) -> Option<&'a [u8]>,
) -> Option<u64> {
    // MVP single-header choice: first policy entry with a `header` source.
    let header_name = policies.iter().find_map(|p| p.header.as_ref())?;
    // map (NOT filter) — present-empty must map to Some(xxh64(b"")).
    lookup(&header_name.header_name).map(envoy_cluster::hash_request_key)
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

/// 16 Task 4: outcome of one upstream attempt inside the H1 retry loop.
///
/// Pure data carrier returned by [`run_attempt`]; the caller (`serve_connection`'s
/// proxy arm) drives all counters / `record_response` / retry classification from
/// these fields so the per-attempt lifecycle (pick → acquire → send → receive)
/// lives in one place and Task 5 mirrors only the clean version.
struct AttemptResult {
    /// The response (or synth) produced by this attempt.
    response: Response,
    /// True when the attempt took the zero-copy fast path: the transformed
    /// response HEAD was serialized into the caller's `direct_head_buf` and
    /// `response.headers` is intentionally EMPTY (`response.status`/`body`
    /// remain authoritative). The caller must write `direct_head_buf` + body
    /// instead of `write_to_buf`.
    direct_head: bool,
    /// The endpoint this attempt picked, if any. `None` ONLY on the
    /// `pick() -> None` path (no endpoint to attribute): the caller then skips
    /// both `record_response` and the `%UPSTREAM_HOST%` log capture. `Some` on
    /// every path that reached an endpoint (connect-fail, reset, overflow,
    /// real response).
    endpoint: Option<std::net::SocketAddr>,
    /// This attempt's classifiable outcome for the retry decision. `Some` for the
    /// picked-endpoint paths (upstream Response or connect/reset failure); `None`
    /// for `pick() -> None` and pool-overflow synth paths (terminal, not
    /// retriable — lock-in #8/#9 carve-out).
    outcome: Option<AttemptOutcome>,
    /// `true` iff a real upstream RESPONSE was received (gates the per-attempt
    /// `upstream_rq_total` tick — lock-in #5). Connect/reset failures and
    /// overflow synths leave this `false`.
    upstream_response: bool,
}

/// 16 Task 4: run ONE upstream attempt — pick an endpoint, acquire a stream
/// (pool or per-call), send the request, and receive the response. Pure of all
/// counter side effects EXCEPT `cluster.cx_total().inc()` on a per-call connect
/// (which has always lived on that connect boundary); every other counter and
/// the `record_response` hook are driven by the caller from [`AttemptResult`].
///
/// `out_headers`/body framing is rebuilt per attempt by the caller's loop, so
/// the request bits are passed in fresh.
// 30 Task 6: threading `subset_match` alongside the phase-28 `request_hash_key`
// pushes the per-attempt arg count to 8 (the pick inputs are all distinct
// request/route facts; bundling them into a struct would obscure the
// `request_hash_key`-mirror parallel this task is required to preserve).
#[allow(clippy::too_many_arguments)]
async fn run_attempt(
    config: &HCMConfig,
    cluster: &envoy_cluster::ClusterHandle,
    cluster_name: &str,
    req: &Request,
    host_header: &str,
    close: bool,
    request_hash_key: Option<u64>,
    subset_match: Option<&std::collections::BTreeMap<String, String>>,
    direct_out: Option<&mut Vec<u8>>,
) -> AttemptResult {
    // Re-pick the endpoint each attempt — Envoy re-runs LB on every retry (a
    // healthy host may have been ejected, or a round-robin cluster rotates).
    // When `pick() -> None`, no endpoint is attributable: emit the 19-byte
    // no-healthy synth-503 and return (not retriable; no record_response,
    // lock-in #8).
    let Some(endpoint) = cluster.pick_endpoint(request_hash_key, subset_match) else {
        tracing::warn!(
            cluster = %cluster.name(),
            "no healthy endpoint for cluster — returning 503",
        );
        return AttemptResult {
            response: synth_no_healthy_upstream(close),
            direct_head: false,
            endpoint: None,
            outcome: None,
            upstream_response: false,
        };
    };

    // Connection:/Transfer-Encoding: stripping (SPEC §3 D1 one-shot upstream
    // posture / RFC 7230 §3.3.3) now happens inside the serializer via
    // `send_request_borrowed(req, strip_hop_headers=true)` — the request is
    // borrowed per attempt instead of deep-cloned, which is replay-safe by
    // construction (nothing consumes it).
    //
    // Attempt-start timestamp for `x-envoy-upstream-service-time`: coarse
    // monotonic ms (see `date::coarse_monotonic_ms`) — the header's own
    // granularity is ms, and the coarse read skips the hot hardware-counter
    // path of `Instant::now()`.
    let start_ms = crate::date::coarse_monotonic_ms();

    // 13.1 Task 4 (D4): the per-attempt StreamHandle abstraction over a pooled
    // guard vs a one-shot connection.
    enum StreamHandle {
        Pooled(crate::pool::PoolGuard),
        OneShot(ClientStream),
    }

    // 13.1 Task 4 (D4): acquire from the pool when configured (production path),
    // else per-call `Client::connect` (test path with `pool_mgr: None`).
    // `cx_total.inc()` lives inside the pool's connect-on-miss branch; the
    // fallback arms keep the legacy `cluster.cx_total().inc()`.
    //
    // 16 Task 4: connect/overflow failures classify into an `AttemptOutcome`
    // for the retry decision rather than immediately surfacing a synth response.
    enum AcquireOutcome {
        // Connected; ready to send.
        Stream(StreamHandle),
        // Connect failed → synth-503, AttemptOutcome::ConnectFailure.
        ConnectFailure(Response),
        // Overflow / pending-overflow synth-503 — terminal, not retriable in
        // this phase (no AttemptOutcome).
        Overflow(Response),
    }
    let acquire: AcquireOutcome = if let Some(pool_mgr) = config.pool_mgr.as_ref() {
        match pool_mgr.get(cluster_name) {
            Some(pool) => match pool.acquire(endpoint, host_header).await {
                Ok(guard) => AcquireOutcome::Stream(StreamHandle::Pooled(guard)),
                Err(crate::pool::PoolError::Connect(source)) => {
                    tracing::warn!(
                        cluster = %cluster.name(),
                        addr = %endpoint,
                        error = ?source,
                        "upstream connect failed (pool) — returning 503",
                    );
                    AcquireOutcome::ConnectFailure(synth_status(503, close))
                }
                Err(crate::pool::PoolError::Overflow { .. }) => {
                    tracing::warn!(
                        cluster = %cluster.name(),
                        "pool overflow — returning 503",
                    );
                    AcquireOutcome::Overflow(synth_overflow(close))
                }
                Err(crate::pool::PoolError::PendingOverflow { .. }) => {
                    tracing::warn!(
                        cluster = %cluster.name(),
                        "pending-request overflow (max_pending_requests:0) — returning 503",
                    );
                    AcquireOutcome::Overflow(synth_overflow(close))
                }
            },
            None => match Client::connect(endpoint, host_header).await {
                Ok(s) => {
                    cluster.cx_total().inc();
                    AcquireOutcome::Stream(StreamHandle::OneShot(s))
                }
                Err(source) => {
                    tracing::warn!(
                        cluster = %cluster.name(),
                        addr = %endpoint,
                        error = ?source,
                        "upstream connect failed — returning 503",
                    );
                    AcquireOutcome::ConnectFailure(synth_status(503, close))
                }
            },
        }
    } else {
        match Client::connect(endpoint, host_header).await {
            Ok(s) => {
                cluster.cx_total().inc();
                AcquireOutcome::Stream(StreamHandle::OneShot(s))
            }
            Err(source) => {
                tracing::warn!(
                    cluster = %cluster.name(),
                    addr = %endpoint,
                    error = ?source,
                    "upstream connect failed — returning 503",
                );
                AcquireOutcome::ConnectFailure(synth_status(503, close))
            }
        }
    };

    // 13.1 cx_active guard: only the `OneShot` arm needs it (the Pooled
    // PoolGuard owns its own `ConnGaugeGuard` against the SAME `Arc<Gauge>`, so
    // an outer guard would double-count; the terminal arms reached no connect).
    //
    // The guard MUST drop AFTER the stream handle — the documented invariant is
    // that `upstream_cx_active` decrements only once the upstream connection
    // has closed. Rust drops locals in reverse declaration order, so `_cx_guard`
    // is declared HERE, BEFORE the `match acquire` below binds the `OneShot`
    // `ClientStream` into `handle`. The stream (declared later) drops FIRST,
    // closing the upstream connection; then `_cx_guard` drops, firing the gauge
    // decrement — the correct ordering for an `active` gauge.
    let _cx_guard: Option<envoy_cluster::ConnGaugeGuard> = match &acquire {
        AcquireOutcome::Stream(StreamHandle::OneShot(_)) => Some(cluster.cx_active_guard()),
        _ => None,
    };

    match acquire {
        AcquireOutcome::Stream(mut handle) => {
            // Direct (zero-copy head) vs owned send. `SendOk` unifies the two
            // shapes so the error arm below is shared verbatim.
            enum SendOk {
                Owned(Response),
                Direct {
                    status: u16,
                    upstream_close: bool,
                    body: bytes::Bytes,
                },
            }
            let send_result: Result<SendOk, Http1Error> = if let Some(out) = direct_out {
                let stream = match &mut handle {
                    StreamHandle::Pooled(g) => g.stream_mut(),
                    StreamHandle::OneShot(s) => s,
                };
                stream
                    .send_request_direct(req, true, start_ms, close, out)
                    .await
                    .map(|d| match d {
                        crate::client::DirectOutcome::Direct {
                            status,
                            upstream_close,
                            body,
                        } => SendOk::Direct {
                            status,
                            upstream_close,
                            body,
                        },
                        crate::client::DirectOutcome::Fallback(r) => SendOk::Owned(r),
                    })
            } else {
                match &mut handle {
                    StreamHandle::Pooled(g) => {
                        g.stream_mut().send_request_borrowed(req, true).await
                    }
                    StreamHandle::OneShot(s) => s.send_request_borrowed(req, true).await,
                }
                .map(SendOk::Owned)
            };
            match send_result {
                Ok(SendOk::Direct {
                    status,
                    upstream_close,
                    body,
                }) => {
                    // Same pooled-stream invalidation rule as the owned arm.
                    if upstream_close && let StreamHandle::Pooled(g) = &mut handle {
                        g.invalidate();
                    }
                    AttemptResult {
                        response: Response {
                            status,
                            reason: None,
                            headers: Vec::new(),
                            body,
                        },
                        direct_head: true,
                        endpoint: Some(endpoint),
                        outcome: Some(AttemptOutcome::Response),
                        upstream_response: true,
                    }
                }
                Ok(SendOk::Owned(upstream_response)) => {
                    // 23 D8.1: if the upstream responded with `Connection: close`
                    // (e.g. the http1-echo-server, which sets it unconditionally),
                    // the upstream peer has closed / will close the TCP connection
                    // after this response. Invalidate a pooled stream so Drop
                    // destroys it instead of returning it to the idle list — a
                    // stale connection in the idle list causes the next request to
                    // read UnexpectedEof. Matches Envoy's connection-management
                    // behaviour: upstream `Connection: close` signals single-use.
                    let upstream_close = upstream_response.headers.iter().any(|(n, v)| {
                        n.eq_ignore_ascii_case(headers::CONNECTION)
                            && v.eq_ignore_ascii_case("close")
                    });
                    if upstream_close && let StreamHandle::Pooled(g) = &mut handle {
                        g.invalidate();
                    }
                    let elapsed_ms = crate::date::coarse_monotonic_ms().saturating_sub(start_ms);
                    let response = crate::router::construct_proxied_response(
                        upstream_response,
                        elapsed_ms,
                        close,
                    );
                    AttemptResult {
                        response,
                        direct_head: false,
                        endpoint: Some(endpoint),
                        outcome: Some(AttemptOutcome::Response),
                        upstream_response: true,
                    }
                }
                Err(source) => {
                    // send/recv failure → classify as Reset (the upstream did
                    // not deliver a complete response). Invalidate a pooled
                    // stream so Drop destroys it + fires cx_destroy.
                    if let StreamHandle::Pooled(g) = &mut handle {
                        g.invalidate();
                    }
                    tracing::warn!(
                        cluster = %cluster.name(),
                        addr = %endpoint,
                        error = ?source,
                        "upstream request failed — returning 503",
                    );
                    AttemptResult {
                        response: synth_status(503, close),
                        direct_head: false,
                        endpoint: Some(endpoint),
                        outcome: Some(AttemptOutcome::Reset),
                        upstream_response: false,
                    }
                }
            }
        }
        AcquireOutcome::ConnectFailure(response) => AttemptResult {
            response,
            direct_head: false,
            endpoint: Some(endpoint),
            outcome: Some(AttemptOutcome::ConnectFailure),
            upstream_response: false,
        },
        AcquireOutcome::Overflow(response) => {
            // Terminal: not retriable in this phase. No upstream_rq_total tick
            // (no connect reached); the picked endpoint still gets a
            // record_response (it failed to serve), mirroring the pre-phase-16
            // behavior at the old `cluster.record_response` site.
            AttemptResult {
                response,
                direct_head: false,
                endpoint: Some(endpoint),
                outcome: None,
                upstream_response: false,
            }
        }
    }
}

/// Read the Content-Length-delimited request body: first drain what is
/// already buffered in `buf`, then read the remainder from `downstream`
/// (extracted verbatim from `serve_connection`'s per-request loop).
///
/// Returns `Ok(Some(body))` on success (`body_len == 0` yields an empty
/// `Bytes`), `Ok(None)` on an idle-read timeout mid-body (the caller maps
/// this to `serve_connection`'s graceful `return Ok(())` close), and `Err`
/// for the `UnexpectedEof` / io-error dispositions — the same three exits
/// the inlined block had.
async fn read_request_body(
    downstream: &mut TcpStream,
    buf: &mut BytesMut,
    body_len: usize,
) -> Result<Option<Bytes>, Http1Error> {
    if body_len > 0 {
        let mut body_buf = BytesMut::with_capacity(body_len.min(INITIAL_BODY_BUF_CAP));
        let from_buf = buf.len().min(body_len);
        body_buf.extend_from_slice(&buf[..from_buf]);
        buf.advance(from_buf);
        let mut remaining = body_len - from_buf;
        while remaining > 0 {
            // Read the CL-framed remainder straight into body_buf — no 4 KiB
            // stack bounce buffer + second memcpy. `take` bounds the read to
            // the declared body length so a following pipelined request's
            // bytes are never consumed (identical framing to the old
            // `chunk[..to_read]` cap). Same pattern as client.rs's response
            // body read.
            let mut limited = (&mut *downstream).take(remaining as u64);
            let n = match tokio::time::timeout(IDLE_READ_TIMEOUT, limited.read_buf(&mut body_buf))
                .await
            {
                Ok(Ok(0)) => return Err(Http1Error::UnexpectedEof),
                Ok(Ok(n)) => n,
                Ok(Err(source)) => return Err(Http1Error::Io { source }),
                Err(_elapsed) => return Ok(None),
            };
            remaining -= n;
        }
        Ok(Some(body_buf.freeze()))
    } else {
        Ok(Some(Bytes::new()))
    }
}

/// Decode-side filter invocation with its boundary conversion (extracted
/// verbatim from `serve_connection`'s per-request loop): construct
/// `FilterRequest` from the filter-visible subset of `envoy_http1::Request`,
/// invoke `decode_headers`, write back. The codec-state fields (`version`,
/// `bytes_consumed`) stay in `req` and are not surfaced to filters.
///
/// Returns the pipeline's decode decision plus the per-request dynamic
/// metadata, captured before `filter_req` is dropped so the access-log
/// record build can render %DYNAMIC_METADATA% (phase 33 T9).
fn run_decode_filters(
    pipeline: &mut envoy_filter::FilterPipeline,
    req: &mut Request,
) -> (
    envoy_filter::Decision,
    std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
) {
    let mut filter_req = envoy_filter::FilterRequest {
        method: std::mem::take(&mut req.method),
        path: std::mem::take(&mut req.path),
        headers: std::mem::take(&mut req.headers),
        body: req.body.take(),
        dynamic_metadata: std::collections::BTreeMap::new(),
    };
    let decode_decision = pipeline.decode_headers(&mut filter_req);
    // Write back the (possibly mutated) fields.
    req.method = filter_req.method;
    req.path = filter_req.path;
    req.headers = filter_req.headers;
    req.body = filter_req.body;
    // Phase 33 T9: capture the pipeline's dynamic metadata before
    // `filter_req` is dropped (a full move of the remaining field —
    // `filter_req` is already partially moved by the four write-backs).
    let dynamic_metadata = filter_req.dynamic_metadata;
    (decode_decision, dynamic_metadata)
}

async fn serve_connection(
    config: Arc<HCMConfig>,
    mut downstream: TcpStream,
) -> Result<(), Http1Error> {
    let mut buf = BytesMut::with_capacity(READ_BUFFER_INITIAL_CAPACITY);
    // Per-connection response wire buffer, reused across every keep-alive
    // request on this connection (cleared+refilled per response in
    // `write_to_buf`) so response serialization allocates once per connection
    // rather than once per response.
    let mut write_buf: Vec<u8> = Vec::with_capacity(READ_BUFFER_INITIAL_CAPACITY);
    // Per-CONNECTION filter-pipeline clone (was per-request). Safe because
    // every route-config-sensitive filter (Cors/Csrf/Buffer) fully overwrites
    // its per-request policy in `apply_route_config` — including the
    // None-clears case — and stateful filters (LocalRateLimit) share state
    // across clones by construction (per-request cloning would otherwise have
    // reset their buckets). `apply_route_config` still runs per request below.
    let mut pipeline = (*config.filter_pipeline).clone();
    // Zero-copy proxied-response fast-path eligibility, decided once per
    // connection: Router-only chain (decode/encode are no-ops) and no
    // access-log sink (the record build needs the owned header vec). The
    // per-request attempt additionally requires the vhost attempt-count
    // response header to be OFF.
    let direct_conn_eligible =
        config.access_log.is_empty() && config.filter_pipeline.is_router_only();
    // Reusable buffer for the fast path's pre-serialized response head.
    let mut direct_head_buf: Vec<u8> = Vec::new();
    // Per-connection idle-read timer, reused across every keep-alive read
    // instead of constructing a fresh `tokio::time::timeout(..)` future (and
    // its timer state) on each request's first read. It is reset to a fresh
    // IDLE_READ_TIMEOUT deadline immediately before every blocking read, so the
    // idle-close semantics are byte-identical to the former per-read `timeout`.
    // `read_buf` is cancellation-safe (it only appends on a completed read), so
    // losing the `select!` race to this timer never drops buffered bytes —
    // exactly as when `timeout` dropped the pending read future.
    let idle_timer = tokio::time::sleep(IDLE_READ_TIMEOUT);
    tokio::pin!(idle_timer);
    // 13.1 Task 4: the per-connection tier-1 micro-cache (a single cached
    // `(cluster, endpoint, ClientStream)` reused on the next request) was
    // removed at Task 4 and SUBSUMED by `H1Pool` (lock-in #5). The pool is
    // shared across every downstream connection — it observes more reuse
    // than the per-conn cache ever could — and its `cx_total.inc()` fires
    // on the SAME connect-on-miss boundary the old cache hit (lock-in #6).
    loop {
        // 1. Try parsing what's already in the buffer (for keep-alive
        //    second-and-later requests where bytes from the previous read
        //    may already contain the next request's headers).
        let req = match Http1Codec::parse_request(&buf)? {
            Some(req) => req,
            None => {
                // 2. Need more bytes. Read with the reused per-connection idle
                //    timer: arm a fresh IDLE_READ_TIMEOUT window, then race the
                //    read against it. `biased` polls the read first so a ready
                //    socket never pays for the timer branch.
                idle_timer
                    .as_mut()
                    .reset(tokio::time::Instant::now() + IDLE_READ_TIMEOUT);
                let read = tokio::select! {
                    biased;
                    res = downstream.read_buf(&mut buf) => res,
                    _ = idle_timer.as_mut() => return Ok(()), // idle timeout → clean close
                };
                match read {
                    Ok(0) => {
                        // peer closed; clean exit if the buffer is empty.
                        if buf.is_empty() {
                            return Ok(());
                        }
                        return Err(Http1Error::UnexpectedEof);
                    }
                    Ok(_) => continue, // re-parse
                    Err(source) => return Err(Http1Error::Io { source }),
                }
            }
        };

        // 06.2 Task 6: capture request-arrival timing immediately after
        // parse-success. The Instant is for duration measurement
        // (monotonic); the SystemTime is for `%START_TIME%` rendering
        // (wall-clock). Both are sampled at request-arrival per Envoy's
        // %START_TIME% semantic.
        // Perf: both samples feed ONLY the access-log record (duration /
        // %START_TIME%); when no sink is configured, skip the two clock reads —
        // on virtualized hosts each gettime is a measurable per-request cost.
        // Lazy `OnceCell`-style capture is not needed: the only consumer is the
        // single dispatch site at the bottom of this function.
        let log_enabled = !config.access_log.is_empty();
        let req_arrival_instant = log_enabled.then(std::time::Instant::now);
        let req_arrival_systime = log_enabled.then(std::time::SystemTime::now);

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

        // 4. Compute body length (for the body read + the M25.1-1 reservation bound) before consuming.
        let body_len = parse_content_length(&req.headers)?;
        let chunked = has_chunked_transfer_encoding(&req.headers);

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
        // Phase-23 D2: resolve the matched route up-front and thread its
        // per-filter config into the pipeline before decode (inert for every
        // non-CORS filter — the 07.1 foundation-slice property). MUST be
        // placed after the pipeline clone but BEFORE the mem::take below
        // (mem::take empties req's path + headers which resolve_route needs).
        // apply_route_config clones the policy into the Cors instance, so the
        // borrow of `config` via `matched_route` ends before mem::take of `req`.
        //
        // Take the §5.4 read-once route-table snapshot ONCE per request and
        // share it with `build_response_in` below: one `RwLock` read per
        // request instead of two, and the up-front resolution + the post-decode
        // re-match are backed by the identical table (a concurrent RDS swap can
        // no longer split the two views). `route_snapshot` lives to the
        // build_response_in call site in this same loop iteration.
        let route_snapshot = config.current_route_config();
        let matched_route = resolve_route_in(&route_snapshot, &req, &config.runtime);
        pipeline.apply_route_config(matched_route.as_ref().map(ResolvedRoute::route));

        // 25.1 D1: read the Content-Length-delimited request body into `req.body`
        // BEFORE the filter pipeline, so a body-dependent filter (phase 25.2's
        // buffer) can length-check it and so the router arm can forward it
        // upstream. This REPLACES the former post-response discard-drain. Chunked
        // requests carry no Content-Length (`body_len == 0`) and are 501-rejected
        // below without a body read. The idle-read-timeout → `Ok(())` graceful
        // close, the `UnexpectedEof`, and the io-error dispositions match the
        // former drain loop verbatim.
        let consumed = req.bytes_consumed;
        buf.advance(consumed);
        let request_body: Bytes =
            match read_request_body(&mut downstream, &mut buf, body_len).await? {
                Some(body) => body,
                // Idle-read timeout mid-body → graceful close (the helper's
                // `Ok(None)` maps to the former in-line `return Ok(())`).
                None => return Ok(()),
            };
        req.body = Some(request_body);

        // Boundary conversion + decode pass (see `run_decode_filters`);
        // `dynamic_metadata` feeds the access-log record build below.
        let (decode_decision, dynamic_metadata) = run_decode_filters(&mut pipeline, &mut req);

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
                    BuildOutcome::Synth(synth_501(close), None)
                } else {
                    // Reuse the per-request snapshot taken at resolve time —
                    // no second RwLock read, identical table to `resolve_route_in`.
                    build_response_in(&route_snapshot, &mut req, close, &config.runtime)
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
        // phase 43 (ADR-0100): per-request %UPSTREAM_CLUSTER%. Set at the proxy
        // ARM ENTRY (where a route resolves to a cluster) — NOT gated on
        // upstream success, mirroring Envoy: the operator renders the cluster
        // name even when the upstream attempt then fails.
        let mut upstream_cluster_for_log: Option<String> = None;
        // phase 42 (ADR-0099): per-request %RESPONSE_CODE_DETAILS%. Set by the
        // synth writer-arm (carries the BuildOutcome detail) and the
        // proxy-success arm (`via_upstream`); error/filter synths leave it None.
        let mut response_code_details_for_log: Option<String> = None;
        // phase 51 (ADR-0108): per-request %RESPONSE_FLAGS% = "URX" discriminator.
        // URX (UpstreamRetryLimitExceeded) is the FIRST flag NOT 1:1 with a unique
        // %RESPONSE_CODE_DETAILS% — the retry-limit-exceeded path's rcd is the
        // SHARED "via_upstream" (a real upstream 503, already matching Envoy), so
        // the `build_access_log_record` derive cannot key on rcd here. Set true
        // ONLY at the retry-loop
        // limit-exceeded exit (the same gate as upstream_rq_retry_limit_exceeded);
        // read by the %RESPONSE_FLAGS% derive in `build_access_log_record`.
        // `Copy` → no borrow/move
        // interaction with the rcd String. Stays false on every other path
        // (default → "-"/no-flags).
        let mut retry_limit_exceeded_for_log = false;

        // phase 52 (ADR-0109): per-request %RESPONSE_FLAGS% = "UF"
        // (UpstreamConnectionFailure) discriminator. Set true POST-LOOP when the
        // FINAL attempt's outcome was AttemptOutcome::ConnectFailure (a connect-
        // failure RETRIED to success must NOT flag UF — so this is the final
        // outcome, not a per-attempt set). Like URX, UF is NOT 1:1 with a unique
        // %RESPONSE_CODE_DETAILS% (the connect-failure rcd is the shared
        // "via_upstream"), so it keys on this boolean, not on the rcd.
        let mut connect_failure_for_log = false;

        // 07.1 Task 5: per-arm-populated response value, written to the wire
        // once below the match (factored unified-site). 07.1 Task 6 flipped
        // this from `let outgoing` to `let mut outgoing` because the
        // encode-side `Decision::StopAndSend(replacement)` branch replaces
        // `outgoing` entirely with a filter's substitute response. Rust's
        // flow analysis still verifies every writer arm populates `outgoing`
        // before the unified site reads it; a compile error (E0381) fires
        // if any arm is missed.
        let mut outgoing: Response;
        // True when `outgoing` came from a direct-head attempt: the wire head
        // is already serialized in `direct_head_buf` and `outgoing.headers`
        // is intentionally empty. Reset per request; synth paths leave false.
        let mut outgoing_direct = false;
        // 110.1: true when `outgoing` is a LOCALLY GENERATED reply rather than
        // a real upstream response. Every writer arm below is local EXCEPT the
        // proxy arm's completing upstream response, which clears it from
        // `completing_upstream_response`. Gates the gRPC local-reply transform
        // — a proxied response must NEVER be transformed (non-goal 4 /
        // CF-110-2). Defaults to `true` so a newly added synth arm is covered
        // by omission rather than silently skipped.
        let mut outgoing_local = true;

        // 8. Dispatch the request_path to the wire. 07.1 Task 6 wraps the
        // Task 5 writer-arm match inside `RequestPath::Match(outcome)`; the
        // new `SynthFromDecode(resp)` arm short-circuits when a decode-side
        // filter emitted `StopAndSend` (unreachable under the Router-only
        // 07.1 chain — Router never emits StopAndSend; the arm lands
        // structurally for 07.2 HeaderMutation forward-compat).
        match request_path {
            RequestPath::Match(outcome) => match outcome {
                BuildOutcome::Synth(resp, details) => {
                    outgoing = resp;
                    response_code_details_for_log = details.map(str::to_owned);
                }
                BuildOutcome::Proxy {
                    cluster: cluster_name,
                    retry_config,
                    include_attempt_count_in_response,
                    request_hash_key,
                    subset_match,
                } => {
                    // phase 43 (ADR-0100): the route resolved to a cluster —
                    // capture its name for %UPSTREAM_CLUSTER% at the ARM ENTRY,
                    // BEFORE the endpoint pick / attempt loop, so it renders
                    // even if the upstream attempt then fails (cluster_name is
                    // borrowed below, so clone).
                    // Only consumed under the access-log guard below — skip the
                    // per-request String clone when no sink is configured.
                    if log_enabled {
                        upstream_cluster_for_log = Some(cluster_name.clone());
                    }
                    // The validator (envoy-config Task 2) ensures every cluster
                    // name referenced from a RouteAction::Route exists in the
                    // bootstrap; the .expect() is defense-in-depth.
                    let cluster = config
                        .cluster_mgr
                        .get(&cluster_name)
                        .expect("validator ensures cluster present");

                    let host_header = find_header(&req.headers, headers::HOST)
                        .expect(
                            "build_response rejected missing/empty Host before BuildOutcome::Proxy",
                        )
                        .to_owned();

                    // 17 D5 (ADR-0046/ADR-0047): the request-budget gate
                    // (max_requests). Acquired ONCE per downstream request; the
                    // guard spans the ENTIRE retry loop (L9b) — bound here so it
                    // lives until the final response is built, then released on
                    // drop. Fires BEFORE the retry loop and BEFORE any pool/backend
                    // contact (L9a gate ordering: the request breaker fires before
                    // the retry breaker). On `Rejected` the request never reaches
                    // the retry loop: we build the overflow synth-503 and fall
                    // through to the unified writer site (where downstream_rq_5xx +
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
                        outgoing = synth_overflow(close);
                        // phase 50 (ADR-0107): the request-budget (max_requests)
                        // overflow is the SAME UO/overflow disposition as the pool
                        // arms — same synth_overflow helper, same 503 wire shape.
                        // Tag the rcd so the `build_access_log_record` derive maps it
                        // => "UO". This arm
                        // BYPASSES the retry loop, so it is tagged HERE (not via the
                        // retry loop's outcome:None discriminator).
                        // In-process-backstopped (M50-C: its
                        // differential witness is deferred — 0058 exercises only the
                        // pool PendingOverflow arm).
                        response_code_details_for_log =
                            Some("upstream_reset_before_response_started{overflow}".to_owned());
                        // L11: the overflow local reply carries
                        // x-envoy-attempt-count: 1 when the vhost flag is set (only
                        // the would-be first attempt; none ever dispatched).
                        if include_attempt_count_in_response {
                            outgoing.headers.push((
                                crate::router::X_ENVOY_ATTEMPT_COUNT.to_string(),
                                "1".to_string(),
                            ));
                        }
                        // Fall through to the unified writer site (no pool contact,
                        // no retry loop).
                    } else {
                        // `Unlimited` (no circuit_breakers — constraint vi,
                        // byte-identical to phase-16) → None; `Acquired` → hold the
                        // slot across the whole retry loop (L9b).
                        _request_guard = match request_acquire {
                            envoy_cluster::BudgetAcquisition::Acquired(g) => Some(g),
                            _ => None, // only Unlimited reaches here; Rejected is gated above
                        };

                        // 16 Task 4: H1 retry loop. With `retry_config: None`,
                        // `max_retries == 0` so the loop runs exactly once and the
                        // path is byte-identical to the pre-phase-16 single-attempt
                        // dispatch (no retry counters tick, no x-envoy-attempt-count).
                        let max_retries = retry_config.as_ref().map_or(0, |r| r.num_retries);
                        let mut attempts: u32 = 0;
                        // Whether the FINAL attempt (the one we broke out on) was
                        // itself retriable. Assigned on every `break` path; read
                        // post-loop to split retry_success vs limit_exceeded (L4).
                        #[allow(unused_assignments)]
                        let mut final_retriable = false;
                        // phase 52 (ADR-0109): the FINAL attempt's outcome. Captured each
                        // iteration because the loop `break` carries only
                        // (response, upstream_response), NOT attempt.outcome. Read post-loop to
                        // set connect_failure_for_log. AttemptOutcome is Copy (no move/borrow
                        // interaction with the per-iter `match attempt.outcome`).
                        #[allow(unused_assignments)]
                        let mut final_outcome: Option<AttemptOutcome> = None;
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

                        let attempt_direct =
                            direct_conn_eligible && !include_attempt_count_in_response;
                        let (final_response, completing_upstream_response, final_direct): (
                            Response,
                            bool,
                            bool,
                        ) = loop {
                            attempts += 1;

                            // Run one attempt: pick → acquire → send → receive. All
                            // counter side effects (except the per-call `cx_total`
                            // connect tick that lives inside) are driven HERE from
                            // the returned `AttemptResult`, so behavior is identical
                            // to the inlined version.
                            let attempt = run_attempt(
                                &config,
                                &cluster,
                                &cluster_name,
                                &req,
                                &host_header,
                                close,
                                request_hash_key,
                                subset_match.as_ref(),
                                if attempt_direct {
                                    Some(&mut direct_head_buf)
                                } else {
                                    None
                                },
                            )
                            .await;

                            if let Some(endpoint) = attempt.endpoint {
                                // 06.2 Task 6: capture the resolved upstream endpoint
                                // for the access-log `%UPSTREAM_HOST%` token (last
                                // attempt's endpoint wins). Skipped on pick()->None.
                                // Only consumed under the `!config.access_log.is_empty()`
                                // guard below (the `build_access_log_record` call at
                                // the dispatch site), so skip the
                                // per-request `SocketAddr` Display allocation entirely
                                // when no access-log sink is configured.
                                if !config.access_log.is_empty() {
                                    upstream_host_for_log = Some(endpoint.to_string());
                                }
                                // phase 50 (ADR-0107): discriminate the pool-overflow
                                // outcome (endpoint:Some + outcome:None — UNIQUELY the
                                // AcquireOutcome::Overflow result, hcm.rs:640; success
                                // :600 / reset :620 / connect-fail :629 all carry a
                                // non-None outcome) from a real upstream response. The
                                // overflow path is NOT a real upstream response →
                                // Envoy emits %RESPONSE_CODE_DETAILS% =
                                // "upstream_reset_before_response_started{overflow}"
                                // / %RESPONSE_FLAGS% = "UO" (state-0 recon); the
                                // derive in `build_access_log_record` maps the detail
                                // => "UO". Covers BOTH
                                // pool arms (max_connections :503/:508 +
                                // max_pending_requests :510/:515). All other
                                // endpoint:Some outcomes keep "via_upstream"
                                // (byte-identical to pre-phase-50).
                                response_code_details_for_log = Some(
                                    if attempt.outcome.is_none() {
                                        "upstream_reset_before_response_started{overflow}"
                                    } else {
                                        "via_upstream"
                                    }
                                    .to_owned(),
                                );
                            } else {
                                // phase 45 (ADR-0102): pick()->None is the no-healthy-upstream synth-503
                                // path (the ONLY `endpoint: None` AttemptResult, hcm.rs:438). Envoy emits
                                // %RESPONSE_CODE_DETAILS% = "no_healthy_upstream" here (state-1 recon).
                                response_code_details_for_log =
                                    Some("no_healthy_upstream".to_owned());
                            }

                            // L5: per-attempt upstream_rq_total — only for received
                            // upstream responses (single source of truth; the
                            // increment moved here from
                            // router::construct_proxied_response).
                            if attempt.upstream_response {
                                cluster.upstream_rq_total().inc();
                            }

                            // 14.2 D4 / lock-in #9 (L8): response-receipt hook PER
                            // ATTEMPT — each attempt feeds outlier detection. Records
                            // against the picked endpoint for Response, connect-fail,
                            // send-fail (Reset), and overflow paths (every path that
                            // reached a pick()). Skipped on pick()->None (no endpoint
                            // to attribute, lock-in #8). Inert without
                            // outlier_detection.
                            if let Some(endpoint) = attempt.endpoint {
                                cluster.record_response(endpoint, attempt.response.status);
                            }

                            // phase 52 (ADR-0109): capture the final attempt's outcome
                            // (read post-loop to set connect_failure_for_log).
                            final_outcome = attempt.outcome;
                            // Retry decision. `final_retriable` mirrors whether THIS
                            // (final-so-far) attempt is retriable — used post-loop to
                            // split retry_success vs limit_exceeded (L4).
                            final_retriable = match attempt.outcome {
                                Some(outcome) => retry_config.as_ref().is_some_and(|r| {
                                    r.is_retriable(attempt.response.status, outcome)
                                }),
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
                                        if let Some(d) = RetryConfig::backoff(attempts) {
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
                                        if let Some(d) = RetryConfig::backoff(attempts) {
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
                            break (
                                attempt.response,
                                attempt.upstream_response,
                                attempt.direct_head,
                            );
                        };

                        // Post-loop reconciliation.
                        // L5: upstream_rq_5xx reflects the COMPLETING REAL upstream
                        // response only (retried-away 5xx attempts do NOT tick it).
                        // Gated on the completing attempt having received a real
                        // upstream response — synth local replies (the no-healthy-
                        // upstream synth-503, connect-failure synth-503, reset synth-
                        // 503, and overflow synth-503 paths) do NOT tick it, preserving
                        // the pre-phase-16 baseline (they never did). Single source of
                        // truth (moved here from router::construct_proxied_response).
                        if completing_upstream_response && final_response.status / 100 == 5 {
                            cluster.upstream_rq_5xx().inc();
                        }
                        // L4: retry outcome counters. Only when at least one retry
                        // fired (attempts > 1). If the final attempt was still
                        // retriable (we ran out of budget) → limit_exceeded (L9 final
                        // response is the last upstream verbatim); else → success.
                        // 17 D4 (L7 exclusivity): a budget-blocked exit bypasses this
                        // split entirely — the overflow counter already accounted for
                        // it, and the blocked retry never fired (so attempts==1 here in
                        // the L1 case; the guard is belt-and-braces for a >0-cap
                        // exhaustion after one or more retries already fired).
                        if attempts > 1 && !retry_budget_blocked {
                            if final_retriable {
                                cluster.upstream_rq_retry_limit_exceeded().inc();
                                // phase 51 (ADR-0108): the L9 retry-limit-exceeded
                                // exit — num_retries consumed with the final attempt
                                // still retriable → the last upstream response is
                                // surfaced downstream verbatim. Envoy renders
                                // %RESPONSE_FLAGS% = "URX" here (access-log-only,
                                // never a response header). Set the discriminator
                                // co-located with the counter (one shared gate) so
                                // the %RESPONSE_FLAGS% derive renders "URX". The rcd
                                // stays "via_upstream" (a real upstream 503 —
                                // UNCHANGED). EXCLUDED: the retry-BUDGET-blocked exit
                                // (gated out by !retry_budget_blocked) and the
                                // pre-loop request-budget overflow (bypasses the loop
                                // → renders "UO").
                                retry_limit_exceeded_for_log = true;
                            } else {
                                cluster.upstream_rq_retry_success().inc();
                            }
                        }
                        // phase 52 (ADR-0109): flag UF when the FINAL attempt was a
                        // connect failure — independent of the retry split (a single
                        // connect-failure attempt with no retry_policy flags it too).
                        // A connect-failure retried to success has final_outcome =
                        // Some(Response) → not flagged. If BOTH this and
                        // retry_limit_exceeded_for_log are set (a retry-exhausted-
                        // connect-failure — un-recon'd combination, §4), the derive's
                        // URX-before-UF ordering renders URX deterministically.
                        connect_failure_for_log =
                            matches!(final_outcome, Some(AttemptOutcome::ConnectFailure));
                        // phase 54 (ADR-0111): set the deterministic upstream-reset rcd on
                        // the pure-reset final-outcome path. Envoy renders
                        // %RESPONSE_CODE_DETAILS% =
                        // "upstream_reset_before_response_started{connection_termination}"
                        // here — a FIXED reset-reason enum (deterministic, UNLIKE the
                        // connect-failure rcd's OS-derived brace). This OVERRIDES the shared
                        // "via_upstream" the in-loop result-consumption arm wrote for the
                        // reset path, and the %RESPONSE_FLAGS% derive in
                        // `build_access_log_record` maps it
                        // => "UC" (the phase-50 {overflow} => "UO" precedent). A reset
                        // retried to success has final_outcome = Some(Response) → not set
                        // (replay-safe, ADR-0044). Guarded `!retry_limit_exceeded_for_log`
                        // so the retry-exhausted-reset case (M53-3) keeps rcd =
                        // "via_upstream" and renders %RESPONSE_FLAGS% = "URX" (the derive's
                        // URX branch is checked first).
                        if matches!(final_outcome, Some(AttemptOutcome::Reset))
                            && !retry_limit_exceeded_for_log
                        {
                            response_code_details_for_log = Some(
                                "upstream_reset_before_response_started{connection_termination}"
                                    .to_owned(),
                            );
                        }
                        // Release the retry-budget slot now, before building the outgoing response,
                        // so the slot (and its gauges) reflect completion rather than lingering
                        // until this stack frame unwinds.
                        drop(retry_guard_slot);

                        outgoing = final_response;
                        outgoing_direct = final_direct;
                        // 110.1: `upstream_response` is the tree's existing
                        // "a real upstream RESPONSE was received" bit
                        // (`AttemptResult` doc). Its complement is exactly
                        // "this is a local reply": `synth_no_healthy_upstream`,
                        // `synth_status(503)` and `synth_overflow` from
                        // `run_attempt` all leave it false.
                        outgoing_local = !completing_upstream_response;

                        // L6: x-envoy-attempt-count on the downstream response, ONLY
                        // when the vhost flag is set. Emitted on ALL outcomes that
                        // reached the proxy arm (proxied responses and synths), value
                        // = total attempts. (Envoy emits attempt-count on local replies
                        // generated by the router too when the flag is on.)
                        if include_attempt_count_in_response {
                            outgoing.headers.push((
                                crate::router::X_ENVOY_ATTEMPT_COUNT.to_string(),
                                attempts.to_string(),
                            ));
                        }
                    } // close the `else` (request-budget Acquired/Unlimited path)
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
                decorate_filter_synth_response(&mut outgoing, Some(connection_value(close)));
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
                outgoing_direct = false;
                // 110.1: a filter's substitute response IS a local reply, even
                // when it replaced a proxied one. MEASURED upstream: an RBAC
                // deny with a gRPC content-type returns 200 + `grpc-status: 7`
                // + `grpc-message: RBAC: access denied`.
                outgoing_local = true;
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
                decorate_filter_synth_response(&mut outgoing, Some(connection_value(close)));
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
        // 110.1: the gRPC local-reply transform, at the FIRST of the two H1
        // wire funnels (the io_uring worker has its own — `uring.rs`'s
        // `write_owned`).
        //
        // PLACEMENT IS LOAD-BEARING. This must run BEFORE
        // `response_status_for_log` / `response_body_len` are derived below,
        // because those two drive the access-log record AND the per-class
        // counter dispatch. MEASURED upstream: a transformed local reply logs
        // `%RESPONSE_CODE%` = 200 and `%BYTES_SENT%` = 0, and ticks
        // `downstream_rq_2xx` — NOT the original status's class.
        // `%RESPONSE_CODE_DETAILS%` is unchanged by the transform.
        //
        // NOT installed in `synth_with` / any `synth_*` / `build_response`:
        // `envoy-http2` calls `envoy_http1::build_response`, so a transform
        // there would rewrite H2 route-decision replies while missing H2's own
        // `synth_h2_*` family (CF-110-1; the ADR-0049 class).
        if outgoing_local {
            // A local reply never takes the zero-copy direct-head path:
            // `direct_head: true` is set at exactly one site, the successful
            // proxied attempt, which also sets `upstream_response: true`.
            debug_assert!(
                !outgoing_direct,
                "a local reply must never carry a pre-serialized direct head"
            );
            crate::grpc::apply_grpc_local_reply(&mut outgoing, &req.headers);
        }

        let response_status_for_log: u16 = outgoing.status;
        let response_body_len: u64 = outgoing.body.len() as u64;
        // `outgoing` stays owned and alive through the access-log block below, so
        // the single consumer (`extract_upstream_service_time` at the record build)
        // borrows `outgoing.headers` directly rather than paying a per-request clone
        // of the whole response-header vec (it was only ever read once, logging-on).

        // 07.1 Task 5: unified wire-write site. 07.1 Task 6 inserted
        // `pipeline.encode_headers` above (boundary conversion + write-back
        // / replacement) so the wire-write below sees the post-encode value.
        if outgoing_direct {
            // Fast path: the transformed head is already serialized in
            // `direct_head_buf`; emit head + body with the same
            // threshold/vectored strategy as `write_to_buf`.
            crate::response::write_head_and_body(
                &mut downstream,
                &mut direct_head_buf,
                &outgoing.body,
            )
            .await?;
        } else {
            Http1Response::write_to_buf(&outgoing, &mut downstream, &mut write_buf).await?;
        }

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
        if let (Some(req_arrival_instant), Some(req_arrival_systime)) =
            (req_arrival_instant, req_arrival_systime)
        {
            let duration = req_arrival_instant.elapsed();
            let record = build_access_log_record(
                AccessLogRequestInfo {
                    req: &req,
                    start_time: req_arrival_systime,
                    bytes_received: request_body_len,
                    matched_route: matched_route.as_ref(),
                    dynamic_metadata: &dynamic_metadata,
                },
                AccessLogResponseInfo {
                    status: response_status_for_log,
                    bytes_sent: response_body_len,
                    duration,
                    headers: &outgoing.headers,
                    upstream_host: upstream_host_for_log,
                    upstream_cluster: upstream_cluster_for_log,
                    response_code_details: response_code_details_for_log,
                    retry_limit_exceeded: retry_limit_exceeded_for_log,
                    connect_failure: connect_failure_for_log,
                },
            );
            for sink in &config.access_log {
                // Phase 70 Task 7: gate emission on the sink's compiled filter.
                // A sink with no filter accepts every record, so the resulting
                // access_logs_total is identical to the pre-70 bulk add(len).
                // Phase 72: the `header_filter` arm reads the downstream request
                // headers in scope here (`req.headers`, the same snapshot that
                // feeds forwarded_for/authority above). The other arms ignore it.
                // Phase 74: thread the record's dynamic-metadata store for the
                // `metadata_filter` arm (already built above — the record is
                // constructed BEFORE this loop). The other arms ignore it.
                if !sink.should_log(
                    record.response_code,
                    &record.response_flags,
                    &req.headers,
                    &record.dynamic_metadata,
                ) {
                    continue;
                }
                // 06.3 D15.3.e: increment access_logs_total at queue-enter time
                // (BEFORE the per-sink await), per parent SPEC §6 Rule 4 —
                // fire-and-forget emission's failures do NOT deflate the count.
                // Phase 70 moved this INSIDE the loop (was one bulk
                // `add(access_log.len())` per 06.1 REVIEW §7 R-8): the count is
                // now per intent-to-emit, so a filter-suppressed sink does not
                // over-count.
                config.stats.access_logs_total.inc();
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

/// Request-side inputs to [`build_access_log_record`], grouped so the record
/// build does not need a many-argument signature. Borrows of (or copies from)
/// `serve_connection`'s per-request locals.
struct AccessLogRequestInfo<'a> {
    /// The (post-decode-filter) request: supplies method / path and the
    /// request-header-backed operators (%REQ(...)%).
    req: &'a Request,
    /// Wall-clock sample at request arrival (`%START_TIME%`).
    start_time: std::time::SystemTime,
    /// Request-side wire-byte count (`%BYTES_RECEIVED%` — body only).
    bytes_received: u64,
    /// The up-front resolved route (`%ROUTE_NAME%`); `matched_route` (bound
    /// at resolve_route in `serve_connection`) is still live at the dispatch
    /// site.
    matched_route: Option<&'a ResolvedRoute>,
    /// Pipeline dynamic metadata captured by `run_decode_filters`
    /// (`%DYNAMIC_METADATA%`).
    dynamic_metadata:
        &'a std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

/// Response-side inputs to [`build_access_log_record`]: the values derived
/// from the wire response plus the per-request `*_for_log` locals populated
/// by the writer arms. The `Option<String>` fields are moved in (each is
/// consumed exactly once by the record — no extra clones).
struct AccessLogResponseInfo<'a> {
    /// `outgoing.status` (`%RESPONSE_CODE%`).
    status: u16,
    /// `outgoing.body.len()` (`%BYTES_SENT%`).
    bytes_sent: u64,
    /// Request-arrival → dispatch-site elapsed time (`%DURATION%`).
    duration: std::time::Duration,
    /// `outgoing.headers`, borrowed for the upstream-service-time extract.
    headers: &'a [(String, String)],
    /// `upstream_host_for_log` (`%UPSTREAM_HOST%`).
    upstream_host: Option<String>,
    /// `upstream_cluster_for_log` (`%UPSTREAM_CLUSTER%`).
    upstream_cluster: Option<String>,
    /// `response_code_details_for_log` (`%RESPONSE_CODE_DETAILS%`, also keys
    /// part of the `%RESPONSE_FLAGS%` derive).
    response_code_details: Option<String>,
    /// `retry_limit_exceeded_for_log` (phase 51, `%RESPONSE_FLAGS%` = "URX").
    retry_limit_exceeded: bool,
    /// `connect_failure_for_log` (phase 52, `%RESPONSE_FLAGS%` = "UF").
    connect_failure: bool,
}

/// Build the per-request access-log record (extracted verbatim from
/// `serve_connection`'s factored access-log dispatch site, including the
/// `%RESPONSE_FLAGS%` derive block).
fn build_access_log_record(
    request: AccessLogRequestInfo<'_>,
    response: AccessLogResponseInfo<'_>,
) -> envoy_accesslog::AccessLogRecord {
    envoy_accesslog::AccessLogRecord {
        start_time: request.start_time,
        method: request.req.method.clone(),
        path: x_envoy_original_path_or_path(request.req).to_owned(),
        protocol: "HTTP/1.1".to_owned(),
        response_code: response.status,
        // phase 48 (ADR-0105) / 49 (ADR-0106) / 50 (ADR-0107) / 51
        // (ADR-0108): %RESPONSE_FLAGS%. Phase 51 prepends a boolean branch
        // for "URX" (UpstreamRetryLimitExceeded) — the FIRST flag NOT
        // derivable from %RESPONSE_CODE_DETAILS% (the retry-limit-exceeded
        // path's rcd is the shared "via_upstream"); it keys on the
        // `retry_limit_exceeded_for_log` boolean set at the retry-loop
        // limit-exceeded exit (the same gate as
        // upstream_rq_retry_limit_exceeded). The else-branch is the
        // unchanged phase-48/49/50 rcd-match:
        //   route_not_found     → NR (NoRoute)          — the two no-route
        //                          synth_404 arms (host-miss + route-miss in
        //                          `build_response_in`).
        //   no_healthy_upstream → UH (NoHealthyUpstream) — the single
        //                          pick()->None no-healthy synth-503 arm
        //                          (the `endpoint: None` arm in
        //                          `serve_connection`'s retry loop).
        //   upstream_reset_before_response_started{overflow}
        //                       → UO (UpstreamOverflow) — the overflow
        //                          synth-503: both pool arms (the
        //                          outcome:None discriminator in
        //                          `serve_connection`'s retry loop) and
        //                          the request-budget arm (the
        //                          `BudgetAcquisition::Rejected` branch).
        //   upstream_reset_before_response_started{connection_termination}
        //                       → UC (UpstreamConnectionTermination) — the
        //                          pure-reset synth-503 (§A, phase 54).
        // The boolean is set ONLY on the L9 path (rcd = via_upstream → the
        // else-match's `_ => "-"` arm), so the NR/UH/UO arms are unreachable
        // with it set → byte-identical to phase 50. Read by-ref here;
        // `response_code_details_for_log` is moved into the
        // `response_code_details:` field below (bool is Copy — no interaction).
        // phase 52 (ADR-0109): the `connect_failure_for_log => "UF"`
        // (UpstreamConnectionFailure) branch — the SECOND flag NOT
        // derivable from %RESPONSE_CODE_DETAILS% (the connect-failure rcd
        // is the shared "via_upstream", which would otherwise fall to the
        // else-match's `_ => "-"` arm); it keys on the
        // `connect_failure_for_log` boolean set post-loop when the FINAL
        // attempt's AttemptOutcome is ConnectFailure. Ordered after URX
        // (the un-recon'd retry-exhausted-connect-failure combination, if
        // it ever sets both, renders URX deterministically — §4).
        // phase 54 (ADR-0111): "UC" (UpstreamConnectionTermination) is now
        // derived 1:1 from %RESPONSE_CODE_DETAILS% =
        // "upstream_reset_before_response_started{connection_termination}"
        // (the rcd-match arm below — the phase-50 {overflow} => "UO"
        // precedent), set by §A on the pure-reset final-outcome path. The
        // phase-53 reset-discriminator boolean was retired (the reset rcd
        // is no longer the shared "via_upstream"). UNLIKE URX/UF, whose
        // rcds genuinely STAY "via_upstream" (so they remain
        // boolean-derived).
        response_flags: if response.retry_limit_exceeded {
            "URX"
        } else if response.connect_failure {
            "UF"
        } else {
            match response.response_code_details.as_deref() {
                Some("route_not_found") => "NR",
                Some("no_healthy_upstream") => "UH",
                Some("upstream_reset_before_response_started{overflow}") => "UO",
                Some("upstream_reset_before_response_started{connection_termination}") => "UC",
                _ => "-",
            }
        }
        .to_owned(),
        bytes_received: request.bytes_received,
        bytes_sent: response.bytes_sent,
        duration: response.duration,
        upstream_service_time: extract_upstream_service_time(response.headers),
        forwarded_for: access_log_header_value(&request.req.headers, "x-forwarded-for"),
        user_agent: access_log_header_value(&request.req.headers, "user-agent"),
        request_id: access_log_header_value(&request.req.headers, "x-request-id"),
        authority: access_log_header_value(&request.req.headers, "host"),
        upstream_host: response.upstream_host,
        // phase 43 (ADR-0100): %UPSTREAM_CLUSTER% — set at the proxy
        // arm entry (Some on a routed request, None for direct_response
        // / synth / error paths).
        upstream_cluster: response.upstream_cluster,
        // phase 41: the matched route's config `name` (empty = unnamed
        // → None), rendered by %ROUTE_NAME%. `matched_route` (bound at
        // resolve_route in `serve_connection`) is still live here.
        route_name: request
            .matched_route
            .map(|r| r.route().name.as_str())
            .filter(|n| !n.is_empty())
            .map(str::to_owned),
        // phase 42 (ADR-0099): %RESPONSE_CODE_DETAILS% backing field,
        // set per response-path (direct_response / via_upstream).
        response_code_details: response.response_code_details,
        dynamic_metadata: request.dynamic_metadata.clone(),
    }
}

/// Resolve the request body length from `Content-Length`, rejecting the
/// request-smuggling shapes RFC 7230 §3.3.3 forbids.
///
/// Previously this read only the FIRST `Content-Length` (via `find_header`), so a
/// request carrying two `Content-Length` rows with different values
/// (`Content-Length: 5` / `Content-Length: 6`) framed on the first and left the
/// second body as a pipelined "next request" — the classic CL/CL smuggling
/// vector. Now every `Content-Length` row is scanned: conflicting values are
/// rejected as `MalformedHeader` (the same disposition as a non-numeric value, so
/// no new response shape is introduced), matching upstream Envoy's rejection.
/// Identical repeated values are tolerated (§3.3.3 permits combining them).
/// True if any `Transfer-Encoding` header names `chunked` as one of its
/// comma-separated tokens (case-insensitive, OWS-trimmed). envoy-rust does not
/// support chunked request bodies and rejects them 501. Matching only the exact
/// value `"chunked"` (the former inline check) let smuggling-shaped values like
/// `"chunked, gzip"` or `"gzip, chunked"` — and a `chunked` token split across
/// two `Transfer-Encoding` rows — slip through to Content-Length framing, a
/// TE/CL desync vector. Detecting the token in any position rejects them all,
/// consistent with the codec's "chunked not supported" stance.
fn has_chunked_transfer_encoding(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
    })
}

pub(crate) fn parse_content_length(headers: &[(String, String)]) -> Result<usize, Http1Error> {
    let mut seen: Option<usize> = None;
    for (name, value) in headers {
        if !name.eq_ignore_ascii_case(headers::CONTENT_LENGTH) {
            continue;
        }
        let parsed = value
            .parse::<usize>()
            .map_err(|_| Http1Error::MalformedHeader)?;
        match seen {
            Some(prev) if prev != parsed => return Err(Http1Error::MalformedHeader),
            _ => seen = Some(parsed),
        }
    }
    Ok(seen.unwrap_or(0))
}

/// Phase 70/71/72/73/74 — translate a config-side `AccessLogFilter` into the
/// runtime `LogFilter` predicate the sink evaluates per record. The envoy-config
/// validator (`validate_access_logs`) already enforced that exactly one oneof
/// arm is set, so the 0/multi-arm cases are `unreachable!` (CF-70-1: the
/// zero-arm `expect()` is gone). SIX arms ship: `status_code_filter` (phase
/// 70), `response_flag_filter` (phase 71), `header_filter` (phase 72), the
/// recursive `and_filter`/`or_filter` composition arms (phase 73), which map
/// each nested child via `.iter().map(compile_access_log_filter)`, and
/// `metadata_filter` (phase 74).
fn compile_access_log_filter(f: &envoy_config::AccessLogFilter) -> envoy_accesslog::LogFilter {
    match (
        &f.status_code_filter,
        &f.response_flag_filter,
        &f.header_filter,
        &f.and_filter,
        &f.or_filter,
        &f.metadata_filter,
    ) {
        (Some(scf), None, None, None, None, None) => {
            let op = match scf.comparison.op {
                envoy_config::ComparisonOp::Eq => envoy_accesslog::FilterOp::Eq,
                envoy_config::ComparisonOp::Ge => envoy_accesslog::FilterOp::Ge,
                envoy_config::ComparisonOp::Le => envoy_accesslog::FilterOp::Le,
            };
            envoy_accesslog::LogFilter::StatusCode(envoy_accesslog::StatusCodeComparison {
                op,
                // `runtime_key` is RTDS-inert here — the comparison always uses
                // `default_value` (see `RuntimeUInt32`'s envoy-config doc comment).
                threshold: scf.comparison.value.default_value,
            })
        }
        (None, Some(rff), None, None, None, None) => envoy_accesslog::LogFilter::ResponseFlag {
            flags: rff.flags.clone(),
        },
        // Phase 72 (ADR-0150): box the config `HeaderMatcher` into the injected
        // `HeaderMatch` seam. The validator already compiled its SafeRegex, so
        // the runtime `matches` never hits its `.expect()`.
        (None, None, Some(hf), None, None, None) => envoy_accesslog::LogFilter::Header {
            matcher: std::sync::Arc::new(hf.header.clone()),
        },
        // Phase 73: the two composition arms map each child recursively.
        (None, None, None, Some(af), None, None) => envoy_accesslog::LogFilter::And(
            af.filters.iter().map(compile_access_log_filter).collect(),
        ),
        (None, None, None, None, Some(of), None) => envoy_accesslog::LogFilter::Or(
            of.filters.iter().map(compile_access_log_filter).collect(),
        ),
        // Phase 74 (ADR-0150/ADR-0155): box the config `MetadataMatcher` into
        // the injected `MetadataMatch` seam (the validator already compiled its
        // SafeRegex, so the runtime `matches` never hits its `.expect()`), and
        // resolve the `google.protobuf.BoolValue` wrapper default — absent means
        // `true` (MEASURED, SPEC §0 R-0.4; `--mode validate` provably cannot
        // reach this). A matcher-less `metadata_filter` (accepted upstream,
        // R-0.2) compiles to `matcher: None`, so every record takes the
        // not-found policy.
        (None, None, None, None, None, Some(mf)) => envoy_accesslog::LogFilter::Metadata {
            matcher: mf.matcher.as_ref().map(|m| {
                std::sync::Arc::new(m.clone()) as std::sync::Arc<dyn envoy_accesslog::MetadataMatch>
            }),
            match_if_key_not_found: mf.match_if_key_not_found.unwrap_or(true),
        },
        _ => unreachable!("validated by validate_access_logs: exactly one filter arm is set"),
    }
}

/// Phase 32 Task 5 (ADR-0079) — build the access-log `CompiledFormat` for a
/// file sink: the config-supplied
/// `log_format.text_format_source.inline_string` if present, else the Envoy
/// default format. The config validator (`envoy-config` Task 4) already compiled
/// the string at config-load, so `from_inline` here re-parses an already-validated
/// string and cannot fail in practice — but we map any error defensively rather
/// than panic (the HCM build is `Result`-returning).
fn compiled_log_format(
    file_cfg: &envoy_config::FileAccessLog,
) -> Result<envoy_accesslog::LogFormat, Http1Error> {
    let map_err = |err: envoy_accesslog::FormatParseError| Http1Error::AccessLogFormat {
        message: err.to_string(),
    };
    match &file_cfg.log_format {
        // exactly-one-of already enforced by the envoy-config validator (Task 2);
        // this build is defense-in-depth — prefer the set arm, default if neither.
        Some(s) => match (&s.text_format_source, &s.json_format) {
            (Some(ds), _) => Ok(
                // ADR-0096 §B — thread `omit_empty_values` onto the compiled format.
                envoy_accesslog::CompiledFormat::from_inline(&ds.inline_string)
                    .map_err(map_err)?
                    .with_omit_empty(s.omit_empty_values)
                    .into(),
            ),
            (None, Some(map)) => {
                // Bridge: map the config-side recursive `JsonFormatValue` into the
                // accesslog-side `JsonValueInput` mirror (the crate dependency runs
                // envoy-config → envoy-accesslog; the reverse would be a cycle, so
                // the caller owns this mapping — ADR-0094 / phase 39 T5).
                let input: std::collections::BTreeMap<String, envoy_accesslog::JsonValueInput> =
                    map.iter()
                        .map(|(k, v)| (k.clone(), json_format_value_to_input(v)))
                        .collect();
                Ok(envoy_accesslog::CompiledJsonFormat::from_map(&input)
                    .map_err(map_err)?
                    .with_omit_empty(s.omit_empty_values) // ADR-0096 §B/§D
                    .into())
            }
            (None, None) => Ok(envoy_accesslog::CompiledFormat::default().into()),
        },
        None => Ok(envoy_accesslog::CompiledFormat::default().into()),
    }
}

/// Phase 39 T5 (ADR-0094) — map the config-side recursive `JsonFormatValue` into
/// the accesslog-side `JsonValueInput` mirror. This lives on the caller (HCM)
/// side because the crate dependency direction is `envoy-config` →
/// `envoy-accesslog` (a reverse edge would be a cycle); `envoy-accesslog` cannot
/// see `JsonFormatValue`. The mapping is a structural 1:1 (no behavior).
fn json_format_value_to_input(
    value: &envoy_config::JsonFormatValue,
) -> envoy_accesslog::JsonValueInput {
    use envoy_accesslog::JsonValueInput;
    use envoy_config::JsonFormatValue;
    match value {
        JsonFormatValue::Null => JsonValueInput::Null,
        JsonFormatValue::Bool(b) => JsonValueInput::Bool(*b),
        JsonFormatValue::Format(s) => JsonValueInput::Format(s.clone()),
        JsonFormatValue::Array(items) => {
            JsonValueInput::Array(items.iter().map(json_format_value_to_input).collect())
        }
        JsonFormatValue::Object(map) => JsonValueInput::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), json_format_value_to_input(v)))
                .collect(),
        ),
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
    /// phase 42 (ADR-0099): the second field carries the
    /// `%RESPONSE_CODE_DETAILS%` access-log detail string for this synth path
    /// (`Some("direct_response")` for a DirectResponse route; `None` for error
    /// synths 400/404/501 — deferred).
    Synth(Response, Option<&'static str>),
    Proxy {
        cluster: String,
        /// 16 Task 4: resolved per-route retry policy (`None` → no retries).
        /// Built from the matched route's `RouteAction_Route.retry_policy` via
        /// `RetryConfig::from`.
        retry_config: Option<RetryConfig>,
        /// 16 Task 4 (L6): the matched virtual-host's
        /// `include_attempt_count_in_response` flag. Gates emission of the
        /// `x-envoy-attempt-count` downstream response header.
        include_attempt_count_in_response: bool,
        /// 28 Task 6 (ADR-0070): the per-request hash key resolved from the
        /// matched route's `hash_policy` against the request headers (via
        /// [`request_hash_key`]). `Some(xxh64(value))` when the route has a
        /// header `hash_policy` and that header is PRESENT (empty value →
        /// `Some(xxh64(b""))`, NOT a fallback); `None` when there is no
        /// `hash_policy` or the header is ABSENT. Threaded to
        /// `ClusterHandle::pick_endpoint`; `RoundRobin` clusters ignore it.
        request_hash_key: Option<u64>,
        /// 30 Task 6: the matched route's `metadata_match` envoy.lb map (subset LB).
        /// `None` when the route has no `metadata_match` (the no-subset no-op). Static
        /// route config — resolved at route-match, threaded to `pick_endpoint`.
        subset_match: Option<std::collections::BTreeMap<String, String>>,
    },
}

/// Per-request dispatch path (07.1 Task 6).
///
/// `Match` — the request passed through `pipeline.decode_headers` and
/// hit the writer-arm match via `build_response`.
///
/// `SynthFromDecode` — a decode-side filter short-circuited the request
/// with `StopAndSend`; the response goes directly to the unified
/// factored site without consulting the writer arms or `build_response`.
enum RequestPath {
    Match(BuildOutcome),
    SynthFromDecode(Response),
}

/// 26 Task 2: a matched route that OWNS its route-table snapshot. With the
/// route table now behind a swappable handle ([`HCMConfig::current_route_config`]),
/// `resolve_route` can no longer return a `&Route` borrowed from the config —
/// the snapshot is a temporary. So it returns this value, which keeps the
/// snapshot `Arc<RouteConfiguration>` alive and exposes the matched route by
/// stored vhost/route indices via [`ResolvedRoute::route`]. Holding it pins the
/// §5.4 snapshot for the request's lifetime regardless of a concurrent `store`.
pub struct ResolvedRoute {
    route_config: Arc<RouteConfiguration>,
    vh_idx: usize,
    route_idx: usize,
}

impl ResolvedRoute {
    /// Borrow the matched route out of the held snapshot. The indices were
    /// computed against this exact `route_config` snapshot in `resolve_route`,
    /// so they are always in-bounds.
    pub fn route(&self) -> &Route {
        &self.route_config.virtual_hosts[self.vh_idx].routes[self.route_idx]
    }
}

/// Phase-23 D2: resolve the matched route up-front (vh-match + route-match),
/// for threading per-route filter config into the pipeline BEFORE the decode
/// pass. Returns `None` for missing/empty Host, no matching vh, or no matching
/// route — the no-route paths carry no per-route config (a 404'd request has no
/// CORS policy). Shares `vh_matches`/`route_matches` with `build_response`, so
/// the up-front resolution and `build_response`'s internal re-match are
/// guaranteed identical (the 30-fixture regression-equivalence guarantee).
///
/// 26 Task 2: snapshots the swappable route table once (§5.4 read-once) and
/// returns a [`ResolvedRoute`] that owns that snapshot, so the borrowed route
/// stays valid even if the RDS watcher swaps the table mid-request.
pub fn resolve_route(config: &HCMConfig, req: &Request) -> Option<ResolvedRoute> {
    // Public entry: take the §5.4 read-once snapshot here, then delegate. The
    // H1 keep-alive loop instead snapshots ONCE per request and threads that
    // single snapshot into both `resolve_route_in` and `build_response_in`
    // (see the handler) — one lock read per request, and resolve/build share
    // the exact same table even if the RDS watcher swaps concurrently.
    resolve_route_in(&config.current_route_config(), req, &config.runtime)
}

/// Snapshot-threaded core of [`resolve_route`]: the caller owns the §5.4
/// read-once `Arc<RouteConfiguration>` snapshot and passes it in, so a single
/// snapshot can back both the up-front resolution and the later
/// [`build_response_in`] without a second `RwLock` read (and without the two
/// views ever splitting across a concurrent `store_route_config`).
pub(crate) fn resolve_route_in(
    route_config: &Arc<RouteConfiguration>,
    req: &Request,
    runtime: &envoy_config::runtime::RuntimeSnapshot,
) -> Option<ResolvedRoute> {
    let host_raw = find_header(&req.headers, headers::HOST).filter(|h| !h.is_empty())?;
    let host = strip_port(host_raw);
    let vh_idx = route_config
        .virtual_hosts
        .iter()
        .position(|vh| vh_matches(vh, host))?;
    let route_idx = route_config.virtual_hosts[vh_idx]
        .routes
        .iter()
        .position(|r| route_matches(r, &req.path, &req.headers, runtime))?;
    // Cheap refcount bump: the returned `ResolvedRoute` keeps the snapshot
    // alive for the caller's borrow, identical to the pre-snapshot-sharing
    // behaviour.
    Some(ResolvedRoute {
        route_config: route_config.clone(),
        vh_idx,
        route_idx,
    })
}

pub fn build_response(config: &HCMConfig, req: &mut Request, close: bool) -> BuildOutcome {
    // Public entry: take the §5.4 read-once snapshot here, then delegate. The
    // H1 handler instead shares one snapshot with `resolve_route_in` (see the
    // handler comment) — behaviour-identical for a stable table, and strictly
    // MORE consistent under a concurrent RDS swap (resolve + build can no
    // longer land on two different tables).
    build_response_in(&config.current_route_config(), req, close, &config.runtime)
}

/// Snapshot-threaded core of [`build_response`]: routes against the caller's
/// §5.4 read-once `Arc<RouteConfiguration>` snapshot instead of re-reading the
/// `RwLock`. See [`resolve_route_in`] for why the H1 loop shares one snapshot.
pub(crate) fn build_response_in(
    route_config: &Arc<RouteConfiguration>,
    req: &mut Request,
    close: bool,
    runtime: &envoy_config::runtime::RuntimeSnapshot,
) -> BuildOutcome {
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
            return BuildOutcome::Synth(synth_400(close), None);
        }
    };
    let host = strip_port(host_raw);

    // Walk virtual_hosts first-match-wins on Host.
    let vh = match route_config
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
            // phase 47 (ADR-0104): %RESPONSE_CODE_DETAILS% = route_not_found on the host-miss 404 path
            return BuildOutcome::Synth(synth_404(close), Some("route_not_found"));
        }
    };

    // Walk routes first-match-wins on path + headers.
    let route = match vh
        .routes
        .iter()
        .find(|r| route_matches(r, &req.path, &req.headers, runtime))
    {
        Some(r) => r,
        None => {
            tracing::warn!(
                host = %host,
                method = %req.method,
                path = %req.path,
                "request rejected: no matching route"
            );
            // phase 46 (ADR-0103): %RESPONSE_CODE_DETAILS% = route_not_found on the route-miss 404 path
            return BuildOutcome::Synth(synth_404(close), Some("route_not_found"));
        }
    };

    // Hardcoded router-filter call site.
    match &route.action {
        RouteAction::DirectResponse(dr) => {
            BuildOutcome::Synth(synth_direct_response(dr, close), Some("direct_response"))
        }
        // 76.2: the REAL redirect arm, replacing 76.1's honest `synth_501`
        // placeholder (ADR-0169 DECISION 4). ONE arm serves BOTH codecs — H2 has
        // no route-action dispatch of its own and calls this function.
        RouteAction::Redirect(rd) => {
            // The authority comes from the `Host:` header VERBATIM (port
            // included), NOT from the socket. Re-read it as an OWNED string so
            // the immutable borrow of `req.headers` ends before the `req.path`
            // write-back below.
            let authority = find_header(&req.headers, headers::HOST)
                .unwrap_or_default()
                .to_string();
            let plan = plan_redirect(&authority, &req.path, route.r#match.prefix.as_deref(), rd);
            // MEASURED: `prefix_rewrite` MUTATES the logged `:path` while
            // `path_redirect` does NOT. `build_access_log_record` reads
            // `req.path` AFTER this function returns, so the rewrite must land
            // in the request itself.
            if let Some(new_path) = plan.rewritten_path {
                req.path = new_path;
            }
            // MEASURED: `%RESPONSE_CODE_DETAILS%` for a redirect is
            // `direct_response` — the SAME bare literal the arm above emits. No
            // new detail string, `Op` or `AccessLogRecord` field is needed.
            BuildOutcome::Synth(
                synth_redirect(plan.status, plan.location, close),
                Some("direct_response"),
            )
        }
        RouteAction::Route(ar) => BuildOutcome::Proxy {
            cluster: ar.cluster.clone(),
            // 16 Task 4: resolve the per-route retry policy. `None` → no retries.
            retry_config: ar.retry_policy.as_ref().map(RetryConfig::from),
            // 16 Task 4 (L6): the matched vhost's attempt-count flag travels to
            // the dispatch seam so the retry loop can gate x-envoy-attempt-count.
            include_attempt_count_in_response: vh.include_attempt_count_in_response,
            // 28 Task 6 (ADR-0070): resolve the per-request hash key from the
            // matched route's `hash_policy` against the request headers HERE
            // (before the pick). `find_header` returns the value only when the
            // header is PRESENT — `map` (not filter) preserves present-empty as
            // `Some(xxh64(b""))`. Empty `hash_policy` → `None` (the common,
            // allocation-free non-RING_HASH path).
            request_hash_key: request_hash_key(&ar.hash_policy, |name| {
                find_header(&req.headers, name).map(str::as_bytes)
            }),
            // 30 Task 6: the matched route's `metadata_match` envoy.lb map travels
            // to the dispatch seam for metadata subset LB. STATIC route config (no
            // request data) — `None` when the route has no `metadata_match`.
            subset_match: ar.metadata_match.as_ref().map(|m| m.envoy_lb.clone()),
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

fn route_matches(
    r: &Route,
    path: &str,
    headers: &[(String, String)],
    runtime: &envoy_config::runtime::RuntimeSnapshot,
) -> bool {
    // 109.1: the runtime_fraction gate, evaluated FIRST (upstream AND-combines
    // it with the path/header criteria; order is behavior-neutral for an AND).
    // `route_fraction_passes` is infallible — every nondeterministic input is
    // boot-fatal at all three validation paths, so the request path never
    // sees an error.
    if let Some(rf) = &r.r#match.runtime_fraction
        && !runtime.route_fraction_passes(rf)
    {
        return false;
    }
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
    crate::date::now_imf_fixdate()
}

fn connection_value(close: bool) -> &'static str {
    if close { "close" } else { "keep-alive" }
}

/// Shared skeleton behind every synth-response builder: `status` + `body`
/// with EXACTLY the 5 standard HTTP/1.1 headers in canonical order
/// `[server, date, content-length, content-type, connection]`
/// (`content-length` derived from `body.len()`). The wrappers below
/// (`synth_direct_response`, `synth_status`, `synth_no_healthy_upstream`,
/// `synth_overflow`) exist so each synth path keeps its documented wire
/// contract; `synth_overflow` appends `x-envoy-overloaded` AFTER these 5.
/// Header ORDER is load-bearing — the differential harness byte-compares
/// against upstream Envoy.
fn synth_with(status: u16, body: Bytes, close: bool) -> Response {
    Response {
        status,
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

fn synth_direct_response(dr: &DirectResponse, close: bool) -> Response {
    let body_str = dr.body.inline_string.as_deref().unwrap_or("");
    synth_with(
        dr.status,
        Bytes::copy_from_slice(body_str.as_bytes()),
        close,
    )
}

pub(crate) fn synth_status(status: u16, close: bool) -> Response {
    synth_with(status, Bytes::new(), close)
}

/// 76.2 (SPEC 2.4): the pure, total outcome of applying a `RedirectAction` to
/// one request. Produced by [`plan_redirect`] with no I/O, so every measured
/// upstream cell is unit-testable without a socket.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RedirectPlan {
    /// The `location:` header value.
    pub location: String,
    /// The wire status, from `RedirectResponseCode::status()`.
    pub status: u16,
    /// `Some(new_path)` exactly when `prefix_rewrite` applied. The dispatch arm
    /// writes it back into `req.path` so the access log observes the rewrite —
    /// MEASURED: `prefix_rewrite` MUTATES the logged `:path` while
    /// `path_redirect` does NOT.
    pub rewritten_path: Option<String>,
}

/// 76.2 (SPEC 2.4): build the redirect plan from the MEASURED upstream rules
/// (a)-(e). Pure and total — it never panics and never touches the network.
///
/// * `authority` — the request's `Host:` header VERBATIM, port included. The
///   authority in `location` comes from that header, NOT from the socket
///   (MEASURED: a `Host:` port differing from the listen port is echoed).
/// * `target` — the raw request target, query included.
/// * `matched_prefix` — the matched route's `match.prefix`, the span that
///   `prefix_rewrite` replaces. `None` (a `path:`-matched route) means the
///   whole path is the matched span.
fn plan_redirect(
    authority: &str,
    target: &str,
    matched_prefix: Option<&str>,
    rd: &RedirectAction,
) -> RedirectPlan {
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (target, None),
    };

    // (a) Scheme. `scheme_redirect` wins and is NOT validated against any
    // allow-list (MEASURED: the literal `ftp` is accepted and emitted
    // verbatim); else `https_redirect: true` forces https; else the scheme the
    // request arrived on. An explicit `https_redirect: false` is the default.
    let scheme = match (rd.scheme_redirect.as_deref(), rd.https_redirect) {
        (Some(s), _) => s,
        (None, Some(true)) => "https",
        (None, _) => "http",
    };

    // (b) Authority — the asymmetry, and the trap. `host_redirect` SET makes
    // the authority that host and DROPS the request's original port;
    // `host_redirect` UNSET preserves the request's authority INCLUDING its
    // port. `port_redirect` overrides the port in BOTH cases and is rendered
    // verbatim with no range clamp (MEASURED: upstream accepts `70000` and
    // emits `:70000`), and a scheme-only change does NOT normalise a now
    // redundant `:443`.
    let host_part = rd.host_redirect.as_deref().unwrap_or(authority);
    let authority_out = match rd.port_redirect {
        Some(port) => format!("{}:{}", strip_port(host_part), port),
        None => host_part.to_string(),
    };

    // (c) Path. The two rewrites are mutually exclusive (rejected at load by
    // the 76.1 oneof validator), and an EMPTY `path_redirect` performs NO
    // rewrite — MEASURED: `path_redirect: ""` leaves the original path.
    let mut rewritten_path = None;
    let new_path = match (
        rd.path_redirect.as_deref().filter(|p| !p.is_empty()),
        rd.prefix_rewrite.as_deref(),
    ) {
        (Some(p), _) => p.to_string(),
        (None, Some(pr)) => {
            // `get(..).unwrap_or("")` keeps the function TOTAL: a matched span
            // longer than the path, or one landing off a UTF-8 boundary, yields
            // an empty tail instead of panicking.
            let matched_len = matched_prefix.map_or(path.len(), str::len);
            let rewritten = format!("{}{}", pr, path.get(matched_len..).unwrap_or(""));
            // The request's own query rides along on the rewritten `:path`;
            // `strip_query` is a location-side rule only.
            rewritten_path = Some(match query {
                Some(q) => format!("{rewritten}?{q}"),
                None => rewritten.clone(),
            });
            rewritten
        }
        (None, None) => path.to_string(),
    };

    // (d) Query. Preserved by default even when `path_redirect` replaced the
    // path wholesale; `strip_query: true` drops it.
    let query_suffix = match (rd.strip_query, query) {
        (false, Some(q)) => format!("?{q}"),
        _ => String::new(),
    };

    RedirectPlan {
        location: format!("{scheme}://{authority_out}{new_path}{query_suffix}"),
        // (e) Status. Default 301; the five `response_code` values map through
        // the 76.1 `RedirectResponseCode::status()` table.
        status: rd.response_code.status(),
        rewritten_path,
    }
}

/// 76.2: the redirect response builder. MEASURED against
/// `envoyproxy/envoy:v1.33.0`: a redirect carries EXACTLY `location`, `date`,
/// `server`, `connection`, `content-length` — and NO `content-type`, which a
/// `direct_response` DOES carry. It therefore must NOT reuse [`synth_with`],
/// whose fixed 5-header list always emits `content-type`; doing so fails the
/// harness's `diff_headers` name-set check with
/// `only-in-envoy-rust=["content-type"]`. Header ORDER matches the measured
/// upstream wire order. Body is empty, `content-length: 0`.
fn synth_redirect(status: u16, location: String, close: bool) -> Response {
    Response {
        status,
        reason: None,
        headers: vec![
            (headers::LOCATION.to_string(), location),
            (headers::DATE.to_string(), now_imf_fixdate()),
            (headers::SERVER.to_string(), DEFAULT_SERVER_NAME.to_string()),
            (
                headers::CONNECTION.to_string(),
                connection_value(close).to_string(),
            ),
            (headers::CONTENT_LENGTH.to_string(), "0".to_string()),
        ],
        body: Bytes::new(),
    }
}

/// 12.2 (parent-12 D6.2 per ADR-0037): no-healthy-upstream synth-503 response.
/// Mirrors `synth_status`'s 5-header shape but emits the 19-byte body
/// `no healthy upstream` (hex `6e 6f 20 68 65 61 6c 74 68 79 20 75 70 73 74
/// 72 65 61 6d`; no trailing newline) matching upstream Envoy v1.33.0's
/// no-healthy-upstream wire shape (§6.2 item-2; locked at parent-12 split
/// `4f9ba04`; ADR-0037). Used ONLY at the `pick() -> None` arm of HCM's
/// per-request dispatch in this file; the connect-fail 503 and other synth
/// paths keep `synth_status`'s empty body.
pub(crate) fn synth_no_healthy_upstream(close: bool) -> Response {
    synth_with(503, Bytes::from_static(b"no healthy upstream"), close)
}

/// 15 D5 (ADR-0043 §6.2 finding 3): the `max_connections` /
/// `max_pending_requests:0` overflow synth-503. Body is the byte-exact
/// 81-byte Envoy local-reply `upstream connect error or disconnect/reset
/// before headers. reset reason: overflow` (no trailing newline). Adds the
/// `x-envoy-overloaded: true` header — the wire surfacing of Envoy's
/// access-log-only `UO` response flag — on top of `synth_status`'s 5 standard
/// HTTP/1.1 headers (6 headers total). Envoy itself omits the `connection`
/// header on this reply; envoy-rust keeps it (allow-listed by the harness —
/// the 0019/0022 synth-503 precedent). Called from BOTH the pool cap-overflow
/// arm AND the pending-overflow arm; H1/H2 parity sibling is
/// `synth_h2_overflow` (Task 5).
fn synth_overflow(close: bool) -> Response {
    let mut resp = synth_with(
        503,
        Bytes::from_static(
            b"upstream connect error or disconnect/reset before headers. reset reason: overflow",
        ),
        close,
    );
    // x-envoy-overloaded goes AFTER the 5 standard headers (wire order).
    resp.headers
        .push(("x-envoy-overloaded".to_string(), "true".to_string()));
    resp
}

/// Decorate a filter-synth response with the standard response headers
/// (`server`, `date`, `content-length`, `content-type`, and — on HTTP/1.1 —
/// `connection`) per phase-09 ADR-0033. Called from both H1 writer-arm sites
/// where a filter emits `Decision::StopAndSend` (decode-side
/// `RequestPath::SynthFromDecode` at the writer-arm match; encode-side
/// `Decision::StopAndSend(replacement)` after the encode iteration), and —
/// via envoy-http2's `decorate_filter_synth_response_h2` wrapper — from the
/// H2 writer path. The 07.1-landed framework converts `FilterResponse` ↔
/// `Response` verbatim; filter implementations are not expected to populate
/// the standard response headers (their responsibility ends at the
/// application-semantic content). This helper brings filter-synth responses
/// to wire-shape parity with the synth-from-build paths (`synth_status`,
/// `synth_direct_response`) that already populate these headers inline.
///
/// `connection`: H1 passes `Some(connection_value(close))`; H2 passes `None`
/// because `connection` is an H2-forbidden hop-by-hop header
/// (RFC 7540 §8.1.2.2).
///
/// Semantics per ADR-0033:
///
/// - `content-length` is ALWAYS set from `resp.body.len()` (overwrites any
///   filter-provided value). The filter's body is the source of truth; a
///   stale filter-provided `content-length` would corrupt downstream parsing.
/// - `server`, `date`, `connection` (when `Some`) are added only-if-missing
///   (case-insensitive name check) — matches the 06.1 D1 / 08.1 D1 dedupe
///   precedent at `crates/envoy-admin/src/handler.rs::serialize_response`. If
///   a filter chooses to set its own value (e.g., `server: my-proxy`), the
///   filter wins.
/// - `content-type` is added only-if-missing AND only when the body is
///   non-empty. Empty-body local replies (e.g. CORS preflight 200) get no
///   `content-type` — matching Envoy v1.33 empirical behaviour confirmed by
///   fixture 0031 §6.2 verification.
///
/// Symmetric to `synth_status` — same defaults (`DEFAULT_SERVER_NAME`,
/// `DEFAULT_CONTENT_TYPE`, `now_imf_fixdate()`, `connection_value(close)`).
/// Push order is load-bearing (differential byte-compare):
/// `[content-length][content-type][server, date][connection?]`.
pub fn decorate_filter_synth_response(resp: &mut Response, connection: Option<&str>) {
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
    // server / date / connection (when requested): add only-if-missing (always).
    // content-type: add only-if-missing AND only when the body is non-empty —
    // upstream Envoy v1.33 does not emit content-type on empty-body local
    // replies (e.g. the CORS preflight 200). The §6.2 empirical verification
    // for fixture 0031-http-filter-cors confirms: upstream returns no
    // content-type header when body length is 0.
    let has_content_type = resp
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case(headers::CONTENT_TYPE));
    if !has_content_type && !resp.body.is_empty() {
        resp.headers.push((
            headers::CONTENT_TYPE.to_string(),
            DEFAULT_CONTENT_TYPE.to_string(),
        ));
    }
    let standards = [
        Some((headers::SERVER, DEFAULT_SERVER_NAME.to_string())),
        Some((headers::DATE, now_imf_fixdate())),
        connection.map(|c| (headers::CONNECTION, c.to_string())),
    ];
    for (name, value) in standards.into_iter().flatten() {
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
pub(crate) fn synth_501(close: bool) -> Response {
    synth_status(501, close)
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::runtime::RuntimeSnapshot;
    use envoy_config::{DataSource, HashPolicyHeader, LbMetadata, RouteAction_Route, RouteMatch};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Phase 32 Task 5 — synthetic `AccessLogRecord` for the
    /// `compiled_log_format` wiring tests: method=GET, response_code=200,
    /// start_time=UNIX_EPOCH (renders as the fixed `1970-01-01T00:00:00.000Z`
    /// bracket), all optional fields `None` so `%UPSTREAM_HOST%` (and the other
    /// `%REQ/RESP%` optionals) render as the `-` sentinel.
    fn record_get_200() -> envoy_accesslog::AccessLogRecord {
        envoy_accesslog::AccessLogRecord {
            start_time: std::time::UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 0,
            duration: Duration::from_millis(0),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: None,
            upstream_host: None,
            upstream_cluster: None,
            route_name: None,
            response_code_details: None,
            dynamic_metadata: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn compiled_log_format_uses_config_string_when_present() {
        let file_cfg = envoy_config::FileAccessLog {
            path: "/tmp/x".into(),
            log_format: Some(envoy_config::SubstitutionFormatString {
                text_format_source: Some(envoy_config::DataSourceInline {
                    inline_string: "%REQ(:METHOD)% %RESPONSE_CODE%".into(),
                }),
                json_format: None,
                omit_empty_values: false,
            }),
        };
        let fmt = compiled_log_format(&file_cfg).expect("valid");
        assert_eq!(fmt.render(&record_get_200()), "GET 200");
    }

    #[test]
    fn compiled_log_format_picks_json_arm() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "c".to_string(),
            envoy_config::JsonFormatValue::Format("%RESPONSE_CODE%".to_string()),
        );
        let file_cfg = envoy_config::FileAccessLog {
            path: "/tmp/x".into(),
            log_format: Some(envoy_config::SubstitutionFormatString {
                text_format_source: None,
                json_format: Some(map),
                omit_empty_values: false,
            }),
        };
        let fmt = compiled_log_format(&file_cfg).unwrap();
        assert!(matches!(fmt, envoy_accesslog::LogFormat::Json(_)));
        assert_eq!(fmt.render(&record_get_200()), "{\"c\":200}\n");
    }

    #[test]
    fn compiled_log_format_picks_json_arm_nested() {
        // Phase 39 T5 (ADR-0094) — a NESTED json_format map threads through the
        // JsonFormatValue → JsonValueInput bridge to a recursive Json LogFormat.
        let mut inner = std::collections::BTreeMap::new();
        inner.insert(
            "code".to_string(),
            envoy_config::JsonFormatValue::Format("%RESPONSE_CODE%".to_string()),
        );
        inner.insert(
            "method".to_string(),
            envoy_config::JsonFormatValue::Format("%REQ(:METHOD)%".to_string()),
        );
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "obj".to_string(),
            envoy_config::JsonFormatValue::Object(inner),
        );
        map.insert(
            "list".to_string(),
            envoy_config::JsonFormatValue::Array(vec![
                envoy_config::JsonFormatValue::Format("%PROTOCOL%".to_string()),
                envoy_config::JsonFormatValue::Bool(true),
                envoy_config::JsonFormatValue::Null,
            ]),
        );
        let file_cfg = envoy_config::FileAccessLog {
            path: "/tmp/x".into(),
            log_format: Some(envoy_config::SubstitutionFormatString {
                text_format_source: None,
                json_format: Some(map),
                omit_empty_values: false,
            }),
        };
        let fmt = compiled_log_format(&file_cfg).unwrap();
        assert_eq!(
            fmt.render(&record_get_200()),
            "{\"list\":[\"HTTP/1.1\",true,null],\"obj\":{\"code\":200,\"method\":\"GET\"}}\n"
        );
    }

    #[test]
    fn compiled_log_format_picks_text_arm() {
        let file_cfg = envoy_config::FileAccessLog {
            path: "/tmp/x".into(),
            log_format: Some(envoy_config::SubstitutionFormatString {
                text_format_source: Some(envoy_config::DataSourceInline {
                    inline_string: "%RESPONSE_CODE%".into(),
                }),
                json_format: None,
                omit_empty_values: false,
            }),
        };
        assert!(matches!(
            compiled_log_format(&file_cfg).unwrap(),
            envoy_accesslog::LogFormat::Text(_)
        ));
    }

    #[test]
    fn compiled_log_format_threads_omit_empty_values_text() {
        // ADR-0096 §B / phase40 T4 — `omit_empty_values: true` on the text arm
        // makes an absent op render `""` (swap), not the `-` sentinel.
        let mk = |omit: bool| envoy_config::FileAccessLog {
            path: "/tmp/x".into(),
            log_format: Some(envoy_config::SubstitutionFormatString {
                text_format_source: Some(envoy_config::DataSourceInline {
                    inline_string: "up=%UPSTREAM_HOST%".into(),
                }),
                json_format: None,
                omit_empty_values: omit,
            }),
        };
        // record_get_200 has upstream_host = None.
        assert_eq!(
            compiled_log_format(&mk(false))
                .unwrap()
                .render(&record_get_200()),
            "up=-"
        );
        assert_eq!(
            compiled_log_format(&mk(true))
                .unwrap()
                .render(&record_get_200()),
            "up="
        );
    }

    #[test]
    fn compiled_log_format_threads_omit_empty_values_json() {
        // ADR-0096 §B/§C — `omit_empty_values: true` on the json arm: the
        // multi-segment leaf gets the swap; a single absent op stays `null`.
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "mixed".to_string(),
            envoy_config::JsonFormatValue::Format("up=%UPSTREAM_HOST%".to_string()),
        );
        map.insert(
            "single".to_string(),
            envoy_config::JsonFormatValue::Format("%UPSTREAM_HOST%".to_string()),
        );
        let mk =
            |omit: bool, map: std::collections::BTreeMap<String, envoy_config::JsonFormatValue>| {
                envoy_config::FileAccessLog {
                    path: "/tmp/x".into(),
                    log_format: Some(envoy_config::SubstitutionFormatString {
                        text_format_source: None,
                        json_format: Some(map),
                        omit_empty_values: omit,
                    }),
                }
            };
        assert_eq!(
            compiled_log_format(&mk(false, map.clone()))
                .unwrap()
                .render(&record_get_200()),
            "{\"mixed\":\"up=-\",\"single\":null}\n"
        );
        assert_eq!(
            compiled_log_format(&mk(true, map))
                .unwrap()
                .render(&record_get_200()),
            "{\"mixed\":\"up=\",\"single\":null}\n" // §C: single stays null
        );
    }

    #[test]
    fn compiled_log_format_falls_back_to_default_when_absent() {
        let file_cfg = envoy_config::FileAccessLog {
            path: "/tmp/x".into(),
            log_format: None,
        };
        let fmt = compiled_log_format(&file_cfg).expect("default");
        let line = fmt.render(&record_get_200());
        assert!(
            line.starts_with("[1970-01-01T00:00:00.000Z] "),
            "line: {line}"
        );
        assert!(
            line.ends_with("\"-\"\n"),
            "default render ends with the last token + newline: {line}"
        );
    }

    /// 28 Task 6 (c) — THE load-bearing empty-vs-absent distinction (ADR-0070).
    /// A header that is PRESENT but EMPTY hashes to `xxh64(b"")` (deterministic,
    /// NOT the fallback); a header that is ABSENT yields `None` (the random-host
    /// fallback). This guards against the classic `.filter(|v| !v.is_empty())`
    /// collapse bug that wrongly treats present-empty as absent.
    #[test]
    fn request_hash_key_present_empty_is_some_not_none() {
        let policies = vec![HashPolicy {
            header: Some(HashPolicyHeader {
                header_name: "x-hash-key".to_string(),
            }),
            ..Default::default()
        }];

        // PRESENT but EMPTY → Some(xxh64(b"")) — NOT None.
        let present_empty = request_hash_key(&policies, |name| {
            if name == "x-hash-key" {
                Some(b"".as_slice())
            } else {
                None
            }
        });
        assert_eq!(
            present_empty,
            Some(envoy_cluster::hash_request_key(b"")),
            "present-empty header must hash to xxh64(b\"\"), NOT fall back to None"
        );

        // ABSENT → None (the random-host fallback path).
        let absent = request_hash_key(&policies, |_name| None);
        assert_eq!(absent, None, "absent header must yield None (fallback)");
    }

    /// 28 Task 6 (d): a present, non-empty header value is hashed and threaded.
    #[test]
    fn request_hash_key_present_nonempty_is_hashed() {
        let policies = vec![HashPolicy {
            header: Some(HashPolicyHeader {
                header_name: "x-hash-key".to_string(),
            }),
            ..Default::default()
        }];
        let key = request_hash_key(&policies, |name| {
            if name == "x-hash-key" {
                Some(b"key-0".as_slice())
            } else {
                None
            }
        });
        assert_eq!(
            key,
            Some(envoy_cluster::hash_request_key(b"key-0")),
            "present non-empty header is hashed via hash_request_key"
        );
    }

    /// 28 Task 6: an empty `hash_policy` (the regression-equivalence default for
    /// every non-RING_HASH route) yields `None` without consulting the lookup.
    #[test]
    fn request_hash_key_empty_policy_is_none() {
        let key = request_hash_key(&[], |_name| panic!("lookup must not be called"));
        assert_eq!(key, None, "empty hash_policy → None");
    }

    /// Optional `circuit_breakers` shape for [`cluster_mgr_with_endpoint_opts`].
    enum TestBreakers {
        /// No `circuit_breakers` block at all.
        Absent,
        /// A single DEFAULT-priority threshold with `track_remaining: true`
        /// and an optional cap line rendered BEFORE it, e.g.
        /// `Some(("max_retries", 2))` or `Some(("max_requests", 0))`.
        Threshold(Option<(&'static str, u32)>),
    }

    /// Shared builder behind `cluster_mgr_with_endpoint`,
    /// `cluster_mgr_with_endpoint_max_retries` and
    /// `cluster_mgr_with_endpoint_max_requests`: a ClusterManager with a
    /// single static cluster `name` whose only endpoint is
    /// `127.0.0.1:<port>`, plus the StatsRegistry it was built against
    /// (only the `_max_requests` wrapper consumes the registry — see its doc).
    async fn cluster_mgr_with_endpoint_opts(
        name: &str,
        port: u16,
        breakers: TestBreakers,
    ) -> (
        Arc<envoy_cluster::ClusterManager>,
        Arc<envoy_stats::StatsRegistry>,
    ) {
        let circuit_breakers_block = match breakers {
            TestBreakers::Absent => String::new(),
            TestBreakers::Threshold(cap) => {
                let cap_line = match cap {
                    Some((key, n)) => format!("            {key}: {n}\n"),
                    None => String::new(),
                };
                format!(
                    "      circuit_breakers:\n        \
                     thresholds:\n          \
                     - priority: DEFAULT\n\
                     {cap_line}            track_remaining: true\n"
                )
            }
        };
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
{circuit_breakers_block}      load_assignment:
        cluster_name: {name}
        endpoints:
          - lb_endpoints:
              - endpoint: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: {port} }} }} }}
"#
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

    /// Build a ClusterManager with a single static cluster `name` whose only
    /// endpoint is `127.0.0.1:<port>`. Reused by the 04.3 Task 9 router-proxy
    /// arm tests.
    async fn cluster_mgr_with_endpoint(
        name: &str,
        port: u16,
    ) -> Arc<envoy_cluster::ClusterManager> {
        cluster_mgr_with_endpoint_opts(name, port, TestBreakers::Absent)
            .await
            .0
    }

    /// 17 Task 4: like `cluster_mgr_with_endpoint` but with a
    /// `circuit_breakers` block. `track_remaining` is always set so the
    /// remaining gauges register too (inert for these tests). When
    /// `max_retries` is `Some(n)` the explicit cap is emitted; `None` omits
    /// the `max_retries:` line so the default cap (3) applies. Used by the
    /// retry-budget gate tests to build a cluster whose `try_acquire_retry`
    /// actively gates.
    async fn cluster_mgr_with_endpoint_max_retries(
        name: &str,
        port: u16,
        max_retries: Option<u32>,
    ) -> Arc<envoy_cluster::ClusterManager> {
        cluster_mgr_with_endpoint_opts(
            name,
            port,
            TestBreakers::Threshold(max_retries.map(|n| ("max_retries", n))),
        )
        .await
        .0
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
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some(prefix.to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status,
                            body: DataSource {
                                filename: None,
                                inline_string: Some(body.to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
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

    /// 25.2 M25.1-2: like `drive`, but writes the request HEAD and BODY in two
    /// separate `write_all`s with a flush + sleep between them, forcing the kernel
    /// to deliver them in distinct reads on loopback (so the body-read reassembly
    /// loop `while remaining > 0` actually runs). Reuses `drive`'s single-connection
    /// serve-and-connect idiom.
    async fn drive_split(config: Arc<HCMConfig>, head: &[u8], body: &[u8]) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let _ = serve_connection(config, sock).await;
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(head).await.unwrap();
        client.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await; // force a segment boundary
        client.write_all(body).await.unwrap();
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).await.unwrap();
        drop(client);
        let _ = server.await;
        buf
    }

    /// 25.1 D1: drive multiple requests over ONE keep-alive connection, mirroring
    /// `drive`'s single-connection serve-and-connect idiom. Writes every request
    /// up front (the LAST request must carry `Connection: close` so the server
    /// returns and the client sees EOF), reads the concatenated responses to EOF,
    /// then splits them on the `HTTP/1.1 ` status-line boundary. Used to prove
    /// that request 1's body is fully consumed from `buf` so request 2 parses
    /// cleanly on the same connection.
    async fn drive_keep_alive(config: Arc<HCMConfig>, requests: &[&[u8]]) -> Vec<Vec<u8>> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let _ = serve_connection(config, sock).await;
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        for req in requests {
            client.write_all(req).await.unwrap();
        }
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).await.unwrap();
        drop(client);
        let _ = server.await;
        // Split the concatenated responses on the status-line boundary.
        let marker = b"HTTP/1.1 ";
        let mut starts: Vec<usize> = Vec::new();
        let mut i = 0;
        while i + marker.len() <= buf.len() {
            if &buf[i..i + marker.len()] == marker {
                starts.push(i);
                i += marker.len();
            } else {
                i += 1;
            }
        }
        let mut out = Vec::new();
        for (k, &start) in starts.iter().enumerate() {
            let end = starts.get(k + 1).copied().unwrap_or(buf.len());
            out.push(buf[start..end].to_vec());
        }
        out
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
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "specific".to_string(),
                    domains: vec!["foo.example.com".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("hit\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
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
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![
                        Route {
                            name: String::new(),
                            r#match: RouteMatch {
                                prefix: Some("/healthz".to_string()),
                                path: None,
                                headers: vec![],
                                runtime_fraction: None,
                            },
                            action: RouteAction::DirectResponse(DirectResponse {
                                status: 200,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("first\n".to_string()),
                                },
                            }),
                            typed_per_filter_config: Default::default(),
                        },
                        Route {
                            name: String::new(),
                            r#match: RouteMatch {
                                prefix: Some("/".to_string()),
                                path: None,
                                headers: vec![],
                                runtime_fraction: None,
                            },
                            action: RouteAction::DirectResponse(DirectResponse {
                                status: 500,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("never\n".to_string()),
                                },
                            }),
                            typed_per_filter_config: Default::default(),
                        },
                    ],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
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
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "specific".to_string(),
                    domains: vec!["only.example.com".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
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
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "test_rc".into(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "test_vh".into(),
                    domains: vec!["*".into()],
                    include_attempt_count_in_response: false,
                    routes,
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
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
        super::decorate_filter_synth_response(&mut resp, Some("close"));
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
        super::decorate_filter_synth_response(&mut resp, Some("keep-alive"));
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

    #[test]
    fn decorate_omits_content_type_when_body_is_empty() {
        // Empty-body local reply (e.g. CORS preflight 200): content-type must
        // NOT be added, matching Envoy v1.33 empirical behaviour. server/date
        // MUST still be added (they are unconditional on body size).
        let mut resp = Response {
            status: 200,
            reason: Some("OK"),
            headers: Vec::new(),
            body: bytes::Bytes::new(),
        };
        super::decorate_filter_synth_response(&mut resp, Some("keep-alive"));
        let name = |n: &str| -> Option<&str> {
            resp.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, v)| v.as_str())
        };
        // content-length must be "0".
        assert_eq!(name(headers::CONTENT_LENGTH), Some("0"));
        // server and date MUST be added.
        assert!(name(headers::SERVER).is_some(), "server header missing");
        let date = name(headers::DATE).expect("date header added");
        assert!(!date.is_empty(), "date header empty: {date:?}");
        // connection added (keep-alive for close=false).
        assert_eq!(name(headers::CONNECTION), Some("keep-alive"));
        // content-type MUST NOT be added for empty body.
        assert!(
            name(headers::CONTENT_TYPE).is_none(),
            "content-type must NOT be added for empty body; headers: {:?}",
            resp.headers
        );
    }

    // ── 04.2 header-matcher HCM integration tests ────────────────────────────

    #[tokio::test]
    async fn route_with_no_headers_matches_unchanged() {
        // Regression: a route with empty headers Vec still matches on path only.
        let cfg = build_test_config(vec![Route {
            name: String::new(),
            r#match: RouteMatch {
                prefix: Some("/".into()),
                path: None,
                headers: vec![],
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok\n".into()),
                },
            }),
            typed_per_filter_config: Default::default(),
        }])
        .await;
        let req = b"GET /healthz HTTP/1.1\r\nHost: x.test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        assert!(std::str::from_utf8(&resp).unwrap().contains("200 OK"));
    }

    #[tokio::test]
    async fn single_header_matcher_route_selected_when_match() {
        let matcher_route = Route {
            name: String::new(),
            r#match: RouteMatch {
                prefix: Some("/api/".into()),
                path: None,
                headers: vec![envoy_config::HeaderMatcher {
                    name: "x-foo".into(),
                    mode: envoy_config::HeaderMatcherMode::ExactMatch("bar".into()),
                    invert_match: false,
                }],
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 418,
                body: DataSource {
                    filename: None,
                    inline_string: Some("teapot\n".into()),
                },
            }),
            typed_per_filter_config: Default::default(),
        };
        let default_route = Route {
            name: String::new(),
            r#match: RouteMatch {
                prefix: Some("/".into()),
                path: None,
                headers: vec![],
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok\n".into()),
                },
            }),
            typed_per_filter_config: Default::default(),
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
            name: String::new(),
            r#match: RouteMatch {
                prefix: Some("/api/".into()),
                path: None,
                headers: vec![envoy_config::HeaderMatcher {
                    name: "x-foo".into(),
                    mode: envoy_config::HeaderMatcherMode::ExactMatch("bar".into()),
                    invert_match: false,
                }],
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 418,
                body: DataSource {
                    filename: None,
                    inline_string: Some("teapot\n".into()),
                },
            }),
            typed_per_filter_config: Default::default(),
        };
        let default_route = Route {
            name: String::new(),
            r#match: RouteMatch {
                prefix: Some("/".into()),
                path: None,
                headers: vec![],
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok\n".into()),
                },
            }),
            typed_per_filter_config: Default::default(),
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
            name: String::new(),
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
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 418,
                body: DataSource {
                    filename: None,
                    inline_string: Some("teapot\n".into()),
                },
            }),
            typed_per_filter_config: Default::default(),
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
            name: String::new(),
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
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 418,
                body: DataSource {
                    filename: None,
                    inline_string: Some("teapot\n".into()),
                },
            }),
            typed_per_filter_config: Default::default(),
        };
        let default_route = Route {
            name: String::new(),
            r#match: RouteMatch {
                prefix: Some("/".into()),
                path: None,
                headers: vec![],
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok\n".into()),
                },
            }),
            typed_per_filter_config: Default::default(),
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

    // ── Content-Length smuggling (RFC 7230 §3.3.3) ────────────────────────────

    fn cl_headers(values: &[&str]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|v| ("Content-Length".to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn parse_content_length_absent_is_zero() {
        assert_eq!(parse_content_length(&[]).unwrap(), 0);
        let other = vec![("Host".to_string(), "x".to_string())];
        assert_eq!(parse_content_length(&other).unwrap(), 0);
    }

    #[test]
    fn parse_content_length_single_value() {
        assert_eq!(parse_content_length(&cl_headers(&["42"])).unwrap(), 42);
    }

    #[test]
    fn parse_content_length_non_numeric_rejected() {
        assert!(matches!(
            parse_content_length(&cl_headers(&["not-a-number"])),
            Err(Http1Error::MalformedHeader)
        ));
    }

    #[test]
    fn parse_content_length_identical_duplicates_tolerated() {
        // RFC 7230 §3.3.3: repeated identical Content-Length values may be
        // combined into a single value — accept.
        assert_eq!(
            parse_content_length(&cl_headers(&["7", "7", "7"])).unwrap(),
            7
        );
    }

    #[test]
    fn parse_content_length_conflicting_duplicates_rejected() {
        // The CL/CL request-smuggling vector: two Content-Length rows with
        // different values. RFC 7230 §3.3.3 requires rejection; upstream Envoy
        // returns 400. Previously this silently framed on the FIRST value (5) and
        // left the second body as a pipelined next request. Now it is rejected as
        // MalformedHeader — the same disposition as a non-numeric value.
        assert!(matches!(
            parse_content_length(&cl_headers(&["5", "6"])),
            Err(Http1Error::MalformedHeader)
        ));
        // Order-independent: the conflict is detected regardless of which value
        // appears first.
        assert!(matches!(
            parse_content_length(&cl_headers(&["6", "5"])),
            Err(Http1Error::MalformedHeader)
        ));
    }

    #[tokio::test]
    async fn conflicting_content_length_request_is_rejected_no_response() {
        // End-to-end: a request carrying two conflicting Content-Length rows must
        // NOT be served (it would otherwise smuggle a second request). The codec
        // rejects it and the connection is dropped with no HTTP response written.
        let config = hcm_config_single_route("/", 200, "ok\n").await;
        let req =
            b"POST /up HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabc";
        let resp = drive(config, req).await;
        assert!(
            resp.is_empty(),
            "conflicting Content-Length must be rejected with no response, got: {}",
            String::from_utf8_lossy(&resp)
        );
    }

    #[tokio::test]
    async fn identical_duplicate_content_length_request_is_served() {
        // The tolerant half: identical repeated Content-Length values frame a
        // single body and the request is served normally (direct_response route).
        let config = hcm_config_single_route("/", 200, "ok\n").await;
        let req =
            b"POST /up HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 200 OK\r\n"),
            "identical duplicate Content-Length must be served, got: {resp_str}"
        );
    }

    // ── Transfer-Encoding smuggling variants ──────────────────────────────────

    fn te_headers(values: &[&str]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|v| ("Transfer-Encoding".to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn chunked_detection_covers_multi_token_and_multi_row() {
        // Exact "chunked" (the pre-existing case).
        assert!(has_chunked_transfer_encoding(&te_headers(&["chunked"])));
        assert!(has_chunked_transfer_encoding(&te_headers(&["Chunked"])));
        // Multi-token values: chunked in any position must be detected. The old
        // exact-match check missed these, silently falling back to
        // Content-Length framing (a TE/CL smuggling desync).
        assert!(has_chunked_transfer_encoding(&te_headers(&[
            "chunked, gzip"
        ])));
        assert!(has_chunked_transfer_encoding(&te_headers(&[
            "gzip, chunked"
        ])));
        assert!(has_chunked_transfer_encoding(&te_headers(&[
            "gzip ,  chunked "
        ])));
        // Split across two Transfer-Encoding rows.
        assert!(has_chunked_transfer_encoding(&te_headers(&[
            "gzip", "chunked"
        ])));
        // No chunked token present → not chunked.
        assert!(!has_chunked_transfer_encoding(&te_headers(&["gzip"])));
        assert!(!has_chunked_transfer_encoding(&[]));
    }

    #[tokio::test]
    async fn transfer_encoding_chunked_gzip_is_rejected_501_not_cl_framed() {
        // Regression: `Transfer-Encoding: chunked, gzip` parses cleanly, so the
        // former exact-"chunked" detector treated it as a plain (non-chunked)
        // request and framed on Content-Length. It must instead be rejected 501,
        // the same as a bare `chunked`, closing the desync.
        let config = hcm_config_single_route("/", 200, "ok\n").await;
        let req = b"POST /up HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked, gzip\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 501 Not Implemented\r\n"),
            "got: {resp_str}"
        );
    }

    #[test]
    fn codec_rejects_obs_fold_and_smuggling_shaped_headers() {
        // Characterization of the codec's rejection surface (via httparse), pinned
        // so a parser swap can't silently loosen it. All of these are rejected —
        // matching upstream Envoy's stance on smuggling-shaped request headers.
        // obs-fold (obsolete line folding, RFC 7230 §3.2.4 — must be rejected):
        assert!(
            Http1Codec::parse_request(b"GET / HTTP/1.1\r\nHost: x\r\nX-A: 1\r\n 2\r\n\r\n")
                .is_err()
        );
        // space inside a header name:
        assert!(
            Http1Codec::parse_request(b"GET / HTTP/1.1\r\nHost: x\r\nFoo Bar: 1\r\n\r\n").is_err()
        );
        // NUL byte in a header value:
        assert!(
            Http1Codec::parse_request(b"GET / HTTP/1.1\r\nHost: x\r\nX-A: a\x00b\r\n\r\n").is_err()
        );
    }

    // ── Slow-client idle-read timeout ─────────────────────────────────────────

    #[tokio::test(start_paused = true)]
    async fn slow_client_partial_request_hits_idle_read_timeout() {
        // IDLE_READ_TIMEOUT guards the request-head read, but no test exercised
        // the deadline — a regression that removed or lengthened it would be
        // invisible. With the clock paused, tokio auto-advances virtual time to
        // the next armed timer once the runtime is otherwise idle, so the
        // idle-read branch fires DETERMINISTICALLY (no real 5s wait, no fixed
        // sleep). A slow client that sends a partial request head and then stalls
        // must have its connection cleanly closed with no response written.
        let config = hcm_config_single_route("/", 200, "ok\n").await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            serve_connection(config, sock).await
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        // Complete-looking but missing the terminating CRLF: the codec returns
        // Ok(None) (incomplete) so the server parks on the idle read.
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n")
            .await
            .unwrap();
        client.flush().await.unwrap();
        // Never send the rest. The idle timer is the only way the server can make
        // progress, so virtual time advances to it.
        let result = server.await.unwrap();
        assert!(
            result.is_ok(),
            "idle-read timeout must be a clean close, got: {result:?}"
        );
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).await.unwrap();
        assert!(
            buf.is_empty(),
            "idle-timed-out request must produce no response, got: {}",
            String::from_utf8_lossy(&buf)
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
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "rc".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some(prefix.to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action,
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
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

    /// 14.2 D4: build a ClusterManager with a single static cluster `name`
    /// whose only endpoint is `127.0.0.1:<port>` AND an `outlier_detection`
    /// block configured with `consecutive_5xx: <c5xx>`. Mirrors
    /// `cluster_mgr_with_endpoint` plus the `outlier_detection` YAML stanza —
    /// the same shape the envoy-cluster `from_bootstrap` tests parse. Used by
    /// the D4 response-receipt-hook test below.
    ///
    /// `max_ejection_percent: 100` is REQUIRED for a single-endpoint cluster:
    /// the cap is `floor(host_count * max_ejection_percent / 100)`, so the
    /// Envoy default of 10 yields `floor(1 * 10 / 100) = 0` — the only endpoint
    /// could never be ejected (overflow on the first crossing). 100 yields
    /// `cap_count = 1`, permitting the single endpoint's ejection.
    async fn cluster_mgr_with_outlier_detection(
        name: &str,
        port: u16,
        c5xx: u32,
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
      outlier_detection:
        consecutive_5xx: {c5xx}
        max_ejection_percent: 100
"#
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("bootstrap parses");
        Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
                .await
                .expect("cluster mgr"),
        )
    }

    /// 14.2 D4 (lock-in #9): the H1 router-proxy arm records the FINAL upstream
    /// response status against the picked endpoint's outlier-detection state.
    /// With `consecutive_5xx: 1` and a backend that returns 500, a single
    /// proxied request crosses the threshold and ejects the endpoint — proving
    /// `cluster.record_response(endpoint, outgoing.status)` fired with the 500.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_router_arm_records_response_and_ejects_after_threshold() {
        let upstream_response: &'static [u8] =
            b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
        let upstream_port = spawn_in_process_upstream(upstream_response).await;
        let cluster_mgr = cluster_mgr_with_outlier_detection("backend", upstream_port, 1).await;
        // Keep a handle to assert ejection after the request drives through.
        let cluster = cluster_mgr.get("backend").expect("backend cluster present");
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
            }),
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 500 Internal Server Error\r\n"),
            "first request proxies the backend 500: {s}"
        );
        assert!(
            cluster.is_endpoint_ejected_for_test(0),
            "D4: record_response(endpoint, 500) ejected the endpoint at threshold 1",
        );
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
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
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
        // Route arm should propagate the connect failure as a 503 Service
        // Unavailable downstream response.
        let cluster_mgr = cluster_mgr_with_endpoint("backend", 1).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
            }),
            cluster_mgr,
        );
        let req = b"GET /any HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
            "expected 503 on UpstreamConnect, got: {s}"
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
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
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

    /// 25.1 D1: like `spawn_in_process_upstream`, but RECORDS the bytes the
    /// upstream received (request head + body) into the returned shared buffer,
    /// so a test can assert the forwarded request body. Reads in a loop with a
    /// short per-read timeout; once a read times out (the small test request has
    /// fully arrived) it stops reading and writes the canned `response`. Returns
    /// `(port, captured)`.
    ///
    /// NOTE: named `spawn_recording_upstream` to avoid colliding with the
    /// pre-existing `spawn_capturing_upstream` (which returns a `JoinHandle`,
    /// single-connection, single-read). This loop-with-timeout + `Arc<Mutex>`
    /// form is required so the test can read the captured bytes synchronously
    /// after `drive` and so a body arriving in a second TCP segment is still
    /// captured.
    async fn spawn_recording_upstream(
        response: &'static [u8],
    ) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_acceptor = captured.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let captured_conn = captured_acceptor.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    loop {
                        match tokio::time::timeout(Duration::from_millis(200), sock.read(&mut buf))
                            .await
                        {
                            Ok(Ok(0)) => break, // peer closed
                            Ok(Ok(n)) => captured_conn.lock().unwrap().extend_from_slice(&buf[..n]),
                            Ok(Err(_)) => break,    // io error
                            Err(_elapsed) => break, // request fully arrived
                        }
                    }
                    let _ = sock.write_all(response).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        (port, captured)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h1_forwards_request_body_upstream() {
        // 25.1 D1: an H1 POST with a Content-Length-delimited body must reach the
        // upstream with its body intact (today it does not — the router forwards an
        // always-empty body and drains-and-discards the downstream body).
        let upstream_response: &'static [u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let (upstream_port, captured) = spawn_recording_upstream(upstream_response).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
            }),
            cluster_mgr,
        );
        let req = b"POST /submit HTTP/1.1\r\nHost: x.test\r\nContent-Length: 11\r\nConnection: close\r\n\r\nhello world";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 200 OK\r\n"),
            "downstream got 200: {s}"
        );

        let got = captured.lock().unwrap().clone();
        let got_str = String::from_utf8_lossy(&got);
        assert!(
            got_str.starts_with("POST /submit HTTP/1.1\r\n"),
            "upstream received the request line: {got_str}"
        );
        assert!(
            got_str.ends_with("hello world"),
            "upstream received the request body bytes: {got_str}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h1_forwards_body_split_across_tcp_segments() {
        // 25.2 M25.1-2: head and body arrive in SEPARATE reads, so the body-read
        // reassembly loop (`while remaining > 0`) actually runs. Assert the upstream
        // still receives the full forwarded body.
        let (upstream_port, captured) =
            spawn_recording_upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
            }),
            cluster_mgr,
        );
        let resp = drive_split(
            cfg,
            b"POST /seg HTTP/1.1\r\nHost: x.test\r\nContent-Length: 11\r\nConnection: close\r\n\r\n",
            b"hello world",
        )
        .await;
        assert!(
            String::from_utf8_lossy(&resp).starts_with("HTTP/1.1 200 OK\r\n"),
            "downstream got 200"
        );
        let got = captured.lock().unwrap().clone();
        let got_str = String::from_utf8_lossy(&got);
        assert!(
            got_str.starts_with("POST /seg HTTP/1.1\r\n"),
            "upstream got the request: {got_str}"
        );
        assert!(
            got_str.ends_with("hello world"),
            "upstream got the reassembled body: {got_str}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h1_forwards_large_body_grows_on_demand() {
        // 25.2 M25.1-1: a body larger than one 4 KiB read chunk is forwarded
        // byte-exact even though the up-front reservation is now bounded — proves
        // `extend_from_slice` grows the buffer correctly.
        let (upstream_port, captured) =
            spawn_recording_upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
            }),
            cluster_mgr,
        );
        let body = vec![b'z'; 10_000]; // > one 4 KiB read chunk
        let mut req = format!(
            "POST /big HTTP/1.1\r\nHost: x.test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        req.extend_from_slice(&body);
        let resp = drive(cfg, &req).await;
        assert!(String::from_utf8_lossy(&resp).starts_with("HTTP/1.1 200 OK\r\n"));
        let got = captured.lock().unwrap().clone();
        assert!(
            got.ends_with(&body),
            "upstream received the full 10 KB body verbatim"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h1_chunked_request_still_501_after_body_forwarding() {
        // 25.1 D1 regression: chunked requests carry no Content-Length, so the
        // body-read is skipped (body_len == 0) and the existing 501 rejection stands.
        let (upstream_port, _captured) =
            spawn_recording_upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
            }),
            cluster_mgr,
        );
        let req = b"POST /c HTTP/1.1\r\nHost: x.test\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n0\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 501 "),
            "chunked request is 501-rejected: {s}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h1_bodyless_get_unchanged_after_body_forwarding() {
        // 25.1 D1 regression: a GET with no body proxies exactly as before
        // (body_len == 0 → the body-read block is a no-op beyond the head advance).
        let (upstream_port, captured) =
            spawn_recording_upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
            }),
            cluster_mgr,
        );
        let req = b"GET /g HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        assert!(
            String::from_utf8_lossy(&resp).starts_with("HTTP/1.1 200 OK\r\n"),
            "bodyless GET proxies"
        );
        let got = String::from_utf8_lossy(&captured.lock().unwrap()).to_string();
        assert!(
            got.starts_with("GET /g HTTP/1.1\r\n"),
            "upstream got the GET: {got}"
        );
        // No request body bytes were appended after the head terminator.
        let body_after_head = got.split("\r\n\r\n").nth(1).unwrap_or("");
        assert!(
            body_after_head.is_empty(),
            "no body forwarded for a GET: {got:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h1_keep_alive_two_bodied_posts_do_not_bleed() {
        // 25.1 D1 regression: on a single keep-alive connection, request 1's body
        // bytes must be fully consumed from `buf` so request 2 parses cleanly.
        let (upstream_port, captured) =
            spawn_recording_upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
        let cfg = hcm_config_with_cluster(
            "/",
            RouteAction::Route(RouteAction_Route {
                cluster: "backend".into(),
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
            }),
            cluster_mgr,
        );
        let resps = drive_keep_alive(
            cfg,
            &[
                b"POST /one HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\n\r\naaa",
                b"POST /two HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\nConnection: close\r\n\r\nbbbb",
            ],
        )
        .await;
        assert_eq!(resps.len(), 2, "two responses");
        assert!(
            resps
                .iter()
                .all(|r| String::from_utf8_lossy(r).starts_with("HTTP/1.1 200 OK"))
        );
        let got = String::from_utf8_lossy(&captured.lock().unwrap()).to_string();
        assert!(got.contains("POST /one"), "upstream saw request 1: {got}");
        assert!(got.contains("aaa"), "upstream saw body 1: {got}");
        assert!(
            got.contains("POST /two"),
            "upstream saw request 2 (clean parse): {got}"
        );
        assert!(got.contains("bbbb"), "upstream saw body 2: {got}");
    }

    /// 25.1 D1: stateful recording upstream for the retry-replay regression.
    /// RECORDS every received connection's bytes into the shared buffer (so the
    /// test can assert the body bytes appear once per attempt), and returns
    /// `fail_status` (CL: 0) for the first connection then 200 for all later
    /// connections — exercising the retry path. Mirrors `spawn_recording_upstream`
    /// (loop-accept + per-connection read-until-timeout into a shared `Arc<Mutex>`)
    /// fused with `spawn_fail_then_ok_upstream`'s stateful status selection.
    async fn spawn_fail_then_ok_recording_upstream(
        fail_status: u16,
    ) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<u8>>>) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_acceptor = captured.clone();
        let counter = std::sync::Arc::new(AtomicUsize::new(0));
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let n = counter.fetch_add(1, Ordering::SeqCst);
                let captured_conn = captured_acceptor.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    loop {
                        match tokio::time::timeout(Duration::from_millis(200), sock.read(&mut buf))
                            .await
                        {
                            Ok(Ok(0)) => break,
                            Ok(Ok(m)) => captured_conn.lock().unwrap().extend_from_slice(&buf[..m]),
                            Ok(Err(_)) => break,
                            Err(_elapsed) => break,
                        }
                    }
                    let resp: Vec<u8> = if n == 0 {
                        format!("HTTP/1.1 {fail_status} X\r\nContent-Length: 0\r\n\r\n")
                            .into_bytes()
                    } else {
                        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec()
                    };
                    let _ = sock.write_all(&resp).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        (port, captured)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h1_retried_post_replays_body() {
        // 25.1 D1 regression: a POST whose first upstream attempt 503s is retried,
        // and BOTH attempts carry the request body — proving the per-attempt
        // `req.body.clone()` replays the body (the only body source) on each try.
        let (port, captured) = spawn_fail_then_ok_recording_upstream(503).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let cfg = hcm_config_with_retry(
            "/",
            "backend",
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            false,
            cluster_mgr,
        );
        let req = b"POST /r HTTP/1.1\r\nHost: x.test\r\nContent-Length: 8\r\nConnection: close\r\n\r\nreplayme";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 200 OK\r\n"),
            "downstream is 200 after the retry: {s}"
        );
        let got = String::from_utf8_lossy(&captured.lock().unwrap()).to_string();
        // The body bytes appear once per attempt (original + retry).
        let body_count = got.matches("replayme").count();
        assert_eq!(
            body_count, 2,
            "body must be replayed on each of the two attempts: {got:?}"
        );
        assert_eq!(
            got.matches("POST /r HTTP/1.1\r\n").count(),
            2,
            "two upstream attempts: {got:?}"
        );
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
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
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
                retry_policy: None,
                hash_policy: vec![],
                metadata_match: None,
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
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            }),
            rds: None,
            http_filters: vec![envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            }],
        };
        let hcm_config = Arc::new(
            HCMConfig::from_config(
                &envoy_cfg,
                cluster_mgr,
                Arc::clone(&registry),
                None,
                Arc::new(RuntimeSnapshot::default()),
            )
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
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("body\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            }),
            rds: None,
            http_filters: vec![envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            }],
        };
        let hcm_config = Arc::new(
            HCMConfig::from_config(
                &envoy_cfg,
                cluster_mgr,
                Arc::clone(&registry),
                None,
                Arc::new(RuntimeSnapshot::default()),
            )
            .await
            .expect("HCMConfig builds"),
        );
        (hcm_config, registry)
    }

    /// Phase 70 Task 6: build an HCMConfig via the production `from_config`
    /// constructor with ONE file access-log sink at `path` and a single
    /// `/`-prefix direct-response route returning `status`. `filter` is
    /// `Some((op, value))` for a sink carrying a `status_code_filter`, or
    /// `None` for the unfiltered (pre-phase-70 parity) shape. Routed through
    /// `from_config` — not a struct literal — so the config → runtime filter
    /// compilation is the thing under test.
    async fn hcm_config_with_filtered_access_log(
        filter: Option<(envoy_config::ComparisonOp, u32)>,
        path: &std::path::Path,
        status: u16,
    ) -> Arc<HCMConfig> {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = cluster_mgr_empty().await;
        let envoy_cfg = envoy_config::HttpConnectionManagerConfig {
            stat_prefix: "ingress_http".to_string(),
            codec_type: envoy_config::CodecType::HTTP1,
            http2_protocol_options: None,
            access_log: vec![envoy_config::AccessLog {
                name: "envoy.access_loggers.file".to_string(),
                typed_config: envoy_config::AccessLogTypedConfig::FileAccessLog(
                    envoy_config::FileAccessLog {
                        path: path.to_string_lossy().into_owned(),
                        log_format: None,
                    },
                ),
                filter: filter.map(|(op, value)| envoy_config::AccessLogFilter {
                    status_code_filter: Some(envoy_config::StatusCodeFilter {
                        comparison: envoy_config::ComparisonFilter {
                            op,
                            value: envoy_config::RuntimeUInt32 {
                                default_value: value,
                                runtime_key: "access_log.status_code".to_string(),
                            },
                        },
                    }),
                    response_flag_filter: None,
                    header_filter: None,
                    and_filter: None,
                    or_filter: None,
                    metadata_filter: None,
                }),
            }],
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("body\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            }),
            rds: None,
            http_filters: vec![envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            }],
        };
        Arc::new(
            HCMConfig::from_config(
                &envoy_cfg,
                cluster_mgr,
                registry,
                None,
                Arc::new(RuntimeSnapshot::default()),
            )
            .await
            .expect("HCMConfig builds"),
        )
    }

    /// Phase 70 Task 6: `from_config` compiles an `AccessLog.filter`'s
    /// `status_code_filter` into the built `FileSink`'s runtime predicate.
    ///
    /// Table-driven across ALL THREE shipped operators so the config
    /// `ComparisonOp` → runtime `FilterOp` mapping in
    /// `compile_access_log_filter` is pinned arm-by-arm. Each row is uniquely
    /// satisfied by its own operator (all six op×row combinations checked, and
    /// the `Le` leg's `(100, true)` probe is what separates `Le` from `Eq`) —
    /// swapping any two arms of that match makes this test RED.
    ///
    /// Two of this comment's original claims are struck rather than deleted
    /// (D-3.5), corrected at the second §5.2 re-entry:
    /// - ~~"Each leg probes a status the operator must KEEP and statuses on
    ///   BOTH sides that it must DROP"~~ — true only for the `Eq` leg (403 AND
    ///   405); a `Ge`/`Le` predicate cannot drop on both sides, so the
    ///   sentence was unsatisfiable for two of the three legs (M70-R7). The
    ///   uniqueness conclusion it argued for holds anyway, as restated above.
    /// - ~~"`filter.rs` pins that each `FilterOp` evaluates correctly and the
    ///   envoy-config tests pin that `op: EQ` parses; this is what connects
    ///   the two"~~ — the second half is FALSE (REVIEW.md §8.3, I-2): no test
    ///   anywhere parsed `op: EQ` or `op: LE`, and this table drives Rust
    ///   `ComparisonOp` literals that never cross the serde boundary. The
    ///   YAML-token → `ComparisonOp` mapping is pinned by
    ///   `yaml_op_token_compiles_to_matching_filter_op` below, which drives
    ///   all three tokens through the real `parse_bootstrap` path.
    ///   (`filter.rs` does still pin that each `FilterOp` evaluates
    ///   correctly.)
    #[tokio::test]
    async fn from_config_compiles_status_code_filter_into_sink() {
        use tempfile::tempdir;

        /// One table row: the configured operator, its threshold, and the
        /// statuses the compiled predicate must keep (`true`) or drop (`false`).
        type OpCase<'a> = (envoy_config::ComparisonOp, u32, &'a [(u16, bool)]);

        let cases: &[OpCase] = &[
            (
                envoy_config::ComparisonOp::Ge,
                500,
                &[(499, false), (500, true), (503, true)],
            ),
            (
                envoy_config::ComparisonOp::Eq,
                404,
                &[(403, false), (404, true), (405, false)],
            ),
            (
                envoy_config::ComparisonOp::Le,
                200,
                &[(100, true), (200, true), (201, false)],
            ),
        ];

        let dir = tempdir().expect("tempdir");
        for (i, (op, threshold, expectations)) in cases.iter().enumerate() {
            let path = dir.path().join(format!("access_{i}.log"));
            let config =
                hcm_config_with_filtered_access_log(Some((*op, *threshold)), &path, 200).await;
            let sink = &config.access_log[0];
            for (status, must_log) in *expectations {
                assert_eq!(
                    sink.should_log(*status, "-", &[], &Default::default()),
                    *must_log,
                    "{op:?} {threshold} filter on status {status}: expected should_log={must_log}",
                );
            }
        }
    }

    /// Phase 71 Task 5: build an HCM whose single file sink carries a
    /// `response_flag_filter` with the given `flags`. Mirrors
    /// `hcm_config_with_filtered_access_log` but for the second oneof arm.
    async fn hcm_config_with_response_flag_access_log(
        flags: &[&str],
        path: &std::path::Path,
    ) -> Arc<HCMConfig> {
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = cluster_mgr_empty().await;
        let envoy_cfg = envoy_config::HttpConnectionManagerConfig {
            stat_prefix: "ingress_http".to_string(),
            codec_type: envoy_config::CodecType::HTTP1,
            http2_protocol_options: None,
            access_log: vec![envoy_config::AccessLog {
                name: "envoy.access_loggers.file".to_string(),
                typed_config: envoy_config::AccessLogTypedConfig::FileAccessLog(
                    envoy_config::FileAccessLog {
                        path: path.to_string_lossy().into_owned(),
                        log_format: None,
                    },
                ),
                filter: Some(envoy_config::AccessLogFilter {
                    status_code_filter: None,
                    response_flag_filter: Some(envoy_config::ResponseFlagFilter {
                        flags: flags.iter().map(|s| s.to_string()).collect(),
                    }),
                    header_filter: None,
                    and_filter: None,
                    or_filter: None,
                    metadata_filter: None,
                }),
            }],
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    // A FIXED-path route (not a catch-all) so a request to any
                    // other path is a no-route synth 404 whose record carries
                    // `%RESPONSE_FLAGS%` = `NR` — lets the end-to-end emit-loop
                    // test below drive both a flagged and a flagless record.
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: None,
                            path: Some("/routed".to_string()),
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("body\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            }),
            rds: None,
            http_filters: vec![envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            }],
        };
        Arc::new(
            HCMConfig::from_config(
                &envoy_cfg,
                cluster_mgr,
                registry,
                None,
                Arc::new(RuntimeSnapshot::default()),
            )
            .await
            .expect("HCMConfig builds"),
        )
    }

    /// Phase 71 Task 5 (CF-70-1): `from_config` compiles a `response_flag_filter`
    /// into the built `FileSink`'s runtime predicate via the 2-arm match — the
    /// zero-arm `expect()` is gone. `flags: ["NR"]` keeps the no-route 404 `NR`
    /// record, drops a clean 503 `-`, and drops a non-matching `UH`.
    #[tokio::test]
    async fn from_config_compiles_response_flag_filter_into_sink() {
        use tempfile::tempdir;
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("rf.log");
        let config = hcm_config_with_response_flag_access_log(&["NR"], &path).await;
        let sink = &config.access_log[0];
        assert!(sink.should_log(404, "NR", &[], &Default::default())); // kept
        assert!(!sink.should_log(503, "-", &[], &Default::default())); // dropped (no flag)
        assert!(!sink.should_log(200, "UH", &[], &Default::default())); // dropped (UH ∉ ["NR"])
    }

    /// Phase 72 T5: `compile_access_log_filter` builds the `header_filter` arm
    /// into `LogFilter::Header`, and the runtime gate keeps a matching request
    /// header, drops a present-mismatch AND an absent one.
    #[test]
    fn compile_access_log_filter_builds_header_arm() {
        let filter = envoy_config::AccessLogFilter {
            status_code_filter: None,
            response_flag_filter: None,
            header_filter: Some(envoy_config::HeaderFilter {
                header: envoy_config::HeaderMatcher {
                    name: "x-log".into(),
                    mode: envoy_config::HeaderMatcherMode::ExactMatch("yes".into()),
                    invert_match: false,
                },
            }),
            and_filter: None,
            or_filter: None,
            metadata_filter: None,
        };
        let compiled = compile_access_log_filter(&filter);
        assert!(matches!(
            compiled,
            envoy_accesslog::LogFilter::Header { .. }
        ));
        assert!(compiled.should_log(
            200,
            "-",
            &[("x-log".into(), "yes".into())],
            &Default::default()
        ));
        assert!(!compiled.should_log(
            200,
            "-",
            &[("x-log".into(), "no".into())],
            &Default::default()
        ));
        assert!(!compiled.should_log(200, "-", &[], &Default::default())); // absent → drop
    }

    /// Phase 73 T4: `compile_access_log_filter` builds the `and_filter`/`or_filter`
    /// arms recursively. The and-fixture (0079) keeps only the both-match probe;
    /// the depth-2 or-fixture (0080) keeps the AND-child-true and the leaf-true
    /// probes and drops the rest.
    #[test]
    fn compile_access_log_filter_builds_composition_arms_recursively() {
        let hdr = |name: &str, val: &str| envoy_config::AccessLogFilter {
            status_code_filter: None,
            response_flag_filter: None,
            header_filter: Some(envoy_config::HeaderFilter {
                header: envoy_config::HeaderMatcher {
                    name: name.into(),
                    mode: envoy_config::HeaderMatcherMode::ExactMatch(val.into()),
                    invert_match: false,
                },
            }),
            and_filter: None,
            or_filter: None,
            metadata_filter: None,
        };

        // and_filter { [x-a=1, x-b=1] } → LogFilter::And([Header, Header]).
        let and = envoy_config::AccessLogFilter {
            status_code_filter: None,
            response_flag_filter: None,
            header_filter: None,
            and_filter: Some(envoy_config::AndFilter {
                filters: vec![hdr("x-a", "1"), hdr("x-b", "1")],
            }),
            or_filter: None,
            metadata_filter: None,
        };
        let compiled = compile_access_log_filter(&and);
        assert!(matches!(compiled, envoy_accesslog::LogFilter::And(ref v) if v.len() == 2));
        let a = [("x-a".to_string(), "1".to_string())];
        let ab = [
            ("x-a".to_string(), "1".to_string()),
            ("x-b".to_string(), "1".to_string()),
        ];
        assert!(!compiled.should_log(200, "-", &a, &Default::default())); // only x-a → AND false → drop
        assert!(compiled.should_log(200, "-", &ab, &Default::default())); // both → AND true → keep

        // or_filter { [ and_filter{[x-a,x-b]}, header{x-c} ] } (depth-2).
        let or = envoy_config::AccessLogFilter {
            status_code_filter: None,
            response_flag_filter: None,
            header_filter: None,
            and_filter: None,
            or_filter: Some(envoy_config::OrFilter {
                filters: vec![
                    envoy_config::AccessLogFilter {
                        status_code_filter: None,
                        response_flag_filter: None,
                        header_filter: None,
                        and_filter: Some(envoy_config::AndFilter {
                            filters: vec![hdr("x-a", "1"), hdr("x-b", "1")],
                        }),
                        or_filter: None,
                        metadata_filter: None,
                    },
                    hdr("x-c", "1"),
                ],
            }),
            metadata_filter: None,
        };
        let compiled = compile_access_log_filter(&or);
        assert!(matches!(compiled, envoy_accesslog::LogFilter::Or(ref v) if v.len() == 2));
        let c = [("x-c".to_string(), "1".to_string())];
        assert!(compiled.should_log(200, "-", &ab, &Default::default())); // AND-child true → OR keep
        assert!(compiled.should_log(200, "-", &c, &Default::default())); // leaf true → OR keep
        assert!(!compiled.should_log(200, "-", &a, &Default::default())); // AND-child false, leaf false → drop
    }

    /// Phase 74 T6: `compile_access_log_filter` builds the `metadata_filter`
    /// arm — boxing the config `MetadataMatcher` into the injected
    /// `MetadataMatch` seam and resolving the BoolValue-wrapper default
    /// (`match_if_key_not_found: None` → `true`, MEASURED SPEC §0 R-0.4).
    #[test]
    fn compile_access_log_filter_builds_metadata_arm_with_wrapper_default() {
        use std::collections::BTreeMap;

        let md = |ns: &str, k: &str, v: &str| {
            let mut m: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
            m.entry(ns.to_string())
                .or_default()
                .insert(k.to_string(), v.to_string());
            m
        };
        let empty: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

        let matcher = envoy_config::MetadataMatcher {
            filter: "com.example".into(),
            path: vec![envoy_config::MetadataPathSegment { key: "k".into() }],
            value: envoy_config::ValueMatcher::StringMatch(envoy_config::StringMatcher {
                mode: envoy_config::StringMatcherMode::Exact("1".into()),
                ignore_case: false,
            }),
        };

        // (a) `match_if_key_not_found` ABSENT → compiled to `true` (the MEASURED
        //     BoolValue-wrapper default). Key-absent records are KEPT.
        let default_cfg = envoy_config::AccessLogFilter {
            metadata_filter: Some(envoy_config::MetadataFilter {
                matcher: Some(matcher.clone()),
                match_if_key_not_found: None,
            }),
            ..Default::default()
        };
        let compiled = compile_access_log_filter(&default_cfg);
        assert!(matches!(
            compiled,
            envoy_accesslog::LogFilter::Metadata {
                matcher: Some(_),
                match_if_key_not_found: true
            }
        ));
        assert!(compiled.should_log(200, "-", &[], &md("com.example", "k", "1"))); // match
        assert!(!compiled.should_log(200, "-", &[], &md("com.example", "k", "2"))); // mismatch
        assert!(compiled.should_log(200, "-", &[], &empty)); // absent → default true

        // (b) explicit `false` → key-absent records are DROPPED (the R-0.4
        //     polarity flip that `--mode validate` cannot reach).
        let explicit_false = envoy_config::AccessLogFilter {
            metadata_filter: Some(envoy_config::MetadataFilter {
                matcher: Some(matcher),
                match_if_key_not_found: Some(false),
            }),
            ..Default::default()
        };
        let compiled = compile_access_log_filter(&explicit_false);
        assert!(matches!(
            compiled,
            envoy_accesslog::LogFilter::Metadata {
                match_if_key_not_found: false,
                ..
            }
        ));
        assert!(compiled.should_log(200, "-", &[], &md("com.example", "k", "1")));
        assert!(!compiled.should_log(200, "-", &[], &empty)); // absent → drop

        // (c) MATCHER-LESS (upstream accepts `metadata_filter: {}`, R-0.2) →
        //     `matcher: None`, every record takes the not-found policy.
        let matcher_less = envoy_config::AccessLogFilter {
            metadata_filter: Some(envoy_config::MetadataFilter::default()),
            ..Default::default()
        };
        let compiled = compile_access_log_filter(&matcher_less);
        assert!(matches!(
            compiled,
            envoy_accesslog::LogFilter::Metadata {
                matcher: None,
                match_if_key_not_found: true
            }
        ));
        assert!(compiled.should_log(200, "-", &[], &empty));

        // (d) nested inside a composition arm (phase-73 recursion).
        let nested = envoy_config::AccessLogFilter {
            or_filter: Some(envoy_config::OrFilter {
                filters: vec![
                    envoy_config::AccessLogFilter {
                        metadata_filter: Some(envoy_config::MetadataFilter {
                            matcher: None,
                            match_if_key_not_found: Some(false),
                        }),
                        ..Default::default()
                    },
                    envoy_config::AccessLogFilter {
                        metadata_filter: Some(envoy_config::MetadataFilter::default()),
                        ..Default::default()
                    },
                ],
            }),
            ..Default::default()
        };
        let compiled = compile_access_log_filter(&nested);
        assert!(matches!(compiled, envoy_accesslog::LogFilter::Or(ref v) if v.len() == 2));
        assert!(compiled.should_log(200, "-", &[], &empty)); // second child keeps
    }

    /// Phase 72 T9 (SPEC §2.1 item 5): `header_filter` membership across the
    /// supported modes + the absent-drop, end-to-end through
    /// `compile_access_log_filter` → `LogFilter::Header::should_log`. SafeRegex
    /// membership is covered on the shared engine in `envoy-config::matcher`
    /// tests; the access-log path reuses it verbatim (proven by
    /// `header_match_trait_delegates_to_inherent_engine`).
    #[test]
    fn header_filter_membership_across_modes_and_absent_drop() {
        use envoy_config::HeaderMatcherMode as M;
        let compile_mode = |mode: M| {
            compile_access_log_filter(&envoy_config::AccessLogFilter {
                status_code_filter: None,
                response_flag_filter: None,
                header_filter: Some(envoy_config::HeaderFilter {
                    header: envoy_config::HeaderMatcher {
                        name: "x-log".into(),
                        mode,
                        invert_match: false,
                    },
                }),
                and_filter: None,
                or_filter: None,
                metadata_filter: None,
            })
        };
        let yes = [("x-log".to_string(), "yes".to_string())];
        let no = [("x-log".to_string(), "no".to_string())];
        let absent: [(String, String); 0] = [];

        // exact: keep "yes"; drop mismatch AND absent.
        let f = compile_mode(M::ExactMatch("yes".into()));
        assert!(f.should_log(200, "-", &yes, &Default::default()));
        assert!(!f.should_log(200, "-", &no, &Default::default()));
        assert!(!f.should_log(200, "-", &absent, &Default::default()));

        // prefix / suffix match on the value; drop absent.
        assert!(compile_mode(M::PrefixMatch("ye".into())).should_log(
            200,
            "-",
            &yes,
            &Default::default()
        ));
        assert!(!compile_mode(M::PrefixMatch("ye".into())).should_log(
            200,
            "-",
            &absent,
            &Default::default()
        ));
        assert!(compile_mode(M::SuffixMatch("es".into())).should_log(
            200,
            "-",
            &yes,
            &Default::default()
        ));

        // present: any value keeps; absent drops.
        assert!(compile_mode(M::PresentMatch(true)).should_log(
            200,
            "-",
            &yes,
            &Default::default()
        ));
        assert!(!compile_mode(M::PresentMatch(true)).should_log(
            200,
            "-",
            &absent,
            &Default::default()
        ));

        // string_match { exact } — the fixture-0078 mode.
        let sm = envoy_config::StringMatcher {
            mode: envoy_config::StringMatcherMode::Exact("yes".into()),
            ignore_case: false,
        };
        let f = compile_mode(M::StringMatch(sm));
        assert!(f.should_log(200, "-", &yes, &Default::default()));
        assert!(!f.should_log(200, "-", &no, &Default::default()));
        assert!(!f.should_log(200, "-", &absent, &Default::default()));
    }

    /// Phase-71 state-5 review probe (REVIEW.md §2): the H1 EMIT LOOP threads
    /// the record's REAL `response_flags` token into the widened `should_log`
    /// gate. A mutation measurement showed every prior in-process H1 test stays
    /// green when the gate passes a placeholder `"-"` instead of
    /// `&record.response_flags` (only differential fixture 0077 caught it) —
    /// this test closes that hole in-process: against a `flags: ["NR"]` sink,
    /// a no-route request (404, `NR`) is WRITTEN by the emit loop and a routed
    /// clean 200 (`-`) is NOT. Mirrors the H2 sibling
    /// `h2_response_flag_filter_suppresses_no_flag`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_response_flag_sink_gates_emit_loop_end_to_end() {
        use tempfile::tempdir;
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("rf_e2e.log");
        let config = hcm_config_with_response_flag_access_log(&["NR"], &path).await;

        // A routed clean 200 renders `-` → dropped by ["NR"].
        let _ = drive(
            config.clone(),
            b"GET /routed HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        )
        .await;
        // A no-route 404 renders `NR` → kept.
        let _ = drive(
            config,
            b"GET /nowhere HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let contents = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "flags:[NR] must keep ONLY the no-route 404 record; got {lines:?}"
        );
        assert!(
            lines[0].contains("404"),
            "the single kept line must be the 404 NR record; got {lines:?}"
        );
    }

    /// Phase 71 Task 9: a sink built from a filterless `AccessLog` logs EVERY
    /// record regardless of status/flags — the regression that carries the 28
    /// byte-exact fixtures under the widened `should_log(status, flags)`.
    #[tokio::test]
    async fn no_filter_sink_logs_every_record_after_widening() {
        use tempfile::tempdir;
        let dir = tempdir().expect("tempdir");
        let config =
            hcm_config_with_filtered_access_log(None, &dir.path().join("plain.log"), 200).await;
        let sink = &config.access_log[0];
        assert!(sink.should_log(200, "-", &[], &Default::default()));
        assert!(sink.should_log(503, "NR", &[], &Default::default()));
        assert!(sink.should_log(404, "UF", &[], &Default::default()));
    }

    /// Phase 72 §5.2 state-3 (REVIEW.md F-3, closes M71-5): TWO sinks with
    /// DIFFERENT filter arms on one HCM. Sink A gates on
    /// `header_filter { exact "yes" on x-log }`; sink B gates on
    /// `status_code_filter { EQ 200 }`. Three `GET /x` requests (`x-log: yes`,
    /// `x-log: no`, no header) all return the direct-response 200. The state-5
    /// LIVE-PROBE MEASURED byte-exact parity vs. `envoyproxy/envoy:v1.33.0` for
    /// this exact shape (REVIEW.md Probe 1). This pins that the emit loop's
    /// per-sink gate is INDEPENDENT — sink A keeps only the 1 matching request,
    /// sink B keeps all 3 — with no cross-sink leakage of the `req.headers`
    /// slice.
    #[tokio::test(flavor = "multi_thread")]
    async fn two_sinks_with_mixed_filters_gate_independently() {
        use tempfile::tempdir;
        let dir = tempdir().expect("tempdir");
        let path_a = dir.path().join("sink_a_header.log");
        let path_b = dir.path().join("sink_b_status.log");

        let file_sink_cfg = |path: &std::path::Path, filter: envoy_config::AccessLogFilter| {
            envoy_config::AccessLog {
                name: "envoy.access_loggers.file".to_string(),
                typed_config: envoy_config::AccessLogTypedConfig::FileAccessLog(
                    envoy_config::FileAccessLog {
                        path: path.to_string_lossy().into_owned(),
                        log_format: None,
                    },
                ),
                filter: Some(filter),
            }
        };
        let envoy_cfg = envoy_config::HttpConnectionManagerConfig {
            stat_prefix: "ingress_http".to_string(),
            codec_type: envoy_config::CodecType::HTTP1,
            http2_protocol_options: None,
            access_log: vec![
                // Sink A — header_filter { exact "yes" on x-log }.
                file_sink_cfg(
                    &path_a,
                    envoy_config::AccessLogFilter {
                        status_code_filter: None,
                        response_flag_filter: None,
                        header_filter: Some(envoy_config::HeaderFilter {
                            header: envoy_config::HeaderMatcher {
                                name: "x-log".to_string(),
                                mode: envoy_config::HeaderMatcherMode::ExactMatch(
                                    "yes".to_string(),
                                ),
                                invert_match: false,
                            },
                        }),
                        and_filter: None,
                        or_filter: None,
                        metadata_filter: None,
                    },
                ),
                // Sink B — status_code_filter { EQ 200 }.
                file_sink_cfg(
                    &path_b,
                    envoy_config::AccessLogFilter {
                        status_code_filter: Some(envoy_config::StatusCodeFilter {
                            comparison: envoy_config::ComparisonFilter {
                                op: envoy_config::ComparisonOp::Eq,
                                value: envoy_config::RuntimeUInt32 {
                                    default_value: 200,
                                    runtime_key: "access_log.status_code".to_string(),
                                },
                            },
                        }),
                        response_flag_filter: None,
                        header_filter: None,
                        and_filter: None,
                        or_filter: None,
                        metadata_filter: None,
                    },
                ),
            ],
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: None,
                            path: Some("/x".to_string()),
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("hi\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            }),
            rds: None,
            http_filters: vec![envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            }],
        };
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = cluster_mgr_empty().await;
        let config = Arc::new(
            HCMConfig::from_config(
                &envoy_cfg,
                cluster_mgr,
                registry,
                None,
                Arc::new(RuntimeSnapshot::default()),
            )
            .await
            .expect("HCMConfig builds"),
        );

        // Three requests, all → 200. Only the first carries the matching header.
        let _ = drive(
            config.clone(),
            b"GET /x HTTP/1.1\r\nHost: x\r\nx-log: yes\r\nConnection: close\r\n\r\n",
        )
        .await;
        let _ = drive(
            config.clone(),
            b"GET /x HTTP/1.1\r\nHost: x\r\nx-log: no\r\nConnection: close\r\n\r\n",
        )
        .await;
        let _ = drive(
            config,
            b"GET /x HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let a = tokio::fs::read_to_string(&path_a).await.unwrap_or_default();
        let b = tokio::fs::read_to_string(&path_b).await.unwrap_or_default();
        assert_eq!(
            a.lines().count(),
            1,
            "sink A (header_filter) keeps ONLY the x-log:yes request: {a:?}"
        );
        assert_eq!(
            b.lines().count(),
            3,
            "sink B (status EQ 200) keeps all three 200s: {b:?}"
        );
    }

    /// Phase 71 Task 9: the phase-70 `status_code_filter` still gates PURELY on
    /// status, ignoring the newly-threaded `response_flags` arg — a GE-500 sink
    /// drops a 200 whatever its flag, keeps a 503 whatever its flag.
    #[tokio::test]
    async fn status_code_filter_unchanged_under_widening() {
        use tempfile::tempdir;
        let dir = tempdir().expect("tempdir");
        let config = hcm_config_with_filtered_access_log(
            Some((envoy_config::ComparisonOp::Ge, 500)),
            &dir.path().join("sc.log"),
            200,
        )
        .await;
        let sink = &config.access_log[0];
        assert!(!sink.should_log(200, "NR", &[], &Default::default())); // status-only: 200 < 500 drops
        assert!(!sink.should_log(200, "-", &[], &Default::default()));
        assert!(sink.should_log(503, "-", &[], &Default::default())); // 503 >= 500 keeps
        assert!(sink.should_log(503, "NR", &[], &Default::default()));
    }

    /// Phase 70 Task 7: the H1 emit loop gates each sink on `should_log` of the
    /// record's final response code — a GE-500 sink drops a 200 (0 lines) and
    /// keeps a 503 (1 line). The third leg pins the regression parity that
    /// carries the 27 pre-existing access-log differential fixtures: a sink
    /// with NO filter still logs every record.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_filtered_sink_suppresses_below_threshold() {
        use tempfile::tempdir;

        /// Drive one request through an HCM whose single sink is built from
        /// `filter`, and return the sink file's line count.
        async fn lines_after_one_request(
            filter: Option<(envoy_config::ComparisonOp, u32)>,
            path: &std::path::Path,
            status: u16,
        ) -> usize {
            let config = hcm_config_with_filtered_access_log(filter, path, status).await;
            let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
            let _ = drive(config, req).await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            tokio::fs::read_to_string(path)
                .await
                .unwrap_or_default()
                .lines()
                .count()
        }

        let dir = tempdir().expect("tempdir");
        let ge = envoy_config::ComparisonOp::Ge;

        let suppressed = dir.path().join("filtered_200.log");
        assert_eq!(
            lines_after_one_request(Some((ge, 500)), &suppressed, 200).await,
            0,
            "GE 500 sink must suppress a 200 record"
        );

        let emitted = dir.path().join("filtered_503.log");
        assert_eq!(
            lines_after_one_request(Some((ge, 500)), &emitted, 503).await,
            1,
            "GE 500 sink must emit a 503 record"
        );

        let unfiltered = dir.path().join("unfiltered_200.log");
        assert_eq!(
            lines_after_one_request(None, &unfiltered, 200).await,
            1,
            "a sink with no filter logs every record (pre-phase-70 parity)"
        );
    }

    /// Phase 70 Task 7: `access_logs_total` counts EMITTED records only — a
    /// filter-suppressed sink must not tick it. The counter lives INSIDE the
    /// gated branch (it was a pre-loop `add(access_log.len())`); this pins that
    /// placement, which the line-count assertions above cannot see.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_filtered_sink_suppresses_access_logs_total() {
        use tempfile::tempdir;

        /// Drive one request through an HCM whose single GE-500 sink is at
        /// `path`, and return `access_logs_total` after the emit loop.
        async fn total_after_one_request(path: &std::path::Path, status: u16) -> u64 {
            let config = hcm_config_with_filtered_access_log(
                Some((envoy_config::ComparisonOp::Ge, 500)),
                path,
                status,
            )
            .await;
            let total = Arc::clone(&config.stats.access_logs_total);
            let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
            let _ = drive(config, req).await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            total.value()
        }

        let dir = tempdir().expect("tempdir");

        // The suppressed leg: 200 < 500 -> the sink emits nothing, so the
        // counter must stay at 0 (a pre-loop add(len) would read 1 here).
        let suppressed = dir.path().join("filtered_200.log");
        assert_eq!(
            total_after_one_request(&suppressed, 200).await,
            0,
            "a suppressed sink must not tick access_logs_total"
        );

        // The emitted leg: 503 >= 500 -> exactly one tick.
        let emitted = dir.path().join("filtered_503.log");
        assert_eq!(
            total_after_one_request(&emitted, 503).await,
            1,
            "the emitted record ticks access_logs_total"
        );
    }

    /// Phase 70 Task 11: parse `yaml` through the production
    /// `envoy_config::parse_bootstrap` path and return the compiled
    /// `LogFilter` of the first listener's HCM's first access-log entry —
    /// `None` when that entry carries no `filter`. Mirrors the production
    /// `entry.filter.as_ref().map(compile_access_log_filter)` line in
    /// `from_config`, so the whole config → runtime translation (serde shape,
    /// validators, compiler) is the thing under test rather than a struct
    /// literal.
    fn compiled_filter_from_bootstrap_yaml(yaml: &str) -> Option<envoy_accesslog::LogFilter> {
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("bootstrap parses");
        let terminal = &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0];
        let Some(envoy_config::TypedConfig::HttpConnectionManager(hcm_cfg)) =
            terminal.typed_config.as_ref()
        else {
            panic!("first network filter is not an HCM");
        };
        hcm_cfg.access_log[0]
            .filter
            .as_ref()
            .map(compile_access_log_filter)
    }

    /// The second §5.2 re-entry (REVIEW.md §8.3, I-2): a bootstrap whose single
    /// HCM access-log entry carries a `status_code_filter` with the given YAML
    /// `op` TOKEN, `default_value`, and `runtime_key`. The token is spliced
    /// into the YAML verbatim, so callers can drive all three upstream tokens
    /// (`EQ`/`GE`/`LE`) through the real serde path — the seam I-1's table
    /// test cannot reach, because it constructs `ComparisonOp` as a Rust
    /// literal.
    fn bootstrap_yaml_with_filter(op_token: &str, default_value: u32, runtime_key: &str) -> String {
        format!(
            r#"
node: {{ id: t11, cluster: t11 }}
static_resources:
  listeners:
    - name: l1
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/t11-access.log
                    filter:
                      status_code_filter:
                        comparison:
                          op: {op_token}
                          value: {{ default_value: {default_value}, runtime_key: {runtime_key} }}
                route_config:
                  name: r
                  virtual_hosts:
                    - name: v
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: "b\n" }} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
        )
    }

    /// Phase 70 Task 11: a bootstrap whose single HCM access-log entry carries
    /// a `GE 500` `status_code_filter` with the given `runtime_key`. Only the
    /// `runtime_key` varies across callers. (Since the second §5.2 re-entry a
    /// thin wrapper over `bootstrap_yaml_with_filter` — byte-identical output.)
    fn bootstrap_yaml_with_runtime_key(runtime_key: &str) -> String {
        bootstrap_yaml_with_filter("GE", 500, runtime_key)
    }

    /// Phase 70 Task 11: `runtime_key` is RTDS-INERT. Upstream Envoy would
    /// consult the runtime layer under this key for a `default_value`
    /// override; envoy-rust has no runtime subsystem, so the comparison ALWAYS
    /// uses `default_value`. Two bootstraps differing ONLY in `runtime_key`
    /// must therefore compile to filters with identical `should_log` outcomes
    /// across the status classes the phase cares about. The key is still
    /// REQUIRED non-empty (upstream PGV `min_len 1`) — that is a load-parity
    /// constraint, pinned separately by the envoy-config validator tests.
    #[test]
    fn runtime_key_is_rtds_inert() {
        let inert = compiled_filter_from_bootstrap_yaml(&bootstrap_yaml_with_runtime_key("unused"))
            .expect("filter compiles");
        let named =
            compiled_filter_from_bootstrap_yaml(&bootstrap_yaml_with_runtime_key("some.key"))
                .expect("filter compiles");

        for status in [200u16, 499, 500, 503] {
            assert_eq!(
                inert.should_log(status, "-", &[], &Default::default()),
                named.should_log(status, "-", &[], &Default::default()),
                "runtime_key must not alter should_log({status}): \
                 runtime_key=unused -> {}, runtime_key=some.key -> {}",
                inert.should_log(status, "-", &[], &Default::default()),
                named.should_log(status, "-", &[], &Default::default()),
            );
        }

        // The compiled filters are structurally identical too — the compiler
        // carries `default_value` through as the threshold and drops the key
        // entirely (it has nowhere to go: `StatusCodeComparison` has no
        // runtime-key field). Phase 72 dropped `LogFilter: PartialEq` (the
        // `Header` arm holds a trait object — ADR-0150), so compare the inner
        // `StatusCodeComparison` (still `PartialEq`) after asserting both arms.
        let (
            envoy_accesslog::LogFilter::StatusCode(inert_cmp),
            envoy_accesslog::LogFilter::StatusCode(named_cmp),
        ) = (&inert, &named)
        else {
            panic!("both runtime_key variants must compile to StatusCode filters");
        };
        assert_eq!(
            inert_cmp, named_cmp,
            "runtime_key must not survive compilation into the runtime filter"
        );

        // Sanity: the shared `GE 500` threshold really is the one in effect,
        // so the equality above is not two identically-vacuous filters.
        assert!(
            !inert.should_log(499, "-", &[], &Default::default()),
            "GE 500 must reject a 499"
        );
        assert!(
            inert.should_log(500, "-", &[], &Default::default()),
            "GE 500 must accept a 500"
        );
    }

    /// Phase 70 Task 11: regression parity for the 29 pre-phase-70 access-log
    /// differential fixtures — an `AccessLog` with NO `filter` compiles to
    /// `None`, and a `None`-filtered sink logs EVERY record. Both legs are
    /// pinned: the config → `None` compile step, and the sink's unconditional
    /// `should_log` on a sink built by the production `from_config`.
    #[tokio::test]
    async fn no_filter_logs_every_record() {
        use tempfile::tempdir;

        // Leg 1: a filterless access-log entry compiles to `None`.
        let yaml = bootstrap_yaml_with_runtime_key("unused").replace(
            r#"                    filter:
                      status_code_filter:
                        comparison:
                          op: GE
                          value: { default_value: 500, runtime_key: unused }
"#,
            "",
        );
        assert!(
            !yaml.contains("filter:"),
            "the filter block must be stripped from the YAML under test"
        );
        assert!(
            compiled_filter_from_bootstrap_yaml(&yaml).is_none(),
            "an access_log with no `filter` must compile to None"
        );

        // Leg 2: the sink `from_config` builds from that shape logs every
        // record, whatever the final response code.
        let dir = tempdir().expect("tempdir");
        let config =
            hcm_config_with_filtered_access_log(None, &dir.path().join("unfiltered.log"), 200)
                .await;
        let sink = &config.access_log[0];
        for status in [200u16, 499, 500, 503] {
            assert!(
                sink.should_log(status, "-", &[], &Default::default()),
                "a sink with no filter must log every record; should_log({status}) was false"
            );
        }
    }

    /// The second §5.2 re-entry (REVIEW.md §8.3, I-2): pin the YAML-token →
    /// `ComparisonOp` serde mapping — the `#[serde(rename)]` attributes on
    /// `envoy_config::ComparisonOp` — for ALL THREE shipped tokens, by driving
    /// production YAML through the real `parse_bootstrap` → validators →
    /// `compile_access_log_filter` path. The Task-6 table test above drives
    /// Rust `ComparisonOp` literals that never cross the serde boundary, so it
    /// cannot see a swapped rename: swapping the `EQ`/`LE` renames (variant
    /// names untouched) left the whole suite green at 886/0 while `op: EQ 404`
    /// silently logged a 403 (measured, REVIEW.md §8.3). This test goes RED
    /// under exactly that mutation.
    ///
    /// Each row is uniquely satisfied by its own operator: the `EQ` leg drops
    /// on both sides (403 AND 405), and the `LE` leg's `(100, true)` probe is
    /// load-bearing — a naive `(200, true), (201, false)` table is also
    /// satisfied by `Eq 200` and stays green under the rename swap (measured,
    /// REVIEW.md §8.1).
    #[test]
    fn yaml_op_token_compiles_to_matching_filter_op() {
        /// One table row: the YAML `op` token, its threshold, and the statuses
        /// the compiled predicate must keep (`true`) or drop (`false`).
        type TokenCase<'a> = (&'a str, u32, &'a [(u16, bool)]);

        let cases: &[TokenCase] = &[
            ("EQ", 404, &[(403, false), (404, true), (405, false)]),
            ("GE", 500, &[(499, false), (500, true), (503, true)]),
            ("LE", 200, &[(100, true), (200, true), (201, false)]),
        ];

        for (token, threshold, expectations) in cases {
            let filter = compiled_filter_from_bootstrap_yaml(&bootstrap_yaml_with_filter(
                token, *threshold, "unused",
            ))
            .expect("filter compiles");
            for (status, must_log) in *expectations {
                assert_eq!(
                    filter.should_log(*status, "-", &[], &Default::default()),
                    *must_log,
                    "op: {token} {threshold} on status {status}: expected should_log={must_log} \
                     (the YAML token compiled to the wrong FilterOp)",
                );
            }
        }
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
    /// the 4 proxy-arm variants (no-endpoint-503, connect-fail-503, send-fail-503,
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
            envoy_accesslog::FileSink::new(
                path.clone(),
                envoy_accesslog::CompiledFormat::default(),
                None,
            )
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

    /// phase 41 (ADR-0098 §C): the H1 HCM sets `route_name` on the access-log
    /// record from the matched route's config `name`. A NAMED route → the name
    /// renders via `%ROUTE_NAME%`; an UNNAMED route → the `-` sentinel.
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_h1_sets_route_name_from_matched_route() {
        use std::path::PathBuf;
        use tempfile::tempdir;

        async fn run_with_route_name(route_name: &str) -> String {
            let dir = tempdir().expect("tempdir");
            let path: PathBuf = dir.path().join("access.log");
            let sink = Arc::new(
                envoy_accesslog::FileSink::new(
                    path.clone(),
                    envoy_accesslog::CompiledFormat::from_inline("r=%ROUTE_NAME%")
                        .expect("format parses"),
                    None,
                )
                .await
                .expect("open sink"),
            );
            let config = Arc::new(HCMConfig {
                stat_prefix: "ingress_http".to_string(),
                cluster_mgr: cluster_mgr_empty().await,
                http2_protocol_options: None,
                stats: mk_stats("ingress_http"),
                access_log: vec![sink],
                filter_pipeline: test_router_only_pipeline(),
                pool_mgr: None,
                route_config: RwLock::new(Arc::new(RouteConfiguration {
                    name: "local_route".to_string(),
                    validate_clusters: None,
                    virtual_hosts: vec![VirtualHost {
                        name: "default".to_string(),
                        domains: vec!["*".to_string()],
                        include_attempt_count_in_response: false,
                        routes: vec![Route {
                            name: route_name.to_string(),
                            r#match: RouteMatch {
                                prefix: Some("/".to_string()),
                                path: None,
                                headers: vec![],
                                runtime_fraction: None,
                            },
                            action: RouteAction::DirectResponse(DirectResponse {
                                status: 200,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("ok\n".to_string()),
                                },
                            }),
                            typed_per_filter_config: Default::default(),
                        }],
                    }],
                })),
                runtime: Arc::new(RuntimeSnapshot::default()),
            });
            let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
            let _ = drive(config, req).await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            tokio::fs::read_to_string(&path).await.unwrap_or_default()
        }

        let named = run_with_route_name("myroute").await;
        assert!(
            named.contains("r=myroute"),
            "NAMED route → %ROUTE_NAME% renders the name; got: {}",
            named.trim()
        );

        let unnamed = run_with_route_name("").await;
        assert!(
            unnamed.contains("r=-"),
            "UNNAMED route → %ROUTE_NAME% renders the `-` sentinel; got: {}",
            unnamed.trim()
        );
    }

    /// phase 42 (ADR-0099): the H1 HCM sets `response_code_details` on the
    /// access-log record per response-path. A `direct_response` route →
    /// `%RESPONSE_CODE_DETAILS%` renders `direct_response`.
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_h1_sets_response_code_details_from_response_path() {
        use std::path::PathBuf;
        use tempfile::tempdir;

        let dir = tempdir().expect("tempdir");
        let path: PathBuf = dir.path().join("access.log");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(
                path.clone(),
                envoy_accesslog::CompiledFormat::from_inline("d=%RESPONSE_CODE_DETAILS%")
                    .expect("format parses"),
                None,
            )
            .await
            .expect("open sink"),
        );
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: "dr".to_string(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let _ = drive(config, req).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let contents = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        assert!(
            contents.contains("d=direct_response"),
            "direct_response route → %RESPONSE_CODE_DETAILS% renders `direct_response`; got: {}",
            contents.trim()
        );
    }

    /// phase 43 Task 4: a request routed to a cluster → the access-log
    /// record's `%UPSTREAM_CLUSTER%` renders the matched cluster's name.
    /// Set at the proxy-ARM ENTRY (not gated on upstream success), mirroring
    /// Envoy: the route resolves to `backend` so the operator renders
    /// regardless of the upstream attempt's outcome. Real routed test: a
    /// live in-process upstream backs the cluster.
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_h1_sets_upstream_cluster_from_routed_cluster() {
        use std::path::PathBuf;
        use tempfile::tempdir;

        let upstream_response: &'static [u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let upstream_port = spawn_in_process_upstream(upstream_response).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;

        let dir = tempdir().expect("tempdir");
        let path: PathBuf = dir.path().join("access.log");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(
                path.clone(),
                envoy_accesslog::CompiledFormat::from_inline("c=%UPSTREAM_CLUSTER%")
                    .expect("format parses"),
                None,
            )
            .await
            .expect("open sink"),
        );
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: "to_backend".to_string(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".into(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let _ = drive(config, req).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let contents = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        assert!(
            contents.contains("c=backend"),
            "routed-to-cluster request → %UPSTREAM_CLUSTER% renders `backend`; got: {}",
            contents.trim()
        );
    }

    /// phase 43 Task 4: a `direct_response` route never resolves to a
    /// cluster → `%UPSTREAM_CLUSTER%` renders the `-` empty token.
    #[tokio::test(flavor = "multi_thread")]
    async fn hcm_h1_upstream_cluster_none_for_direct_response() {
        use std::path::PathBuf;
        use tempfile::tempdir;

        let dir = tempdir().expect("tempdir");
        let path: PathBuf = dir.path().join("access.log");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(
                path.clone(),
                envoy_accesslog::CompiledFormat::from_inline("c=%UPSTREAM_CLUSTER%")
                    .expect("format parses"),
                None,
            )
            .await
            .expect("open sink"),
        );
        let config = hcm_config_with_access_log(vec![sink]).await;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let _ = drive(config, req).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let contents = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        assert!(
            contents.contains("c=-"),
            "direct_response route → %UPSTREAM_CLUSTER% renders the empty `-` token; got: {}",
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
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
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
                envoy_accesslog::FileSink::new(
                    p.clone(),
                    envoy_accesslog::CompiledFormat::default(),
                    None,
                )
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
            envoy_accesslog::CompiledFormat::default(),
            None,
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
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
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
            envoy_accesslog::FileSink::new(
                path.clone(),
                envoy_accesslog::CompiledFormat::default(),
                None,
            )
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
            envoy_accesslog::CompiledFormat::default(),
            None,
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
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            }),
            rds: None,
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
        let hcm_cfg = HCMConfig::from_config(
            &envoy_cfg,
            cluster_mgr,
            registry,
            None,
            Arc::new(RuntimeSnapshot::default()),
        )
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
        let result = HCMConfig::from_config(
            &envoy_cfg,
            cluster_mgr,
            registry,
            None,
            Arc::new(RuntimeSnapshot::default()),
        )
        .await;
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
        let hcm_cfg = HCMConfig::from_config(
            &envoy_cfg,
            cluster_mgr,
            registry,
            None,
            Arc::new(RuntimeSnapshot::default()),
        )
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
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some(prefix.to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: route_status,
                            body: DataSource {
                                filename: None,
                                inline_string: Some(route_body.to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
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
            name: String::new(),
            r#match: RouteMatch {
                prefix: Some("/".to_string()),
                path: None,
                headers: vec![envoy_config::HeaderMatcher {
                    name: "x-test-path-override".to_string(),
                    mode: envoy_config::HeaderMatcherMode::ExactMatch("/bar".to_string()),
                    invert_match: false,
                }],
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("matched\n".to_string()),
                },
            }),
            typed_per_filter_config: Default::default(),
        };
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![],
            filter_pipeline: pipeline,
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![matcher_route],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
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
            envoy_accesslog::FileSink::new(
                log_path.to_path_buf(),
                envoy_accesslog::CompiledFormat::default(),
                None,
            )
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
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
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

    #[test]
    fn synth_no_healthy_upstream_emits_19_byte_body_and_5_headers() {
        // 12.2 D6.2 / ADR-0037: the no-healthy-upstream synth-503 emits the
        // 19-byte body `no healthy upstream` (matching upstream Envoy v1.33.0
        // per parent-12 §6.2 item-2). Mirrors `synth_status` 5-standard-header
        // shape modulo body + content-length.
        let resp = super::synth_no_healthy_upstream(true);
        assert_eq!(resp.status, 503);
        assert_eq!(resp.body.as_ref(), b"no healthy upstream");
        assert_eq!(resp.body.len(), 19, "exact byte count per ADR-0037");
        let header_names: Vec<&str> = resp.headers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            header_names,
            vec![
                headers::SERVER,
                headers::DATE,
                headers::CONTENT_LENGTH,
                headers::CONTENT_TYPE,
                headers::CONNECTION,
            ],
            "5 standard HTTP/1.1 headers in canonical order"
        );
        let cl = resp
            .headers
            .iter()
            .find(|(n, _)| n == headers::CONTENT_LENGTH)
            .map(|(_, v)| v.as_str())
            .expect("content-length present");
        assert_eq!(cl, "19", "content-length matches body length");
    }

    #[test]
    fn synth_overflow_emits_81_byte_body_and_x_envoy_overloaded() {
        // 15 D5 / ADR-0043 §6.2 finding 3: the max_connections /
        // max_pending_requests:0 overflow synth-503 emits the byte-exact
        // 81-byte body `upstream connect error or disconnect/reset before
        // headers. reset reason: overflow` (no trailing newline) + the
        // `x-envoy-overloaded: true` header (the wire surfacing of Envoy's
        // access-log-only `UO` response flag).
        let r = super::synth_overflow(true);
        assert_eq!(r.status, 503);
        assert_eq!(
            r.body.as_ref(),
            b"upstream connect error or disconnect/reset before headers. reset reason: overflow"
        );
        assert_eq!(r.body.len(), 81, "exact byte count per ADR-0043 §6.2");
        let names: Vec<&str> = r.headers.iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            names
                .iter()
                .any(|n| n.eq_ignore_ascii_case("x-envoy-overloaded")),
            "x-envoy-overloaded header present: {names:?}"
        );
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("x-envoy-overloaded") && v == "true"),
            "x-envoy-overloaded: true"
        );
        assert!(
            names
                .iter()
                .any(|n| n.eq_ignore_ascii_case("content-length")),
            "content-length header present: {names:?}"
        );
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

    /// Phase 33 T9 backstop: build a `[set_metadata, router]` pipeline whose
    /// `set_metadata` filter writes `envoy.test`→`{tier: prod}`.
    fn set_metadata_router_pipeline() -> Arc<envoy_filter::FilterPipeline> {
        let filters = vec![
            envoy_config::HttpFilter {
                name: "envoy.filters.http.set_metadata".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::SetMetadata(
                    envoy_config::SetMetadataConfig {
                        metadata: vec![envoy_config::MetadataEntry {
                            metadata_namespace: "envoy.test".to_string(),
                            value: [("tier".to_string(), "prod".to_string())]
                                .into_iter()
                                .collect(),
                            allow_overwrite: false,
                        }],
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
                .expect("set_metadata+router pipeline builds"),
        )
    }

    /// Phase 33 T9 backstop: prove the per-request dynamic-metadata store
    /// threads end-to-end through the H1 HCM into the access-log record and is
    /// rendered by `%DYNAMIC_METADATA%`. Drives one request through a
    /// `[set_metadata, router]` chain + a file logger whose `log_format`
    /// reads a present key (`tier`→`prod`) and an absent key (`missing`→`-`),
    /// then scrapes the written line. This is the sole proof that the
    /// capture-before-drop at the H1 4-field write-back populates the record.
    #[tokio::test]
    async fn h1_dynamic_metadata_threads_into_access_log() {
        let pipeline = set_metadata_router_pipeline();
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let format = envoy_accesslog::CompiledFormat::from_inline(
            "%DYNAMIC_METADATA(envoy.test:tier)% / %DYNAMIC_METADATA(envoy.test:missing)%\n",
        )
        .expect("valid log_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), format, None)
                .await
                .expect("open FileSink"),
        );
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: pipeline,
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let _ = drive(config, req).await;
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "prod / -\n",
            "access log renders present key `prod` and absent key `-`: {logged:?}"
        );
    }

    /// Phase 45 T1 backstop (ADR-0102): build a STATIC NO_FALLBACK subset
    /// cluster `subset_cluster` with ONE endpoint at a literal-unreachable
    /// `127.0.0.1:1` carrying `metadata.filter_metadata.envoy.lb: {stage:prod}`.
    /// A route whose `metadata_match` selects `{stage:nonexistent}` resolves to
    /// NO subset → `pick_endpoint -> None` → the no-healthy synth-503 (the
    /// endpoint is never dialed). Mirrors fixture-0038's `/nope` trigger.
    async fn cluster_mgr_no_fallback_subset() -> Arc<envoy_cluster::ClusterManager> {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners: []
  clusters:
    - name: subset_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      lb_subset_config:
        fallback_policy: NO_FALLBACK
        subset_selectors:
          - keys: [stage]
      load_assignment:
        cluster_name: subset_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
                metadata:
                  filter_metadata:
                    envoy.lb: { stage: prod }
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("bootstrap parses");
        Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
                .await
                .expect("cluster mgr"),
        )
    }

    /// Phase 45 T1 backstop (ADR-0102): drive the FULL H1 dispatch path to a
    /// NO_FALLBACK subset-miss cluster so `pick_endpoint -> None` (the no-healthy
    /// synth-503 arm, hcm.rs:438), capturing the emitted FILE json access-log
    /// line and asserting it carries `rcd:"no_healthy_upstream"` (the phase-45
    /// `else`-branch set at the Proxy arm) — while the response is STILL the
    /// byte-exact 503 + `no healthy upstream` body (UNCHANGED). This is the sole
    /// in-process proof that the no-healthy detail threads into the record built
    /// unconditionally below the writer-arm match (hcm.rs:~1243).
    #[tokio::test]
    async fn h1_no_healthy_access_log_carries_no_healthy_upstream_rcd() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        // json_format logging %RESPONSE_CODE_DETAILS% (key `rcd`) + %RESPONSE_CODE%
        // (key `rc`) — keys sort by UTF-8 byte order (rc, rcd).
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        // A route to `subset_cluster` whose metadata_match selects a
        // non-existent subset (`{stage:nonexistent}`) → subset-miss → 503.
        let mut envoy_lb = std::collections::BTreeMap::new();
        envoy_lb.insert("stage".to_string(), "nonexistent".to_string());
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_no_fallback_subset().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "subset_cluster".into(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: Some(LbMetadata { envoy_lb }),
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        // The 503 + body must be UNCHANGED by the additive detail set.
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "no-healthy synth-503 status unchanged: {resp_str}"
        );
        assert!(
            resp_str.ends_with("no healthy upstream"),
            "no-healthy synth-503 body unchanged: {resp_str}"
        );
        // Brief yield so the FileSink flush reaches disk.
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rcd\":\"no_healthy_upstream\"}\n",
            "no-healthy access-log line carries rcd:\"no_healthy_upstream\": {logged:?}"
        );
    }

    /// Phase 49 T1 backstop (ADR-0106): the no-healthy `pick()->None` synth-503
    /// arm (hcm.rs:1000-1001) emits `%RESPONSE_FLAGS%` = `UH` (NoHealthyUpstream).
    /// Clone of `h1_no_healthy_access_log_carries_no_healthy_upstream_rcd` with
    /// `rf` added to the json_format. `no_healthy_upstream` (set at the
    /// `pick()->None` arm) is 1:1 with the UH flag, derived at the record-build
    /// site (hcm.rs:1232). The 503 status/body are UNCHANGED (additive). Keys
    /// sort UTF-8: rc, rcd, rf.
    #[tokio::test]
    async fn h1_no_healthy_access_log_carries_uh_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        // json_format logging %RESPONSE_CODE% (rc) + %RESPONSE_CODE_DETAILS%
        // (rcd) + %RESPONSE_FLAGS% (rf) — keys sort by UTF-8 byte order
        // (rc, rcd, rf).
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        map.insert(
            "rf".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_FLAGS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        // A route to `subset_cluster` whose metadata_match selects a
        // non-existent subset (`{stage:nonexistent}`) → subset-miss → 503.
        let mut envoy_lb = std::collections::BTreeMap::new();
        envoy_lb.insert("stage".to_string(), "nonexistent".to_string());
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_no_fallback_subset().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "subset_cluster".into(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: Some(LbMetadata { envoy_lb }),
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        // The 503 + body must be UNCHANGED by the additive flag derive.
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "no-healthy synth-503 status unchanged: {resp_str}"
        );
        assert!(
            resp_str.ends_with("no healthy upstream"),
            "no-healthy synth-503 body unchanged: {resp_str}"
        );
        // Brief yield so the FileSink flush reaches disk.
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rcd\":\"no_healthy_upstream\",\"rf\":\"UH\"}\n",
            "no-healthy access-log line carries rf:\"UH\": {logged:?}"
        );
    }

    /// Phase 50 (ADR-0107) §F backstop: drive the FULL H1 dispatch path with a
    /// CONFIGURED pool (`pool_mgr: Some`) whose cluster carries
    /// `circuit_breakers.thresholds:[{max_connections:1, max_pending_requests:0}]`
    /// and a single dead endpoint (`127.0.0.1:1`, never dialed). The first
    /// connect-on-miss is rejected with `PoolError::PendingOverflow` → the
    /// `AcquireOutcome::Overflow` → `AttemptResult{endpoint:Some, outcome:None}`
    /// consumed at the retry-loop site (hcm.rs:990) → the overflow synth-503.
    /// Asserts the FILE json access-log line carries the overflow detail and the
    /// derived UO flag — the sole in-process proof of §A's outcome discriminator
    /// + §B's derive arm on the POOL-overflow path. Fail-first: pre-change it
    /// renders `"rcd":"via_upstream","rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_pool_overflow_access_log_carries_uo_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        map.insert(
            "rf".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_FLAGS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        // STATIC cluster, dead endpoint 127.0.0.1:1 (never dialed), circuit
        // breakers max_connections:1 / max_pending_requests:0 → the
        // connect-on-miss pending-gate rejects the first request.
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 1
            max_pending_requests: 0
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 1 } } }
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("bootstrap parses");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("cluster mgr"),
        );
        let pool_token = tokio_util::sync::CancellationToken::new();
        let pool_mgr = crate::pool::H1PoolManager::for_bootstrap(
            &bootstrap,
            &cluster_mgr,
            Arc::clone(&registry),
            pool_token.clone(),
        )
        .expect("pool manager builds");
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: Arc::clone(&cluster_mgr),
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: Some(Arc::clone(&pool_mgr)),
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        // The overflow synth-503 + 81-byte body must be UNCHANGED.
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "overflow synth-503 status unchanged: {resp_str}"
        );
        assert!(
            resp_str.ends_with(
                "upstream connect error or disconnect/reset before headers. reset reason: overflow"
            ),
            "overflow synth-503 body unchanged: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged,
            "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{overflow}\",\"rf\":\"UO\"}\n",
            "pool-overflow access-log line carries the overflow rcd + rf:\"UO\": {logged:?}"
        );
    }

    /// Phase 46 T1 backstop (ADR-0103): drive the FULL H1 dispatch path with a
    /// vhost (`domains:["*"]`) whose SINGLE route matches only `/specific`, then
    /// probe a NON-matching path (`/nomatch`) so the route-walk hits the
    /// no-matching-route `synth_404` arm (hcm.rs:1553), capturing the emitted
    /// FILE json access-log line and asserting it carries `rcd:"route_not_found"`
    /// (the phase-46 detail set at that arm) — while the response is STILL the
    /// byte-exact 404 with the standard empty 404 body (UNCHANGED). This is the
    /// sole in-process proof that the route-miss detail threads into the record
    /// built unconditionally below the writer-arm match (hcm.rs:~1247).
    #[tokio::test]
    async fn h1_route_miss_access_log_carries_route_not_found_rcd() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        // json_format logging %RESPONSE_CODE_DETAILS% (key `rcd`) + %RESPONSE_CODE%
        // (key `rc`) — keys sort by UTF-8 byte order (rc, rcd).
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        // A vhost `domains:["*"]` (so the host-miss arm at :1535 is NEVER hit)
        // with a SINGLE direct_response route matching only `/specific`. Probing
        // `/nomatch` misses → the no-matching-route arm at :1553 → synth_404.
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/specific".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(envoy_config::DirectResponse {
                            status: 200,
                            body: envoy_config::DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET /nomatch HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        // The 404 + standard (empty) 404 body must be UNCHANGED by the additive
        // detail set.
        assert!(
            resp_str.starts_with("HTTP/1.1 404 "),
            "route-miss synth-404 status unchanged: {resp_str}"
        );
        assert!(
            resp_str.contains("content-length: 0\r\n"),
            "route-miss synth-404 body unchanged (empty): {resp_str}"
        );
        // Brief yield so the FileSink flush reaches disk.
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":404,\"rcd\":\"route_not_found\"}\n",
            "route-miss access-log line carries rcd:\"route_not_found\": {logged:?}"
        );
    }

    /// Phase 47 T1 backstop (ADR-0104): drive the FULL H1 dispatch path with a
    /// vhost whose `domains:["match.test"]` is NON-wildcard and a catch-all
    /// `/` route, then probe with a NON-EMPTY, NON-MATCHING `Host: nomatch.test`
    /// so the route-walk hits the no-matching-VIRTUAL_HOST `synth_404` arm
    /// (hcm.rs:1535 — the host-miss arm, NOT the route-miss arm at :1553),
    /// capturing the emitted FILE json access-log line and asserting it carries
    /// `rcd:"route_not_found"` (the phase-47 detail set at that arm) — while the
    /// response is STILL the byte-exact 404 with the standard empty 404 body
    /// (UNCHANGED). This is the sole in-process proof that the host-miss detail
    /// threads into the record built unconditionally below the writer-arm match.
    /// The `Host: nomatch.test` MUST be non-empty: an empty Host would trip the
    /// codec's `synth_400` guard before the vhost-walk (a different path).
    #[tokio::test]
    async fn h1_host_miss_access_log_carries_route_not_found_rcd() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        // json_format logging %RESPONSE_CODE_DETAILS% (key `rcd`) + %RESPONSE_CODE%
        // (key `rc`) — keys sort by UTF-8 byte order (rc, rcd).
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        // A vhost `domains:["match.test"]` (NON-wildcard) with a catch-all `/`
        // direct_response route. Probing `Host: nomatch.test` matches NO vhost
        // → the no-matching-virtual_host arm at :1535 → synth_404 (the route
        // walk never even runs).
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["match.test".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(envoy_config::DirectResponse {
                            status: 200,
                            body: envoy_config::DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: nomatch.test\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        // The 404 + standard (empty) 404 body must be UNCHANGED by the additive
        // detail set.
        assert!(
            resp_str.starts_with("HTTP/1.1 404 "),
            "host-miss synth-404 status unchanged: {resp_str}"
        );
        assert!(
            resp_str.contains("content-length: 0\r\n"),
            "host-miss synth-404 body unchanged (empty): {resp_str}"
        );
        // Brief yield so the FileSink flush reaches disk.
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":404,\"rcd\":\"route_not_found\"}\n",
            "host-miss access-log line carries rcd:\"route_not_found\": {logged:?}"
        );
    }

    /// Phase 48 T1 backstop (ADR-0105): the route-miss no-route `synth_404` arm
    /// (hcm.rs:1555) emits `%RESPONSE_FLAGS%` = `NR` (NoRoute). Clone of
    /// `h1_route_miss_access_log_carries_route_not_found_rcd` with `rf` added to
    /// the json_format. `route_not_found` (set at the route-miss arm) is 1:1 with
    /// the NR flag, derived at the record-build site (hcm.rs:1225). The 404
    /// status/body are UNCHANGED (additive). Keys sort UTF-8: rc, rcd, rf.
    #[tokio::test]
    async fn h1_route_miss_access_log_carries_nr_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        map.insert(
            "rf".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_FLAGS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        // vhost `domains:["*"]` (host-miss arm never hit) + a SINGLE route on
        // `/specific`. Probing `/nomatch` misses → the route-miss arm (:1555).
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/specific".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(envoy_config::DirectResponse {
                            status: 200,
                            body: envoy_config::DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET /nomatch HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 404 "),
            "route-miss synth-404 status unchanged: {resp_str}"
        );
        assert!(
            resp_str.contains("content-length: 0\r\n"),
            "route-miss synth-404 body unchanged (empty): {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":404,\"rcd\":\"route_not_found\",\"rf\":\"NR\"}\n",
            "route-miss access-log line carries rf:\"NR\": {logged:?}"
        );
    }

    /// Phase 48 T1 backstop (ADR-0105): the host-miss no-route `synth_404` arm
    /// (hcm.rs:1536) emits `%RESPONSE_FLAGS%` = `NR` (NoRoute). Clone of
    /// `h1_host_miss_access_log_carries_route_not_found_rcd` with `rf` added. The
    /// `Host: nomatch.test` MUST be non-empty (an empty Host trips the codec's
    /// synth_400 guard — a different path).
    #[tokio::test]
    async fn h1_host_miss_access_log_carries_nr_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        map.insert(
            "rf".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_FLAGS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        // vhost `domains:["match.test"]` (NON-wildcard) + catch-all `/` route.
        // Probing `Host: nomatch.test` matches NO vhost → the host-miss arm (:1536).
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["match.test".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(envoy_config::DirectResponse {
                            status: 200,
                            body: envoy_config::DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: nomatch.test\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 404 "),
            "host-miss synth-404 status unchanged: {resp_str}"
        );
        assert!(
            resp_str.contains("content-length: 0\r\n"),
            "host-miss synth-404 body unchanged (empty): {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":404,\"rcd\":\"route_not_found\",\"rf\":\"NR\"}\n",
            "host-miss access-log line carries rf:\"NR\": {logged:?}"
        );
    }

    /// Phase 34 T5 backstop: prove that the `header_to_metadata` filter's
    /// output threads end-to-end through the H1 HCM into the access-log record.
    /// Mirrors `h1_dynamic_metadata_threads_into_access_log` verbatim, swapping
    /// the filter chain to `[header_to_metadata, router]` with a rule that
    /// extracts request header `x-tier` → `envoy.lb:tier` on presence.
    /// Drives one H1 GET with `x-tier: prod`; asserts the rendered log line
    /// shows `prod` for the written key and `-` for the absent sentinel.
    #[tokio::test]
    async fn h1_header_to_metadata_threads_into_access_log() {
        use tempfile::tempdir;
        let filters = vec![
            envoy_config::HttpFilter {
                name: "envoy.filters.http.header_to_metadata".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::HeaderToMetadata(
                    envoy_config::HeaderToMetadataConfig {
                        request_rules: vec![envoy_config::HeaderToMetadataRule {
                            header: "x-tier".to_string(),
                            on_header_present: Some(envoy_config::HeaderToMetadataKeyValue {
                                metadata_namespace: "envoy.lb".to_string(),
                                key: "tier".to_string(),
                                value: None,
                                r#type: envoy_config::HeaderToMetadataType::String,
                            }),
                            on_header_missing: None,
                        }],
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
        let pipeline = Arc::new(
            envoy_filter::FilterPipeline::build_from_config(&filters, &registry, "test_prefix")
                .expect("header_to_metadata+router pipeline builds"),
        );
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let format = envoy_accesslog::CompiledFormat::from_inline(
            "%DYNAMIC_METADATA(envoy.lb:tier)% / %DYNAMIC_METADATA(envoy.lb:missing)%\n",
        )
        .expect("valid log_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), format, None)
                .await
                .expect("open FileSink"),
        );
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: pipeline,
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nx-tier: prod\r\nConnection: close\r\n\r\n";
        let _ = drive(config, req).await;
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "prod / -\n",
            "access log renders header-derived `prod` and absent key `-`: {logged:?}"
        );
    }

    // ── 13.1 Task 4: H1Pool dispatch integration ──────────────────────────

    /// 13.1 Task 4 (D4): regression-equivalence proof that the H1 proxy arm
    /// dispatches through `H1Pool::acquire()`. Drives 5 sequential GET
    /// requests through a single downstream H1 keep-alive connection against
    /// an in-process echo backend; asserts the SHARED `cluster.<name>.upstream_cx_total`
    /// counter == 1 (not 5). At the per-call-`Client::connect` regression
    /// this counter would be 5; the pool path coalesces all 5 onto a single
    /// upstream TCP connection (lock-in #6: `cx_total.inc()` fires inside
    /// `H1Pool::acquire`'s connect-on-miss only).
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_hcm_pool_reuses_upstream_conn_across_sequential_requests() {
        // Echo backend that handles many sequential requests on the same
        // socket (keep-alive). Modeled on `pool.rs::echo_backend()`.
        let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_port = backend_listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = backend_listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    loop {
                        let n = match sock.read(&mut buf).await {
                            Ok(0) => return,
                            Ok(n) => n,
                            Err(_) => return,
                        };
                        if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                            let _ = sock
                                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: keep-alive\r\n\r\n")
                                .await;
                        }
                    }
                });
            }
        });

        // Build a single-cluster bootstrap pointed at the backend port.
        // SHARED registry so the cluster_mgr-side `upstream_cx_total` and the
        // pool-side handles are the SAME `Arc<Counter>` (idempotent re-register
        // per envoy-stats's same-kind contract; verified at Task 3).
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
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }} }} }}
"#
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("bootstrap parses");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("cluster mgr"),
        );
        let pool_token = tokio_util::sync::CancellationToken::new();
        let pool_mgr = crate::pool::H1PoolManager::for_bootstrap(
            &bootstrap,
            &cluster_mgr,
            Arc::clone(&registry),
            pool_token.clone(),
        )
        .expect("pool manager builds");

        // Build the HCMConfig with the SHARED pool manager wired in
        // (production-path; the production codepath the bin takes).
        let hcm_config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: Arc::clone(&cluster_mgr),
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: Some(Arc::clone(&pool_mgr)),
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "rc".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });

        // Re-register the shared cx_total handle for assertion.
        let cx_total = registry
            .register_counter("cluster.backend.upstream_cx_total")
            .expect("cx_total re-register (idempotent)");

        // Spawn the HCM accept loop on an ephemeral port; one downstream
        // socket, 5 sequential keep-alive requests.
        let hcm_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let hcm_addr = hcm_listener.local_addr().unwrap();
        let hcm_handle = tokio::spawn(async move {
            let (sock, _) = hcm_listener.accept().await.unwrap();
            let _ = serve_connection(hcm_config, sock).await;
        });

        let mut client = TcpStream::connect(hcm_addr).await.unwrap();
        for i in 0..5 {
            // keep-alive requests (no `Connection: close`).
            let req = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n";
            client
                .write_all(req)
                .await
                .unwrap_or_else(|e| panic!("write request #{i} failed: {e}"));
            // Read exactly one response: read until we see the end of the
            // response head (CRLFCRLF) — the response is CL: 0 so head-end
            // == response-end.
            let mut buf = vec![0u8; 4096];
            let mut total = 0usize;
            loop {
                let n =
                    tokio::time::timeout(StdDuration::from_secs(2), client.read(&mut buf[total..]))
                        .await
                        .unwrap_or_else(|_| panic!("read response #{i} timed out"))
                        .unwrap_or_else(|e| panic!("read response #{i} failed: {e}"));
                if n == 0 {
                    panic!("server closed before response #{i} fully read");
                }
                total += n;
                if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let resp = String::from_utf8_lossy(&buf[..total]);
            assert!(
                resp.starts_with("HTTP/1.1 200 OK"),
                "request #{i} expected 200: {resp}"
            );
        }
        drop(client);
        let _ = hcm_handle.await;
        pool_token.cancel();

        // The load-bearing assertion: 5 requests, 1 upstream TCP connection.
        // Pre-13.1 (tier-1 per-conn cache, scoped to ONE downstream conn) this
        // would also be 1 on the SAME downstream socket — but the pool wins
        // ACROSS downstream connections too (verified end-to-end in the
        // Docker-gated 0020 fixture at Task 7). This unit test pins the
        // per-conn reuse property at the crate boundary.
        assert_eq!(
            cx_total.value(),
            1,
            "expected exactly ONE upstream TCP connection across 5 sequential keep-alive requests \
             (got {}); the H1Pool dispatch path should coalesce reuse onto a single connect-on-miss",
            cx_total.value(),
        );
    }

    /// 13.1 Task 4 code-quality fold-in regression: an in-flight pool-path
    /// request must report `cluster.<name>.upstream_cx_active == 1`, not 2.
    ///
    /// Pre-fix shape: an outer-scope `let _cx_guard = cluster.cx_active_guard()`
    /// in the HCM proxy arm fired BEFORE the pool dispatch, and the pool's
    /// `PoolGuard` (pool.rs::PoolGuard::_cx_active_guard, built via
    /// `acquire_cx_active_guard`) ALSO fired against the SAME `Arc<Gauge>`
    /// (the registry's idempotent same-kind re-register hands back the same
    /// `Arc`). Result: every in-flight pool-path request reported double the
    /// real cx_active. Steady-state at-rest was correct (paired inc/dec), but
    /// any live `/stats` scrape during traffic was 2×.
    ///
    /// This test asserts cx_active == 1 (NOT 2) WHILE the upstream request
    /// is in flight, by driving a slow backend that accepts + reads the
    /// request bytes but holds the response open behind a `oneshot::Sender`.
    /// We scrape cx_active.value() during the in-flight window, then release
    /// the response and assert the post-Drop steady state (cx_active == 0).
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_hcm_pool_path_does_not_double_count_cx_active() {
        use tokio::sync::oneshot;

        // Slow backend: accepts ONE connection, reads the request, then
        // waits on a oneshot before writing the response. This pins one
        // request "in flight" while we scrape cx_active.
        let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_port = backend_listener.local_addr().unwrap().port();
        let (release_tx, release_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let (mut sock, _) = backend_listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            // Read until end-of-head (CRLFCRLF). The test issues a single
            // body-less GET so head-end == request-end.
            loop {
                let n = sock.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            // Hold the response until the test releases us.
            let _ = release_rx.await;
            let _ = sock
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: keep-alive\r\n\r\n",
                )
                .await;
        });

        // Single-cluster bootstrap pointed at the slow backend port.
        // SHARED registry so the HCM-side and pool-side cx_active handles
        // are the SAME `Arc<Gauge>` (same as the precedent test above).
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
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }} }} }}
"#
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("bootstrap parses");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("cluster mgr"),
        );
        let pool_token = tokio_util::sync::CancellationToken::new();
        let pool_mgr = crate::pool::H1PoolManager::for_bootstrap(
            &bootstrap,
            &cluster_mgr,
            Arc::clone(&registry),
            pool_token.clone(),
        )
        .expect("pool manager builds");

        let hcm_config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: Arc::clone(&cluster_mgr),
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: Some(Arc::clone(&pool_mgr)),
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "rc".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });

        // Re-register the SHARED cx_active handle for assertion.
        let cx_active = registry
            .register_gauge("cluster.backend.upstream_cx_active")
            .expect("cx_active re-register (idempotent)");

        // Spawn the HCM accept loop on an ephemeral port; one downstream
        // socket, one request that the backend holds open.
        let hcm_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let hcm_addr = hcm_listener.local_addr().unwrap();
        let hcm_handle = tokio::spawn(async move {
            let (sock, _) = hcm_listener.accept().await.unwrap();
            let _ = serve_connection(hcm_config, sock).await;
        });

        let mut client = TcpStream::connect(hcm_addr).await.unwrap();
        let req = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n";
        client.write_all(req).await.expect("write request");

        // Wait for the request to land at the backend and the pool's
        // connect-on-miss to fire its cx_active.inc(). 200ms is generous
        // for an in-process TCP loopback hop.
        tokio::time::sleep(StdDuration::from_millis(200)).await;

        // LOAD-BEARING assertion (the regression we're guarding):
        // cx_active must be 1 (the pool-owned guard inside PoolGuard) —
        // not 2 (pool-owned guard + an outer HCM-scope guard, which was
        // the pre-fix bug).
        let live = cx_active.value();
        assert_eq!(
            live, 1,
            "expected cx_active == 1 (pool-owned guard only) during in-flight \
             pool-path request; got {live} — outer HCM-scope `cx_active_guard()` \
             is double-counting against the same Arc<Gauge> the pool already \
             holds (pre-fix regression).",
        );

        // Release the backend response so the request can complete and
        // the pool returns the stream to the idle list (the PoolGuard's
        // cx_active_guard drops here).
        let _ = release_tx.send(());

        // Read the response to drive the HCM through the encode-write path.
        let mut buf = vec![0u8; 4096];
        let mut total = 0usize;
        loop {
            let n = tokio::time::timeout(StdDuration::from_secs(2), client.read(&mut buf[total..]))
                .await
                .expect("read response timeout")
                .expect("read response error");
            if n == 0 {
                break;
            }
            total += n;
            if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let resp = String::from_utf8_lossy(&buf[..total]);
        assert!(resp.starts_with("HTTP/1.1 200 OK"), "expected 200: {resp}");

        drop(client);
        let _ = hcm_handle.await;

        // Post-completion steady state: cx_active back to 0. The stream
        // returns to the pool's idle list; the PoolGuard's `_cx_active_guard`
        // drops on return-to-idle path (pool.rs::PoolGuard::Drop).
        // Allow a brief tick for the Drop to land.
        tokio::time::sleep(StdDuration::from_millis(100)).await;
        assert_eq!(
            cx_active.value(),
            0,
            "expected cx_active == 0 after request completion + PoolGuard drop \
             (got {})",
            cx_active.value(),
        );

        pool_token.cancel();
    }

    // ── 16 Task 4: H1 retry loop tests ───────────────────────────────────────

    /// Build an HCMConfig with a single `Route` action whose `retry_policy` is
    /// the caller-supplied value, the vhost `include_attempt_count_in_response`
    /// flag, and a caller-supplied cluster_mgr. Mirrors `hcm_config_with_cluster`
    /// but exposes the two phase-16 fields.
    fn hcm_config_with_retry(
        prefix: &str,
        cluster: &str,
        retry_policy: Option<envoy_config::RetryPolicy>,
        include_attempt_count: bool,
        cluster_mgr: Arc<envoy_cluster::ClusterManager>,
    ) -> Arc<HCMConfig> {
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "rc".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: include_attempt_count,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some(prefix.to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: cluster.to_string(),
                            retry_policy,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        })
    }

    /// Spawn a stateful in-process upstream that returns `fail_status` (CL: 0)
    /// for its first `fail_count` requests, then `ok` (200, body "ok") for all
    /// subsequent requests. Returns `(port, request_counter)` — the counter is
    /// the total number of upstream requests observed (each accepted+read
    /// connection counts as one). Each request arrives on its own connection
    /// (the HCM's per-call `Client::connect` fallback path used by tests with
    /// `pool_mgr: None`).
    async fn spawn_fail_then_ok_upstream(
        fail_status: u16,
        fail_count: usize,
    ) -> (u16, Arc<std::sync::atomic::AtomicUsize>) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_srv = Arc::clone(&counter);
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let n = counter_srv.fetch_add(1, Ordering::SeqCst);
                let mut buf = vec![0u8; 4096];
                let _ = tokio::time::timeout(Duration::from_millis(500), sock.read(&mut buf)).await;
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
        (port, counter)
    }

    /// 16 Task 4 (L4/L5/L6 success path): backend 503-then-200, retry_on 5xx,
    /// num_retries 1, vhost include_attempt_count true. Downstream 200,
    /// x-envoy-attempt-count: 2, retry=1 / retry_success=1 / limit_exceeded=0,
    /// upstream_rq_total=2, upstream_rq_5xx=0 (retried-away 503 doesn't tick).
    #[tokio::test(flavor = "multi_thread")]
    async fn retry_success_path_503_then_200() {
        let (port, reqs) = spawn_fail_then_ok_upstream(503, 1).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry(
            "/",
            "backend",
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 200 OK\r\n"),
            "downstream must be 200 after retry: {s}"
        );
        assert!(
            s.contains("x-envoy-attempt-count: 2\r\n"),
            "x-envoy-attempt-count: 2 expected: {s}"
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

    /// 16 Task 4 (L4/L5/L6/L9 limit-exceeded path): always-503 backend,
    /// retry_on 5xx, num_retries 1, vhost include_attempt_count true.
    /// Downstream 503 (verbatim last upstream), x-envoy-attempt-count: 2,
    /// retry=1 / retry_success=0 / limit_exceeded=1, upstream_rq_total=2,
    /// upstream_rq_5xx=1 (completing 503 only — not 2).
    #[tokio::test(flavor = "multi_thread")]
    async fn retry_limit_exceeded_path_always_503() {
        let (port, reqs) = spawn_fail_then_ok_upstream(503, 1000).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry(
            "/",
            "backend",
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 503 "),
            "downstream must be the last upstream 503: {s}"
        );
        assert!(
            s.contains("x-envoy-attempt-count: 2\r\n"),
            "x-envoy-attempt-count: 2 expected: {s}"
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

    /// 16 Task 4 (no-retry regression): NO retry_policy, backend 503.
    /// Downstream 503, exactly 1 attempt, upstream_rq_total=1, upstream_rq_5xx=1,
    /// NO x-envoy-attempt-count header, all 3 retry counters 0. Proves the
    /// no-retry path is byte-identical to pre-phase-16 behavior.
    #[tokio::test(flavor = "multi_thread")]
    async fn retry_absent_no_retry_single_attempt() {
        let (port, reqs) = spawn_fail_then_ok_upstream(503, 1000).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry("/", "backend", None, false, cluster_mgr);
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(s.starts_with("HTTP/1.1 503 "), "downstream 503: {s}");
        assert!(
            !s.to_ascii_lowercase().contains("x-envoy-attempt-count"),
            "no attempt-count header without vhost flag: {s}"
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

    /// 16 (connect-failure retry): cluster endpoint is 127.0.0.1:1 (kernel-
    /// refused — a deterministic connect failure), retry_on "connect-failure",
    /// num_retries 1. The connect failure MUST classify as
    /// `AttemptOutcome::ConnectFailure` and therefore be retriable under
    /// `connect-failure` (without `reset`). Asserts: downstream synth-503,
    /// upstream_rq_retry=1 (the retry fired → ConnectFailure classification),
    /// limit_exceeded=1 (the retried attempt also refused), retry_success=0,
    /// upstream_rq_total=0 (no upstream RESPONSE was ever received). Sibling of
    /// H2's `h2_connect_failure_retried_on_connect_failure_policy`.
    #[tokio::test(flavor = "multi_thread")]
    async fn connect_failure_retried_on_connect_failure_policy() {
        let cluster_mgr = cluster_mgr_with_endpoint("backend", 1).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry(
            "/",
            "backend",
            Some(envoy_config::RetryPolicy {
                retry_on: "connect-failure".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
            "downstream must be synth-503 after exhausting connect-failure retries: {s}"
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

    /// 16 state-5 review fix: a connect-failure synth-503 with NO retry_policy
    /// (1 attempt) must NOT tick `upstream_rq_5xx`. The post-loop 5xx tick is
    /// gated on the completing attempt having received a REAL upstream response;
    /// the synth-503 (kernel-refused connect) never reached an upstream, so per
    /// ADR-0045 L5 the 1-attempt path is byte-identical to the pre-phase-16
    /// baseline where this path never ticked rq_5xx. Sibling of H2's
    /// `h2_connect_failure_synth_does_not_tick_upstream_rq_5xx`.
    #[tokio::test(flavor = "multi_thread")]
    async fn connect_failure_synth_does_not_tick_upstream_rq_5xx() {
        // 127.0.0.1:1 is kernel-refused — a deterministic connect failure.
        let cluster_mgr = cluster_mgr_with_endpoint("backend", 1).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry("/", "backend", None, false, cluster_mgr);
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
            "downstream must be connect-failure synth-503: {s}"
        );
        assert_eq!(
            cluster.upstream_rq_5xx().value(),
            0,
            "rq_5xx 0 — synth-503 (no real upstream response) must not tick the completing-5xx counter"
        );
        assert_eq!(
            cluster.upstream_rq_total().value(),
            0,
            "rq_total 0 — no upstream response was ever received"
        );
    }

    // ── 17 Task 4: H1 retry-budget gate tests (ADR-0047) ─────────────────────

    /// 17 Task 4 (a) budget-blocked retry (L1/L6/L7): always-503 backend,
    /// retry_on 5xx, num_retries 1, but `circuit_breakers.thresholds[0]
    /// .max_retries: 0`. The FIRST attempt dispatches normally; the would-be
    /// retry is budget-rejected (L1). The downstream response is the backend's
    /// real 503 VERBATIM (L6 — not the overflow synth body, no
    /// `x-envoy-overloaded`). x-envoy-attempt-count: 1.
    /// upstream_rq_retry_overflow=1, upstream_rq_retry=0,
    /// upstream_rq_retry_limit_exceeded=0 (L7 exclusivity), upstream_rq_retry_
    /// success=0, upstream_rq_total=1, backend saw exactly 1 request.
    #[tokio::test(flavor = "multi_thread")]
    async fn budget_blocked_retry_max_retries_zero() {
        let (port, reqs) = spawn_fail_then_ok_upstream(503, 1000).await;
        let cluster_mgr = cluster_mgr_with_endpoint_max_retries("backend", port, Some(0)).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry(
            "/",
            "backend",
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 503 "),
            "downstream must be the backend's real 503 verbatim: {s}"
        );
        assert!(
            !s.to_ascii_lowercase().contains("x-envoy-overloaded"),
            "budget-blocked retry must NOT be the overflow synth (no x-envoy-overloaded): {s}"
        );
        assert!(
            s.contains("x-envoy-attempt-count: 1\r\n"),
            "x-envoy-attempt-count: 1 — only the first attempt dispatched: {s}"
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

    /// 17 Task 4 (b) budget-allowed control (L10): same shape as (a) but
    /// `max_retries` is the default (3 — never blocks a single sequential
    /// retry) and the backend is fail-once-then-succeed. The retry fires
    /// normally: downstream 200, x-envoy-attempt-count: 2, upstream_rq_retry=1,
    /// upstream_rq_retry_success=1, upstream_rq_retry_overflow=0,
    /// upstream_rq_total=2.
    #[tokio::test(flavor = "multi_thread")]
    async fn budget_allowed_retry_default_max_retries() {
        let (port, reqs) = spawn_fail_then_ok_upstream(503, 1).await;
        // Default max_retries (3) — None omits the `max_retries:` line; the
        // circuit_breakers block still carries `track_remaining` so the budget
        // is configured (gating active) but the cap defaults to 3.
        let cluster_mgr = cluster_mgr_with_endpoint_max_retries("backend", port, None).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry(
            "/",
            "backend",
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 200 OK\r\n"),
            "downstream must be 200 after the budget-allowed retry: {s}"
        );
        assert!(
            s.contains("x-envoy-attempt-count: 2\r\n"),
            "x-envoy-attempt-count: 2 expected: {s}"
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

    /// 17 Task 4 (c) regression (L10/iv): NO circuit_breakers at all + retry_
    /// policy + fail-once-then-succeed backend → identical retry behavior to (b)
    /// (200, x-envoy-attempt-count: 2, retry=1 / retry_success=1, rq_total=2).
    /// `try_acquire_retry` returns `Unlimited` → byte-identical to phase-16. No
    /// budget stats registered (the overflow counter is unconditional but inert
    /// at 0).
    #[tokio::test(flavor = "multi_thread")]
    async fn budget_absent_retry_unlimited_regression() {
        let (port, reqs) = spawn_fail_then_ok_upstream(503, 1).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry(
            "/",
            "backend",
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 200 OK\r\n"),
            "downstream must be 200 after retry (Unlimited budget): {s}"
        );
        assert!(
            s.contains("x-envoy-attempt-count: 2\r\n"),
            "x-envoy-attempt-count: 2 expected: {s}"
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

    // ── 17 Task 5: H1 request-budget gate tests (max_requests; ADR-0047) ─────

    /// 17 Task 5: like `cluster_mgr_with_endpoint_max_retries` but emits a
    /// `max_requests:` cap instead of `max_retries:`. Returns the shared
    /// `StatsRegistry` alongside the manager so tests can read the
    /// `cluster.<name>.upstream_rq_pending_overflow` counter (which has no
    /// public ClusterHandle accessor — it lives inside the BudgetState). The
    /// registry's `register_counter` is idempotent (returns the already-
    /// registered Arc), so re-registering by name reflects live ticks.
    ///
    /// The `(ClusterManager, StatsRegistry)` return shape differs from the
    /// `ClusterManager`-only shape of `cluster_mgr_with_endpoint_max_retries`
    /// because `pending_overflow` has no public `ClusterHandle` accessor and
    /// must be read through the registry. The H2 mirror (Task 7) should follow
    /// the same `(manager, registry)` shape for the same reason.
    async fn cluster_mgr_with_endpoint_max_requests(
        name: &str,
        port: u16,
        max_requests: u32,
    ) -> (
        Arc<envoy_cluster::ClusterManager>,
        Arc<envoy_stats::StatsRegistry>,
    ) {
        cluster_mgr_with_endpoint_opts(
            name,
            port,
            TestBreakers::Threshold(Some(("max_requests", max_requests))),
        )
        .await
    }

    /// Read `cluster.<name>.upstream_rq_pending_overflow` from a shared
    /// registry (idempotent re-register returns the live Arc).
    fn pending_overflow(registry: &envoy_stats::StatsRegistry, name: &str) -> u64 {
        registry
            .register_counter(&format!("cluster.{name}.upstream_rq_pending_overflow"))
            .expect("counter")
            .value()
    }

    /// 17 Task 5 (a) request-breaker overflow (L2/L3/L11): `max_requests: 0`
    /// (always-open request breaker), NO retry_policy, vhost
    /// include_attempt_count true. The FIRST downstream request is rejected at
    /// the dispatch entry BEFORE any backend contact → downstream 503 with the
    /// 81-byte overflow body + `x-envoy-overloaded: true` + `x-envoy-attempt-
    /// count: 1`. Backend NEVER contacted (request counter == 0).
    /// upstream_rq_pending_overflow=1, upstream_rq_5xx=1 (L3/ADR-0047 — the ONLY
    /// synth path that ticks it), upstream_rq_total=0, upstream_rq_retry_
    /// overflow=0 (L9a exclusivity).
    #[tokio::test(flavor = "multi_thread")]
    async fn request_budget_overflow_max_requests_zero() {
        // Backend that records every accepted connection; never expected to fire.
        let (port, reqs) = spawn_fail_then_ok_upstream(200, 0).await;
        let (cluster_mgr, registry) =
            cluster_mgr_with_endpoint_max_requests("backend", port, 0).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry("/", "backend", None, true, cluster_mgr);
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 503 "),
            "downstream must be the overflow synth-503: {s}"
        );
        assert!(
            s.to_ascii_lowercase()
                .contains("x-envoy-overloaded: true\r\n"),
            "overflow synth carries x-envoy-overloaded: true: {s}"
        );
        assert!(
            s.contains(
                "upstream connect error or disconnect/reset before headers. reset reason: overflow"
            ),
            "81-byte overflow body: {s}"
        );
        assert!(
            s.contains("x-envoy-attempt-count: 1\r\n"),
            "x-envoy-attempt-count: 1 (L11): {s}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "backend NEVER contacted — the gate fires before pool contact"
        );
        assert_eq!(
            pending_overflow(&registry, "backend"),
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

    /// Phase 50 (ADR-0107) §A′ backstop: the pre-route request-budget overflow
    /// (`max_requests:0`, BudgetAcquisition::Rejected at hcm.rs:911) calls
    /// synth_overflow at :923 and BYPASSES the retry loop, so it is tagged
    /// directly at :923 (not via the :995 discriminator). Asserts the FILE json
    /// access-log line carries the overflow rcd + rf:"UO" — the in-process proof
    /// for the budget arm (its differential witness is deferred: M50-C).
    /// Fail-first: pre-change it renders `"rcd":null,"rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_request_budget_overflow_access_log_carries_uo_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        // A live backend port is required for cluster_mgr_with_endpoint_max_requests;
        // it is NEVER contacted (the budget gate fires before any dispatch).
        let (port, _reqs) = spawn_fail_then_ok_upstream(200, 0).await;
        let (cluster_mgr, _registry) =
            cluster_mgr_with_endpoint_max_requests("backend", port, 0).await;
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        map.insert(
            "rf".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_FLAGS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "request-budget overflow synth-503 status unchanged: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged,
            "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{overflow}\",\"rf\":\"UO\"}\n",
            "request-budget overflow access-log line carries the overflow rcd + rf:\"UO\": {logged:?}"
        );
    }

    /// Phase 51 (ADR-0108) §F backstop: drive the H1 retry-limit-exceeded (L9)
    /// path — an always-503 backend (`spawn_fail_then_ok_upstream(503, 1000)`,
    /// fail_count ≫ the 2 attempts) + `retry_policy{retry_on:"5xx",num_retries:1}`
    /// → both attempts 503, the budget of 1 consumed, the last 503 surfaced
    /// downstream verbatim — with a {rc,rcd,rf} FILE json access-log. Asserts the
    /// logged line carries rcd:"via_upstream" (a REAL upstream 503, UNCHANGED —
    /// matches Envoy, NOT rewritten) and the DERIVED rf:"URX" (set at the
    /// limit-exceeded loop-exit boolean, NOT rcd-derived). The sole in-process
    /// proof of §A's discriminator + §B's derive wrapper. Fail-first: pre-change
    /// the derive's rcd-match falls to `_ => "-"` (via_upstream is unmatched) →
    /// it renders `"rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_retry_limit_exceeded_access_log_carries_urx_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        // Always-503 backend: fail_count 1000 ≫ the 2 attempts → every attempt
        // 503 → the retry budget of 1 is consumed → limit-exceeded (L9).
        let (port, _reqs) = spawn_fail_then_ok_upstream(503, 1000).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        map.insert(
            "rf".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_FLAGS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: Some(envoy_config::RetryPolicy {
                                retry_on: "5xx".into(),
                                num_retries: Some(1),
                                retriable_status_codes: vec![],
                            }),
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "retry-limit-exceeded surfaces the last upstream 503 verbatim: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"URX\"}\n",
            "retry-limit-exceeded access-log line carries rcd:via_upstream + rf:URX: {logged:?}"
        );
    }

    /// phase 52 (ADR-0109): a single connect-failure attempt (endpoint
    /// 127.0.0.1:1, kernel-refused) with NO retry_policy, wired to a {rc,rf}
    /// FILE json access-log. Asserts the downstream response is the synth-503
    /// (Task 1) AND the logged line carries the DERIVED rf:"UF" (set post-loop
    /// from the connect-failure final-outcome boolean, NOT rcd-derived — the
    /// connect-failure rcd is the shared "via_upstream"). The sole in-process
    /// proof of §A's discriminator + §B's derive branch. Fail-first: pre-change
    /// the derive's rcd-match falls to `_ => "-"` (via_upstream unmatched) → it
    /// renders `"rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_connect_failure_access_log_carries_uf_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        // 127.0.0.1:1 is kernel-refused — a deterministic connect failure.
        let cluster_mgr = cluster_mgr_with_endpoint("backend", 1).await;
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rf".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_FLAGS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "connect-failure surfaces the synth-503 downstream: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rf\":\"UF\"}\n",
            "connect-failure access-log line carries rf:UF: {logged:?}"
        );
    }

    /// phase 53 (ADR-0110): an accept-then-close loopback upstream (completes
    /// the TCP connect, drains the request, then drops the socket — a graceful
    /// FIN with NO response) with NO retry_policy drives the single H1 reset arm
    /// (hcm.rs:618, AttemptOutcome::Reset). Asserts the downstream response is
    /// the synth-503 (Task 2 corrected the unvalidated 502 to match Envoy's UC
    /// path). Fail-first: pre-change the reset arm synthesizes 502.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_upstream_reset_returns_503() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                // read-then-close: drain the request (post-connect),
                // then FIN with no response.
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                drop(sock);
            }
        });
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "upstream-reset surfaces the synth-503 downstream: {resp_str}"
        );
        server.abort();
    }

    /// Test-audit 2026-07-17 (TEST_GAP_ANALYSIS §5 item 4): upstream reset
    /// AFTER a partial response — status line + headers + a strict prefix of
    /// the Content-Length-declared body, then FIN. The client's CL body-read
    /// loop hits EOF mid-body (`Http1Error::UnexpectedEof`,
    /// client.rs:477-479) and the attempt classifies as Reset, so the
    /// DOWNSTREAM never sees the upstream's 200: envoy-rust fully buffers the
    /// upstream response before writing anything downstream, so a mid-body
    /// reset yields the same clean synth-503 as a reset-before-response.
    ///
    /// DOCUMENTED DIVERGENCE from upstream Envoy: Envoy streams — by the time
    /// the upstream dies mid-body Envoy has already forwarded the 200 headers
    /// and the body prefix, so the downstream observes a TRUNCATED 200 and a
    /// closed connection (no 503 is possible once headers are committed).
    /// The buffered-proxy architecture makes envoy-rust's behavior here
    /// deliberately different; this test pins that choice so any future move
    /// to streaming proxying revisits it consciously (the fixture suite has
    /// no mid-body-reset case — this is the only guard).
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_upstream_reset_mid_body_synthesizes_503() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                // 200 + CL:1000 but only a 10-byte body prefix, then FIN.
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n0123456789")
                    .await;
                drop(sock);
            }
        });
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "mid-body upstream reset surfaces the synth-503 (nothing was \
             committed downstream — buffered proxy): {resp_str}"
        );
        assert!(
            !resp_str.contains("0123456789"),
            "no fragment of the truncated upstream body may leak downstream: {resp_str}"
        );
        server.abort();
    }

    /// phase 53 (ADR-0110) / 54 (ADR-0111): the accept-then-close reset path (NO
    /// retry_policy), wired to a {rc,rcd,rf} FILE json access-log. Asserts the
    /// downstream is the synth-503 AND the logged line carries the deterministic
    /// reset rcd `upstream_reset_before_response_started{connection_termination}`
    /// (set by §A on the pure-reset final-outcome path, overriding the in-loop
    /// `via_upstream`) AND the rf:"UC" now DERIVED 1:1 from that rcd (the
    /// phase-50 `{overflow} => "UO"` precedent — the phase-53 reset-
    /// discriminator boolean was retired). The in-process proof of §A's
    /// rcd-set + §B's rcd-match arm. Fail-first: pre-change the rcd stays
    /// `via_upstream`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_upstream_reset_access_log_carries_uc_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                drop(sock);
            }
        });
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        map.insert(
            "rf".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_FLAGS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "upstream-reset surfaces the synth-503 downstream: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged,
            "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}\n",
            "upstream-reset access-log line carries the deterministic reset rcd + rf:UC: {logged:?}"
        );
        server.abort();
    }

    /// phase 54 (ADR-0111) — the M53-3 NEGATIVE case: a retry-exhausted RESET
    /// (retry_on:"reset", num_retries:1; the accept-then-close backend resets
    /// every attempt). §A's rcd-set is guarded `!retry_limit_exceeded_for_log`,
    /// so the rcd STAYS the shared "via_upstream" (NOT {connection_termination})
    /// and the %RESPONSE_FLAGS% derive renders "URX" (its branch is checked
    /// before the rcd-match). Proves the single most error-prone line in §A:
    /// without the guard, §A would set rcd = "{connection_termination}" and the
    /// rcd assertion fails. The differential 0062 cannot exercise this path.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_retry_exhausted_reset_keeps_via_upstream_rcd_and_urx_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                drop(sock);
            }
        });
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        map.insert(
            "rf".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_FLAGS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt, None)
                .await
                .expect("open FileSink"),
        );
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: Some(envoy_config::RetryPolicy {
                                retry_on: "reset".into(),
                                num_retries: Some(1),
                                retriable_status_codes: vec![],
                            }),
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "retry-exhausted reset surfaces the synth-503 downstream: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"URX\"}\n",
            "retry-exhausted reset keeps rcd:via_upstream + rf:URX (the §A guard): {logged:?}"
        );
        server.abort();
    }

    /// 17 Task 5 (b) gate ordering (L9a): `max_requests: 0` AND a retry_policy.
    /// Same outcome as (a); the retry budget is never consulted (request breaker
    /// fires FIRST). upstream_rq_retry_overflow=0, upstream_rq_retry=0.
    #[tokio::test(flavor = "multi_thread")]
    async fn request_budget_gate_ordering_before_retry() {
        let (port, reqs) = spawn_fail_then_ok_upstream(200, 0).await;
        let (cluster_mgr, registry) =
            cluster_mgr_with_endpoint_max_requests("backend", port, 0).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry(
            "/",
            "backend",
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 503 ") && s.to_ascii_lowercase().contains("x-envoy-overloaded"),
            "same overflow synth-503 as (a) — request breaker fires before the retry breaker: {s}"
        );
        assert!(
            s.contains("x-envoy-attempt-count: 1\r\n"),
            "x-envoy-attempt-count: 1: {s}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "backend NEVER contacted"
        );
        assert_eq!(
            pending_overflow(&registry, "backend"),
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

    /// 17 Task 5 (c) request-budget lifetime (L9b): `max_requests: 1` + retry_
    /// policy + fail-once-then-succeed backend. The request counts ONCE against
    /// the budget for its WHOLE lifetime (the guard spans the retry loop), so
    /// the sequential retry does NOT overflow `max_requests: 1`. Final 200,
    /// x-envoy-attempt-count: 2, upstream_rq_pending_overflow=0.
    #[tokio::test(flavor = "multi_thread")]
    async fn request_budget_lifetime_spans_retry() {
        let (port, reqs) = spawn_fail_then_ok_upstream(503, 1).await;
        let (cluster_mgr, registry) =
            cluster_mgr_with_endpoint_max_requests("backend", port, 1).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry(
            "/",
            "backend",
            Some(envoy_config::RetryPolicy {
                retry_on: "5xx".into(),
                num_retries: Some(1),
                retriable_status_codes: vec![],
            }),
            true,
            cluster_mgr,
        );
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 200 OK\r\n"),
            "final 200 — the sequential retry counts ONCE against max_requests:1 (L9b): {s}"
        );
        assert!(
            s.contains("x-envoy-attempt-count: 2\r\n"),
            "x-envoy-attempt-count: 2: {s}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "2 attempts — both fit under the single request-budget slot"
        );
        assert_eq!(
            pending_overflow(&registry, "backend"),
            0,
            "pending_overflow 0 — one request = one slot for its whole lifetime"
        );
        assert_eq!(cluster.upstream_rq_total().value(), 2, "rq_total 2");
    }

    /// 17 Task 5 (d) regression (vi): NO circuit_breakers → no behavior change.
    /// A plain proxied request works; the request-budget gate returns Unlimited
    /// (zero side-effects). pending_overflow inert at 0.
    #[tokio::test(flavor = "multi_thread")]
    async fn request_budget_absent_unlimited_regression() {
        let (port, reqs) = spawn_fail_then_ok_upstream(200, 0).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let cluster = cluster_mgr.get("backend").expect("backend present");
        let cfg = hcm_config_with_retry("/", "backend", None, false, cluster_mgr);
        let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
        let resp = drive(cfg, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(
            s.starts_with("HTTP/1.1 200 OK\r\n"),
            "plain proxied request unaffected by the absent request breaker: {s}"
        );
        assert!(
            !s.to_ascii_lowercase().contains("x-envoy-overloaded"),
            "no overflow synth on the unlimited path: {s}"
        );
        assert_eq!(
            reqs.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "backend contacted exactly once"
        );
        assert_eq!(cluster.upstream_rq_total().value(), 1, "rq_total 1");
    }

    // ── Phase-23 D2: resolve_route unit tests ────────────────────────────────

    /// Helper: build an HCMConfig with a single VH `domains: ["*"]` and one
    /// prefix-"/" DirectResponse route. Async to allow `cluster_mgr_empty()`.
    async fn resolve_route_test_config() -> HCMConfig {
        HCMConfig {
            stat_prefix: "test".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("test"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        }
    }

    /// Build a minimal `Request` with the given path and Host header value.
    fn make_req(path: &str, host: &str) -> Request {
        use crate::codec::HttpVersion;
        Request {
            method: "GET".to_string(),
            path: path.to_string(),
            version: HttpVersion::Http11,
            headers: if host.is_empty() {
                vec![]
            } else {
                vec![(headers::HOST.to_string(), host.to_string())]
            },
            bytes_consumed: 0,
            body: None,
        }
    }

    /// 109.1 Task 6 helper: an HCMConfig whose table carries a GATED
    /// direct_response route (`/`-prefix, runtime_fraction default 100/HUNDRED
    /// consulting gate.k, body "gated") ABOVE a bare catch-all
    /// (`/`-prefix, body "fallback"), with `runtime` built from one code-built
    /// layer mapping gate.k to `value` (None = empty snapshot). Values are
    /// `RuntimeValue::Str` — a string "0" stringifies to the same final_value
    /// as the yaml integer 0 (envoy-http1 has no serde_yaml dev-dep).
    async fn gated_route_test_config(value: Option<&str>) -> HCMConfig {
        let runtime = match value {
            None => Arc::new(RuntimeSnapshot::default()),
            Some(v) => {
                let mut static_layer = std::collections::BTreeMap::new();
                static_layer.insert(
                    "gate.k".to_string(),
                    envoy_config::RuntimeValue::Str(v.to_string()),
                );
                let layer = envoy_config::RuntimeLayer {
                    name: "l".to_string(),
                    static_layer: Some(static_layer),
                    ..Default::default()
                };
                Arc::new(RuntimeSnapshot::from_layers(
                    vec!["l".to_string()],
                    &[layer],
                ))
            }
        };
        let mk_route = |name: &str, rf, body: &str| Route {
            name: name.to_string(),
            r#match: RouteMatch {
                prefix: Some("/".to_string()),
                path: None,
                headers: vec![],
                runtime_fraction: rf,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some(body.to_string()),
                },
            }),
            typed_per_filter_config: Default::default(),
        };
        HCMConfig {
            stat_prefix: "test".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("test"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![
                        mk_route(
                            "gated",
                            Some(envoy_config::RuntimeFractionalPercent {
                                default_value: envoy_config::FractionalPercent {
                                    numerator: 100,
                                    denominator: envoy_config::DenominatorType::Hundred,
                                },
                                runtime_key: Some("gate.k".to_string()),
                            }),
                            "gated",
                        ),
                        mk_route("fallback", None, "fallback"),
                    ],
                }],
            })),
            runtime,
        }
    }

    /// The gate at call site 1 of 2 (`resolve_route_in`, the `.position(`):
    /// key "0" -> the gated route NEVER matches; first-match-wins falls to the
    /// catch-all. Key "100" -> the gated route matches.
    #[tokio::test]
    async fn resolve_route_honors_runtime_fraction_gate() {
        let config = gated_route_test_config(Some("0")).await;
        let req = make_req("/x", "localhost");
        let r = resolve_route(&config, &req).expect("catch-all resolves");
        assert!(
            matches!(&r.route().action, RouteAction::DirectResponse(dr) if dr.body.inline_string.as_deref() == Some("fallback")),
            "key 0 must skip the gated route"
        );
        let config = gated_route_test_config(Some("100")).await;
        let r = resolve_route(&config, &req).expect("gated resolves");
        assert!(
            matches!(&r.route().action, RouteAction::DirectResponse(dr) if dr.body.inline_string.as_deref() == Some("gated")),
            "key 100 must match the gated route"
        );
        // Absent key -> default_value (numerator 100) -> gated.
        let config = gated_route_test_config(None).await;
        let r = resolve_route(&config, &req).expect("resolves");
        assert!(
            matches!(&r.route().action, RouteAction::DirectResponse(dr) if dr.body.inline_string.as_deref() == Some("gated")),
            "absent key must honor default_value 100"
        );
    }

    /// The gate at call site 2 of 2 (`build_response_in`, the `.find(`):
    /// the SAME table through build_response — the documented resolve/build
    /// equivalence (hcm.rs "the 30-fixture regression-equivalence guarantee")
    /// now includes the gate.
    #[tokio::test]
    async fn build_response_honors_runtime_fraction_gate() {
        let config = gated_route_test_config(Some("0")).await;
        let mut req = make_req("/x", "localhost");
        match build_response(&config, &mut req, true) {
            BuildOutcome::Synth(resp, _) => assert_eq!(
                std::str::from_utf8(&resp.body).unwrap(),
                "fallback",
                "key 0 must serve the catch-all body"
            ),
            _other => panic!("expected BuildOutcome::Synth"),
        }
        let config = gated_route_test_config(Some("100")).await;
        let mut req = make_req("/x", "localhost");
        match build_response(&config, &mut req, true) {
            BuildOutcome::Synth(resp, _) => assert_eq!(
                std::str::from_utf8(&resp.body).unwrap(),
                "gated",
                "key 100 must serve the gated body"
            ),
            _other => panic!("expected BuildOutcome::Synth"),
        }
    }

    /// 30 Task 6: a config with a `RouteAction::Route` whose `action` carries an
    /// optional `metadata_match`. Used to assert `build_response` surfaces the
    /// route's `envoy.lb` map into `BuildOutcome::Proxy.subset_match`.
    async fn subset_match_test_config(metadata_match: Option<LbMetadata>) -> HCMConfig {
        HCMConfig {
            stat_prefix: "test".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("test"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".into(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        }
    }

    /// Config fixture for the `:path`-mutation tests: ONE route, `prefix`- or
    /// `path`-matched as the caller chooses, whose action is a redirect built
    /// from `rd`.
    async fn redirect_route_config(prefix: &str, rd: RedirectAction) -> HCMConfig {
        HCMConfig {
            stat_prefix: "test".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("test"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some(prefix.to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Redirect(rd),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        }
    }

    /// 76.2 T6-1: `prefix_rewrite` MUTATES the request's `:path` in place, so
    /// the access-log record — built from `req.path` AFTER `build_response`
    /// returns — observes the rewrite. MEASURED upstream: request
    /// `/e-pfx/sub` on a `prefix_rewrite: "/replaced"` route logs as
    /// `path=/replaced/sub`.
    #[tokio::test]
    async fn build_response_prefix_rewrite_mutates_the_request_path() {
        let rd = RedirectAction {
            prefix_rewrite: Some("/replaced".into()),
            ..Default::default()
        };
        let config = redirect_route_config("/e-pfx", rd).await;
        let mut req = make_req("/e-pfx/sub", "envoy-rust.test");
        let outcome = build_response(&config, &mut req, true);
        assert!(matches!(outcome, BuildOutcome::Synth(ref r, _) if r.status == 301));
        assert_eq!(
            req.path, "/replaced/sub",
            "prefix_rewrite must rewrite the request's own :path in place"
        );
    }

    /// 76.2 T6-2: the OTHER HALF of the asymmetry — `path_redirect` changes the
    /// `location` only and MUST NOT touch the request's `:path`. MEASURED
    /// upstream: `/c-pathr/sub` is logged unchanged.
    #[tokio::test]
    async fn build_response_path_redirect_leaves_the_request_path_alone() {
        let rd = RedirectAction {
            path_redirect: Some("/newpath".into()),
            ..Default::default()
        };
        let config = redirect_route_config("/c-pathr", rd).await;
        let mut req = make_req("/c-pathr/sub", "envoy-rust.test");
        match build_response(&config, &mut req, true) {
            BuildOutcome::Synth(resp, _) => {
                assert_eq!(
                    resp.headers
                        .iter()
                        .find(|(n, _)| n == "location")
                        .map(|(_, v)| v.as_str()),
                    Some("http://envoy-rust.test/newpath"),
                );
            }
            _other => panic!("expected BuildOutcome::Synth"),
        }
        assert_eq!(
            req.path, "/c-pathr/sub",
            "path_redirect must NOT touch the request's :path"
        );
    }

    /// Config fixture for the redirect dispatch tests: one `prefix: "/"` route
    /// whose action is `RouteAction::Redirect` with `https_redirect: true`.
    async fn redirect_placeholder_config() -> HCMConfig {
        HCMConfig {
            stat_prefix: "test".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("test"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::Redirect(RedirectAction {
                            https_redirect: Some(true),
                            ..Default::default()
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        }
    }

    /// 76.2 T5-1: THE DELIBERATE FLIP of 76.1's T-C9.
    ///
    /// 76.1 shipped the `redirect:` CONFIG surface with an honest `synth_501`
    /// not-implemented placeholder at the dispatch arm, pinned by a test named
    /// `build_response_redirect_is_not_implemented_placeholder` whose doc block
    /// said "76.2 MUST flip this test". This is that flip: the arm now serves a
    /// real 301 carrying a `location:` header. The rename is the point — the
    /// replacement is a visible, named change rather than an unobserved
    /// behaviour shift.
    ///
    /// Also pins the access-log observable: `%RESPONSE_CODE_DETAILS%` for a
    /// redirect is `direct_response` (MEASURED — the SAME string upstream uses
    /// for a `direct_response:` route, and the same bare literal envoy-rust
    /// already emits), so 76.2 adds NO new detail string, `Op` or
    /// `AccessLogRecord` field.
    #[tokio::test]
    async fn build_response_redirect_emits_301_and_location() {
        let config = redirect_placeholder_config().await;
        let mut req = make_req("/foo", "localhost");
        match build_response(&config, &mut req, true) {
            BuildOutcome::Synth(resp, detail) => {
                assert_eq!(resp.status, 301, "the default response_code is 301");
                assert_eq!(
                    resp.headers
                        .iter()
                        .find(|(n, _)| n == "location")
                        .map(|(_, v)| v.as_str()),
                    Some("https://localhost/foo"),
                    "https_redirect:true forces the scheme; the authority comes \
                     from the Host header"
                );
                assert!(
                    !resp.headers.iter().any(|(n, _)| n == "content-type"),
                    "a redirect MUST NOT carry content-type"
                );
                assert_eq!(
                    detail,
                    Some("direct_response"),
                    "MEASURED: %RESPONSE_CODE_DETAILS% for a redirect is \
                     `direct_response`"
                );
            }
            _other => panic!("expected BuildOutcome::Synth(301)"),
        }
    }

    #[tokio::test]
    async fn build_response_subset_match_populated_from_metadata_match() {
        // Route WITH metadata_match → BuildOutcome::Proxy.subset_match == Some(map).
        let mut envoy_lb = std::collections::BTreeMap::new();
        envoy_lb.insert("stage".to_string(), "canary".to_string());
        envoy_lb.insert("version".to_string(), "v2".to_string());
        let config = subset_match_test_config(Some(LbMetadata {
            envoy_lb: envoy_lb.clone(),
        }))
        .await;
        let mut req = make_req("/foo", "localhost");
        match build_response(&config, &mut req, true) {
            BuildOutcome::Proxy { subset_match, .. } => {
                assert_eq!(
                    subset_match,
                    Some(envoy_lb),
                    "subset_match must mirror the route's metadata_match envoy.lb map"
                );
            }
            _other => panic!("expected BuildOutcome::Proxy"),
        }
    }

    #[tokio::test]
    async fn build_response_subset_match_none_without_metadata_match() {
        // Route WITHOUT metadata_match → subset_match == None (the no-subset no-op).
        let config = subset_match_test_config(None).await;
        let mut req = make_req("/foo", "localhost");
        match build_response(&config, &mut req, true) {
            BuildOutcome::Proxy { subset_match, .. } => {
                assert_eq!(
                    subset_match, None,
                    "no metadata_match must yield subset_match == None"
                );
            }
            _other => panic!("expected BuildOutcome::Proxy"),
        }
    }

    #[tokio::test]
    async fn resolve_route_matches_vh_and_route() {
        let config = resolve_route_test_config().await;
        let req = make_req("/healthz", "localhost");
        let route = resolve_route(&config, &req).expect("route resolves");
        assert!(
            matches!(route.route().action, RouteAction::DirectResponse(_)),
            "expected DirectResponse action"
        );
    }

    #[tokio::test]
    async fn resolve_route_none_on_empty_host() {
        let config = resolve_route_test_config().await;
        // No Host header → None (mirrors build_response's 400 path)
        let req = make_req("/", "");
        assert!(
            resolve_route(&config, &req).is_none(),
            "empty host must yield None"
        );
    }

    #[tokio::test]
    async fn resolve_route_none_on_no_route_match() {
        // Config with a specific path-only route; a request for a different path
        // must return None (no matching route).
        let config = HCMConfig {
            stat_prefix: "test".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("test"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "specific".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: None,
                            path: Some("/exact".to_string()),
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("hit".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        };
        let req = make_req("/other", "localhost");
        assert!(
            resolve_route(&config, &req).is_none(),
            "no matching route must yield None"
        );
    }

    #[tokio::test]
    async fn resolve_route_strips_port_from_host() {
        // VH matches only "myhost"; request with "myhost:8080" must still resolve.
        let config = HCMConfig {
            stat_prefix: "test".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
            http2_protocol_options: None,
            stats: mk_stats("test"),
            access_log: vec![],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "myhost_vh".to_string(),
                    domains: vec!["myhost".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                            runtime_fraction: None,
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("hit".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        };
        let req = make_req("/foo", "myhost:8080");
        let route = resolve_route(&config, &req).expect("port-stripped host must match");
        assert!(matches!(
            route.route().action,
            RouteAction::DirectResponse(_)
        ));
    }

    // ---------------------------------------------------------------------------
    // RouteConfiguration::clone: regression test for typed_per_filter_config
    // preservation (guards field-exhaustive cloning — HCMConfig::new snapshots
    // the loaded route table via `.clone()` into its RwLock<Arc<_>> handle).
    // ---------------------------------------------------------------------------

    #[test]
    fn route_config_clone_preserves_typed_per_filter_config() {
        use std::collections::BTreeMap;

        // Build a Route that carries a non-empty typed_per_filter_config (CORS).
        let cors_policy = envoy_config::CorsPolicy {
            allow_origin_string_match: vec![envoy_config::StringMatcher {
                mode: envoy_config::StringMatcherMode::Exact("http://a.test".to_string()),
                ignore_case: false,
            }],
            allow_methods: None,
            allow_headers: None,
            expose_headers: None,
            max_age: None,
            allow_credentials: None,
        };
        let mut tpfc: BTreeMap<String, envoy_config::PerFilterConfig> = BTreeMap::new();
        tpfc.insert(
            "envoy.filters.http.cors".to_string(),
            envoy_config::PerFilterConfig::Cors(cors_policy),
        );

        let route = Route {
            name: String::new(),
            r#match: RouteMatch {
                prefix: Some("/".to_string()),
                path: None,
                headers: vec![],
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok".to_string()),
                },
            }),
            typed_per_filter_config: tpfc,
        };

        assert!(
            !route.typed_per_filter_config.is_empty(),
            "precondition: source route must have typed_per_filter_config"
        );

        let rc = RouteConfiguration {
            name: "local_route".to_string(),
            validate_clusters: None,
            virtual_hosts: vec![VirtualHost {
                name: "default".to_string(),
                domains: vec!["*".to_string()],
                include_attempt_count_in_response: false,
                routes: vec![route],
            }],
        };

        let cloned = rc.clone();

        assert!(
            !cloned.virtual_hosts[0].routes[0]
                .typed_per_filter_config
                .is_empty(),
            "RouteConfiguration::clone must preserve typed_per_filter_config (was dropped)"
        );
        assert!(
            cloned.virtual_hosts[0].routes[0]
                .typed_per_filter_config
                .contains_key("envoy.filters.http.cors"),
            "RouteConfiguration::clone must preserve the cors key in typed_per_filter_config"
        );
    }

    // ---------------------------------------------------------------------------
    // Phase-26 Task 2: route-table handle (RwLock<Arc<RouteConfiguration>>)
    // swap semantics — the §5.4 read-once guarantee.
    // ---------------------------------------------------------------------------

    /// Helper: a minimal single-vhost RouteConfiguration whose vhost name is
    /// `name` (so a test can tell two route tables apart by identity).
    fn named_route_config(name: &str) -> RouteConfiguration {
        RouteConfiguration {
            name: name.to_string(),
            validate_clusters: None,
            virtual_hosts: vec![VirtualHost {
                name: name.to_string(),
                domains: vec!["*".to_string()],
                include_attempt_count_in_response: false,
                routes: vec![Route {
                    name: String::new(),
                    r#match: RouteMatch {
                        prefix: Some("/".to_string()),
                        path: None,
                        headers: vec![],
                        runtime_fraction: None,
                    },
                    action: RouteAction::DirectResponse(DirectResponse {
                        status: 200,
                        body: DataSource {
                            filename: None,
                            inline_string: Some("ok".to_string()),
                        },
                    }),
                    typed_per_filter_config: Default::default(),
                }],
            }],
        }
    }

    #[tokio::test]
    async fn route_table_handle_swap_is_read_once() {
        // Build an HCMConfig whose route table starts as "v1".
        let config = build_test_config(vec![Route {
            name: String::new(),
            r#match: RouteMatch {
                prefix: Some("/".to_string()),
                path: None,
                headers: vec![],
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("ok".to_string()),
                },
            }),
            typed_per_filter_config: Default::default(),
        }])
        .await;
        // Seed a known v1 table via the writer so we control the name.
        config.store_route_config(Arc::new(named_route_config("v1")));

        // (a) A request reads the CURRENT Arc once at entry. Capture that snapshot.
        let snapshot = config.current_route_config();
        assert_eq!(snapshot.name, "v1", "reader must observe the current table");

        // (b) A store(new_arc) is visible to the NEXT read.
        config.store_route_config(Arc::new(named_route_config("v2")));
        let next = config.current_route_config();
        assert_eq!(
            next.name, "v2",
            "the next read must observe the stored table"
        );

        // (c) The in-flight reader keeps its snapshot — the §5.4 read-once
        // guarantee. `snapshot` was an OWNED Arc clone taken before the store,
        // so the swap does NOT mutate it out from under the in-flight reader.
        assert_eq!(
            snapshot.name, "v1",
            "an in-flight reader's owned snapshot is unaffected by a later store (§5.4)"
        );
    }

    /// Phase 75.1 (ADR-0159): the MODE-SCOPED absence rule propagates to the
    /// ROUTE walker — call site 1 of 5, and the one this sub-phase's
    /// differential fixture 0083 witnesses cross-proxy. `route_matches`
    /// AND-combines the route's HeaderMatchers, so a matcher that must now
    /// return `false` on an absent header must make the whole route not match.
    #[test]
    fn route_header_matcher_absence_rule_is_mode_scoped() {
        use envoy_config::{HeaderMatcher, HeaderMatcherMode};

        let route = |mode: HeaderMatcherMode, invert: bool| Route {
            name: "r".to_string(),
            r#match: RouteMatch {
                prefix: Some("/".to_string()),
                path: None,
                headers: vec![HeaderMatcher {
                    name: "x-a".to_string(),
                    mode,
                    invert_match: invert,
                }],
                runtime_fraction: None,
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("hi".to_string()),
                },
            }),
            typed_per_filter_config: Default::default(),
        };
        let present = [("x-a".to_string(), "zzz".to_string())];
        let absent: [(String, String); 0] = [];

        // D1: a VALUE matcher + invert + ABSENT must NOT match the route.
        let r = route(HeaderMatcherMode::ExactMatch("v".into()), true);
        assert!(
            route_matches(&r, "/x", &present, &RuntimeSnapshot::default()),
            "value+invert, present non-matching value → route matches"
        );
        assert!(
            !route_matches(&r, "/x", &absent, &RuntimeSnapshot::default()),
            "value+invert, ABSENT → route must NOT match (D1 / CF-72-1 closed)"
        );

        // D2: a plain, NON-inverted `present_match: false` requires ABSENCE.
        let r = route(HeaderMatcherMode::PresentMatch(false), false);
        assert!(
            !route_matches(&r, "/x", &present, &RuntimeSnapshot::default()),
            "present_match:false with the header PRESENT → route must NOT match (D2)"
        );
        assert!(
            route_matches(&r, "/x", &absent, &RuntimeSnapshot::default()),
            "present_match:false with the header ABSENT → route matches"
        );

        // P1 THE GUARD: `present_match: true` + invert + ABSENT still matches.
        let r = route(HeaderMatcherMode::PresentMatch(true), true);
        assert!(
            route_matches(&r, "/x", &absent, &RuntimeSnapshot::default()),
            "present_match:true+invert, ABSENT → route STILL matches (P1 parity)"
        );
        assert!(
            !route_matches(&r, "/x", &present, &RuntimeSnapshot::default()),
            "present_match:true+invert, PRESENT → route does not match"
        );
    }

    /// Phase 75.1 (ADR-0159): the MODE-SCOPED absence rule propagates to the
    /// access-log `header_filter` — call site 5 of 5, and the only one reached
    /// through the ADR-0150 `Arc<dyn HeaderMatch>` trait object rather than the
    /// inherent method. Compiled end-to-end via `compile_access_log_filter`, so
    /// this exercises the real boxing the runtime performs, not a stub.
    ///
    /// The CROSS-PROXY witness for this call site is sub-phase 75.2 (fixtures
    /// 0084 + 0085); this in-process pin is what makes 75.1 a complete slice.
    #[test]
    fn access_log_header_filter_absence_rule_is_mode_scoped_through_the_seam() {
        use envoy_config::HeaderMatcherMode as M;

        let compile = |mode: M, invert: bool| {
            compile_access_log_filter(&envoy_config::AccessLogFilter {
                status_code_filter: None,
                response_flag_filter: None,
                header_filter: Some(envoy_config::HeaderFilter {
                    header: envoy_config::HeaderMatcher {
                        name: "x-a".into(),
                        mode,
                        invert_match: invert,
                    },
                }),
                and_filter: None,
                or_filter: None,
                metadata_filter: None,
            })
        };
        let present = [("x-a".to_string(), "zzz".to_string())];
        let absent: [(String, String); 0] = [];

        // D1: value matcher + invert + ABSENT → the record is now DROPPED.
        let f = compile(M::ExactMatch("v".into()), true);
        assert!(
            f.should_log(200, "-", &present, &Default::default()),
            "value+invert, present non-matching → KEEP"
        );
        assert!(
            !f.should_log(200, "-", &absent, &Default::default()),
            "value+invert, ABSENT → DROP (D1 / CF-72-1 closed) — this is the \
             divergence fixture 0078's README recorded as deferred"
        );

        // D2: plain `present_match: false` requires ABSENCE.
        let f = compile(M::PresentMatch(false), false);
        assert!(
            !f.should_log(200, "-", &present, &Default::default()),
            "present_match:false, PRESENT → DROP (D2)"
        );
        assert!(
            f.should_log(200, "-", &absent, &Default::default()),
            "present_match:false, ABSENT → KEEP"
        );

        // P1 THE GUARD — must stay KEEP through the seam.
        let f = compile(M::PresentMatch(true), true);
        assert!(
            f.should_log(200, "-", &absent, &Default::default()),
            "present_match:true+invert, ABSENT → STILL KEEP (P1 parity)"
        );
        assert!(
            !f.should_log(200, "-", &present, &Default::default()),
            "present_match:true+invert, PRESENT → DROP"
        );

        // An EMPTY VALUE counts as PRESENT through the seam too.
        let empty = [("x-a".to_string(), String::new())];
        assert!(
            !compile(M::PresentMatch(false), false).should_log(
                200,
                "-",
                &empty,
                &Default::default()
            ),
            "an EMPTY header value is PRESENT, so present_match:false DROPs"
        );
    }

    /// 76.2 T3-1: the MEASURED `location` table. One row per upstream cell
    /// measured against `envoyproxy/envoy:v1.33.0` (SPEC 2.3 — R1-R16, Q1-Q4,
    /// E1-E2). Table-driven ON PURPOSE: `plan_redirect` is a pure total
    /// function, so a newly measured cell must cost ONE line. Each row carries
    /// its own `label`, so a failure names the exact cell.
    #[test]
    fn plan_redirect_matches_every_measured_location_cell() {
        struct Cell {
            label: &'static str,
            host: &'static str,
            prefix: Option<&'static str>,
            target: &'static str,
            rd: RedirectAction,
            status: u16,
            location: &'static str,
        }
        fn rd(f: impl FnOnce(&mut RedirectAction)) -> RedirectAction {
            let mut r = RedirectAction::default();
            f(&mut r);
            r
        }
        fn cell(
            label: &'static str,
            host: &'static str,
            prefix: &'static str,
            target: &'static str,
            rd: RedirectAction,
            status: u16,
            location: &'static str,
        ) -> Cell {
            Cell {
                label,
                host,
                prefix: Some(prefix),
                target,
                rd,
                status,
                location,
            }
        }
        use envoy_config::RedirectResponseCode as RC;
        let cells = vec![
            cell(
                "R1 host_redirect replaces the authority",
                "envoy-rust.test",
                "/a-host",
                "/a-host",
                rd(|r| r.host_redirect = Some("example.com".into())),
                301,
                "http://example.com/a-host",
            ),
            cell(
                "R2 the query is preserved by default",
                "envoy-rust.test",
                "/b-query",
                "/b-query/deep?a=b",
                rd(|r| r.host_redirect = Some("example.com".into())),
                301,
                "http://example.com/b-query/deep?a=b",
            ),
            cell(
                "R3 path_redirect replaces the path wholesale",
                "envoy-rust.test",
                "/c-pathr",
                "/c-pathr/sub",
                rd(|r| r.path_redirect = Some("/newpath".into())),
                301,
                "http://envoy-rust.test/newpath",
            ),
            cell(
                "R4 path_redirect STILL keeps the query",
                "envoy-rust.test",
                "/d-pathq",
                "/d-pathq/x?k=v",
                rd(|r| r.path_redirect = Some("/newpath".into())),
                301,
                "http://envoy-rust.test/newpath?k=v",
            ),
            cell(
                "R5 prefix_rewrite replaces only the matched span",
                "envoy-rust.test",
                "/e-pfx",
                "/e-pfx/sub",
                rd(|r| r.prefix_rewrite = Some("/replaced".into())),
                301,
                "http://envoy-rust.test/replaced/sub",
            ),
            cell(
                "R6 https_redirect forces the scheme",
                "envoy-rust.test",
                "/f-https",
                "/f-https/x",
                rd(|r| r.https_redirect = Some(true)),
                301,
                "https://envoy-rust.test/f-https/x",
            ),
            cell(
                "R7 response_code TEMPORARY_REDIRECT",
                "envoy-rust.test",
                "/g-c307",
                "/g-c307",
                rd(|r| {
                    r.host_redirect = Some("example.com".into());
                    r.response_code = RC::TemporaryRedirect;
                }),
                307,
                "http://example.com/g-c307",
            ),
            cell(
                "R8 strip_query drops the query",
                "envoy-rust.test",
                "/h-strip",
                "/h-strip/a?q=1&z=2",
                rd(|r| {
                    r.host_redirect = Some("example.com".into());
                    r.strip_query = true;
                }),
                301,
                "http://example.com/h-strip/a",
            ),
            cell(
                "R9 port_redirect alongside host_redirect",
                "envoy-rust.test",
                "/i-port",
                "/i-port",
                rd(|r| {
                    r.host_redirect = Some("example.com".into());
                    r.port_redirect = Some(8443);
                }),
                301,
                "http://example.com:8443/i-port",
            ),
            cell(
                "R10 a bare redirect{} echoes the request",
                "envoy-rust.test",
                "/j-bare",
                "/j-bare/deep",
                RedirectAction::default(),
                301,
                "http://envoy-rust.test/j-bare/deep",
            ),
            cell(
                "R11 scheme_redirect is NOT allow-listed — `ftp` is emitted verbatim",
                "envoy-rust.test",
                "/k-scheme",
                "/k-scheme/x",
                rd(|r| r.scheme_redirect = Some("ftp".into())),
                301,
                "ftp://envoy-rust.test/k-scheme/x",
            ),
            cell(
                "R12 scheme_redirect + host_redirect together",
                "envoy-rust.test",
                "/l-both",
                "/l-both/y",
                rd(|r| {
                    r.scheme_redirect = Some("https".into());
                    r.host_redirect = Some("e.com".into());
                }),
                301,
                "https://e.com/l-both/y",
            ),
            cell(
                "R13 response_code SEE_OTHER + strip_query",
                "envoy-rust.test",
                "/m-see",
                "/m-see/y?q=1",
                rd(|r| {
                    r.host_redirect = Some("e.com".into());
                    r.strip_query = true;
                    r.response_code = RC::SeeOther;
                }),
                303,
                "http://e.com/m-see/y",
            ),
            cell(
                "R14 a scheme change does NOT normalise a redundant :443",
                "envoy-rust.test",
                "/n-hport",
                "/n-hport/y",
                rd(|r| {
                    r.https_redirect = Some(true);
                    r.port_redirect = Some(443);
                }),
                301,
                "https://envoy-rust.test:443/n-hport/y",
            ),
            cell(
                "R15 response_code FOUND",
                "envoy-rust.test",
                "/o-found",
                "/o-found",
                rd(|r| {
                    r.host_redirect = Some("example.com".into());
                    r.response_code = RC::Found;
                }),
                302,
                "http://example.com/o-found",
            ),
            cell(
                "R16 response_code PERMANENT_REDIRECT",
                "envoy-rust.test",
                "/p-perm",
                "/p-perm",
                rd(|r| {
                    r.host_redirect = Some("example.com".into());
                    r.response_code = RC::PermanentRedirect;
                }),
                308,
                "http://example.com/p-perm",
            ),
            cell(
                "Q1 host_redirect UNSET preserves the request's port",
                "envoy-rust.test:1234",
                "/q1-hostport",
                "/q1-hostport/x",
                rd(|r| r.https_redirect = Some(true)),
                301,
                "https://envoy-rust.test:1234/q1-hostport/x",
            ),
            cell(
                "Q2 host_redirect SET DROPS the request's port — the asymmetry",
                "envoy-rust.test:1234",
                "/a-host",
                "/a-host",
                rd(|r| r.host_redirect = Some("example.com".into())),
                301,
                "http://example.com/a-host",
            ),
            cell(
                "Q3 a bare redirect{} preserves the request's port",
                "envoy-rust.test:1234",
                "/q3-hostport",
                "/q3-hostport/d",
                RedirectAction::default(),
                301,
                "http://envoy-rust.test:1234/q3-hostport/d",
            ),
            cell(
                "Q4 port_redirect OVERRIDES the request's port",
                "envoy-rust.test:1234",
                "/n-hport",
                "/n-hport/y",
                rd(|r| {
                    r.https_redirect = Some(true);
                    r.port_redirect = Some(443);
                }),
                301,
                "https://envoy-rust.test:443/n-hport/y",
            ),
            cell(
                "E1 an explicit https_redirect:false is the DEFAULT scheme",
                "envoy-rust.test",
                "/y-hfalse",
                "/y-hfalse/z",
                rd(|r| r.https_redirect = Some(false)),
                301,
                "http://envoy-rust.test/y-hfalse/z",
            ),
            cell(
                "E2 an EMPTY path_redirect performs NO rewrite",
                "envoy-rust.test",
                "/x-emptypath",
                "/x-emptypath/z",
                rd(|r| r.path_redirect = Some(String::new())),
                301,
                "http://envoy-rust.test/x-emptypath/z",
            ),
        ];
        assert_eq!(cells.len(), 22, "all 22 MEASURED cells must be present");
        for c in &cells {
            let plan = plan_redirect(c.host, c.target, c.prefix, &c.rd);
            assert_eq!(plan.location, c.location, "cell {}: location", c.label);
            assert_eq!(plan.status, c.status, "cell {}: status", c.label);
        }
    }

    /// 76.2 T3-2: `prefix_rewrite` is the ONLY arm that reports a rewritten
    /// `:path`. MEASURED: `prefix_rewrite` MUTATES the logged `:path` while
    /// `path_redirect` does NOT.
    #[test]
    fn plan_redirect_reports_a_rewritten_path_only_for_prefix_rewrite() {
        let pfx = RedirectAction {
            prefix_rewrite: Some("/replaced".into()),
            ..Default::default()
        };
        assert_eq!(
            plan_redirect("h.test", "/e-pfx/sub", Some("/e-pfx"), &pfx).rewritten_path,
            Some("/replaced/sub".to_string()),
        );

        let pathr = RedirectAction {
            path_redirect: Some("/newpath".into()),
            ..Default::default()
        };
        assert_eq!(
            plan_redirect("h.test", "/c-pathr/sub", Some("/c-pathr"), &pathr).rewritten_path,
            None,
            "path_redirect must NOT rewrite the request's own :path"
        );

        assert_eq!(
            plan_redirect(
                "h.test",
                "/j-bare/x",
                Some("/j-bare"),
                &RedirectAction::default()
            )
            .rewritten_path,
            None,
            "a bare redirect{{}} rewrites nothing"
        );
    }

    /// 76.2 T3-3: `plan_redirect` is TOTAL — it must not panic on a matched
    /// span longer than the path, nor on one landing off a UTF-8 boundary.
    ///
    /// DEVIATION D-2 from `PLAN.md`: the plan's literal for the second case was
    /// `Some("/é"[..2].into())`, which panics IN THE TEST ITSELF — `"/é"` is
    /// three bytes (`/` = 1, `é` = 2) so byte index 2 is not a char boundary and
    /// slicing there aborts before `plan_redirect` is ever called. A two-byte
    /// ASCII prefix witnesses the same cell honestly: `matched_len` is 2, and
    /// `"/é".get(2..)` lands mid-`é`, returns `None`, and the `unwrap_or("")`
    /// inside `plan_redirect` is what keeps the function total.
    #[test]
    fn plan_redirect_is_total_on_degenerate_spans() {
        let rd = RedirectAction {
            prefix_rewrite: Some("/r".into()),
            ..Default::default()
        };
        // Matched span longer than the path.
        assert_eq!(
            plan_redirect("h.test", "/ab", Some("/abcdefgh"), &rd).location,
            "http://h.test/r"
        );
        // Matched span landing mid-codepoint: `"/é"` is 3 bytes, so a 2-byte
        // span ends inside `é` and `str::get` returns None.
        assert_eq!(
            plan_redirect("h.test", "/\u{e9}", Some("ab"), &rd).location,
            "http://h.test/r"
        );
    }

    /// 76.2 T4-1: a redirect carries EXACTLY five header names and NO
    /// `content-type` — the MEASURED finding that forces a dedicated builder.
    /// Reusing the shared `synth_with` would emit a sixth header upstream does
    /// not, and `diff_headers` would bail on its name-set check with
    /// `only-in-envoy-rust=["content-type"]`.
    #[test]
    fn synth_redirect_emits_five_names_and_no_content_type() {
        let resp = synth_redirect(301, "http://example.com/a".to_string(), true);
        let names: Vec<&str> = resp.headers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec!["location", "date", "server", "connection", "content-length"],
            "measured upstream wire order for a redirect response"
        );
        assert!(
            !names.contains(&"content-type"),
            "a redirect MUST NOT carry content-type"
        );
        assert!(resp.body.is_empty(), "redirect body is empty");
        assert_eq!(
            resp.headers
                .iter()
                .find(|(n, _)| n == "content-length")
                .map(|(_, v)| v.as_str()),
            Some("0"),
            "content-length is compared value-exact by diff_headers"
        );
        assert_eq!(
            resp.headers
                .iter()
                .find(|(n, _)| n == "location")
                .map(|(_, v)| v.as_str()),
            Some("http://example.com/a"),
        );
        assert_eq!(resp.status, 301);
        // 76.2 §5.2 re-entry (REVIEW.md M-3): the reason phrase must be left
        // UNSET, so the wire path falls through to `canonical_reason`
        // (`response.rs`, consulted at the single `reason.unwrap_or_else(…)`
        // site). 76.2 added 303/307/308 to that lookup because they previously
        // fell to `_ => "OK"` and would have emitted `HTTP/1.1 303 OK`.
        //
        // That lookup was pinned only as a PURE FUNCTION — nothing asserted a
        // redirect `Response` actually reaches it, so setting `reason:
        // Some("OK")` here would restore the wrong reason phrase on the wire
        // and survive the entire workspace. The differential fixture cannot
        // catch it either: the harness parses the status CODE only, which SPEC
        // §2.1 flagged as a silent-wrong-answer hazard. This assertion is the
        // link that closes it.
        assert_eq!(
            resp.reason, None,
            "reason must be unset so canonical_reason supplies the measured phrase"
        );
    }

    /// 76.2 §5.2 re-entry — `REVIEW.md` M-1: the two cells this phase
    /// **invented** rather than measured, which `BEHAVIOR_CONTRACT.md` §F items
    /// 7 and 8 record as CHOICES and claim are "pinned by in-process tests".
    /// Before this test that claim was MEASURED FALSE for both, and a refactor
    /// could have silently flipped either cell with a fully green gate.
    ///
    /// They are pinned HERE rather than as rows in
    /// `plan_redirect_matches_every_measured_location_cell`, deliberately: that
    /// table is the 22 cells MEASURED against `envoyproxy/envoy:v1.33.0`, and
    /// folding an invented cell into it would make the table claim upstream
    /// authority it does not have. Keeping them separate also leaves the
    /// table's `cells.len() == 22` guard intact.
    ///
    /// **Neither cell is witnessed by fixture `0086`**, by construction — every
    /// route there is `prefix:`-matched, and the rewritten `:path` is an
    /// access-log observable while `0086` compares responses only. These
    /// assertions are the ONLY thing standing behind either behaviour.
    #[test]
    fn plan_redirect_pins_the_two_invented_cells_contract_f_items_7_and_8() {
        let rd = RedirectAction {
            prefix_rewrite: Some("/replaced".into()),
            ..Default::default()
        };

        // §F item 7 — a `path:`-matched route supplies NO prefix, and
        // envoy-rust's choice is "the matched span is the whole path", i.e. the
        // rewrite replaces the path wholesale and leaves no tail. The 22-row
        // measured table cannot express this: its `cell()` constructor
        // hard-wires `prefix: Some(prefix)`.
        assert_eq!(
            plan_redirect("h.test", "/exact/path", None, &rd).location,
            "http://h.test/replaced",
            "matched_prefix == None means the WHOLE path is the matched span"
        );

        // §F item 8 — the request's query rides along on the REWRITTEN `:path`
        // (an access-log observable), independently of the `location`'s own
        // query suffix. Both are asserted here: nothing else in the tree
        // combines `prefix_rewrite` with a query-bearing target.
        let plan = plan_redirect("h.test", "/e-pfx/sub?k=v", Some("/e-pfx"), &rd);
        assert_eq!(
            plan.rewritten_path,
            Some("/replaced/sub?k=v".to_string()),
            "the query rides along on the rewritten :path"
        );
        assert_eq!(
            plan.location, "http://h.test/replaced/sub?k=v",
            "and the location keeps it too (strip_query defaults false)"
        );
    }

    /// 110.1 seam: a gRPC `content-type` on a request that hits a
    /// `direct_response` route must produce upstream's MEASURED wire shape —
    /// `200`, `content-type: application/grpc`, `grpc-status`, `grpc-message`,
    /// `content-length: 0`, no body — end to end through the real tokio
    /// `serve_connection` funnel, not just through the pure transform.
    #[tokio::test]
    async fn grpc_local_reply_transforms_direct_response() {
        let config = hcm_config_single_route("/", 404, "B404").await;
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let s = String::from_utf8_lossy(&resp);

        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "status line: {s}");
        assert!(s.contains("content-type: application/grpc\r\n"), "ct: {s}");
        assert!(s.contains("grpc-status: 12\r\n"), "grpc-status: {s}");
        assert!(s.contains("grpc-message: B404\r\n"), "grpc-message: {s}");
        assert!(s.contains("content-length: 0\r\n"), "cl: {s}");
        assert!(s.ends_with("\r\n\r\n"), "body must be empty: {s}");
        assert!(
            !s.contains("text/plain"),
            "old content-type must be gone: {s}"
        );
    }

    /// The paired NON-gRPC control on the SAME route: nothing changes. Without
    /// this, a transform that fired unconditionally would still pass the test
    /// above.
    #[tokio::test]
    async fn non_grpc_request_leaves_direct_response_untouched() {
        let config = hcm_config_single_route("/", 404, "B404").await;
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/json\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let s = String::from_utf8_lossy(&resp);

        assert!(s.starts_with("HTTP/1.1 404 Not Found\r\n"), "status: {s}");
        assert!(s.contains("content-type: text/plain\r\n"), "ct: {s}");
        assert!(s.contains("content-length: 4\r\n"), "cl: {s}");
        assert!(!s.contains("grpc-status"), "no grpc-status: {s}");
        assert!(!s.contains("grpc-message"), "no grpc-message: {s}");
        assert!(s.ends_with("\r\nB404"), "body preserved: {s}");
    }

    /// The HCM's OWN unmatched-path 404 — an empty-body local reply that does
    /// NOT come from a `direct_response`. MEASURED upstream: `grpc-status: 12`
    /// and NO `grpc-message` header at all.
    #[tokio::test]
    async fn grpc_local_reply_transforms_route_not_found_without_grpc_message() {
        let config = hcm_config_single_route("/only-this", 200, "ok").await;
        let req = b"GET /nope HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let s = String::from_utf8_lossy(&resp);

        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "status: {s}");
        assert!(s.contains("grpc-status: 12\r\n"), "grpc-status: {s}");
        assert!(
            !s.contains("grpc-message"),
            "grpc-message must be ABSENT: {s}"
        );
        assert!(s.contains("content-length: 0\r\n"), "cl: {s}");
    }

    /// The detection edges, driven through the REAL funnel rather than the pure
    /// function, so a seam that (say) lower-cased the value before matching
    /// would be caught here even though Task 1's unit tests pass.
    #[tokio::test]
    async fn grpc_detection_edges_hold_through_the_seam() {
        for (ct, transformed) in [
            ("application/grpc", true),
            ("application/grpc+proto", true),
            ("application/grpc+", true),
            ("application/grpc; charset=utf-8", false),
            ("APPLICATION/GRPC", false),
            ("application/grpc-web", false),
            ("application/grpcfoo", false),
        ] {
            let config = hcm_config_single_route("/", 404, "B404").await;
            let req = format!(
                "GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: {ct}\r\nConnection: close\r\n\r\n"
            );
            let resp = drive(config, req.as_bytes()).await;
            let s = String::from_utf8_lossy(&resp);
            if transformed {
                assert!(
                    s.starts_with("HTTP/1.1 200 OK\r\n"),
                    "{ct} must transform: {s}"
                );
                assert!(s.contains("grpc-status: 12\r\n"), "{ct}: {s}");
            } else {
                assert!(
                    s.starts_with("HTTP/1.1 404 Not Found\r\n"),
                    "{ct} must NOT transform: {s}"
                );
                assert!(!s.contains("grpc-status"), "{ct}: {s}");
            }
        }
    }

    /// The MEASURED header ORDER, through the real funnel, byte-exact.
    ///
    /// Order is a HOUSE-CONVENTION concern, not a differential one: the
    /// harness's `diff_headers` compares a `BTreeSet` of lower-cased header
    /// NAMES plus exact VALUES outside the 3-entry `HEADER_ALLOW_LIST`, and
    /// never reads order. A wrong order therefore fails THIS test, not a
    /// fixture — which is exactly why this test has to exist.
    #[tokio::test]
    async fn grpc_local_reply_header_order_matches_upstream() {
        let config = hcm_config_single_route("/", 503, "B503").await;
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let s = String::from_utf8_lossy(&resp);
        let head = s.split("\r\n\r\n").next().unwrap_or_default();
        let order: Vec<&str> = head
            .lines()
            .skip(1)
            .filter_map(|l| l.split(':').next())
            .collect();
        assert_eq!(
            order,
            vec![
                "content-type",
                "grpc-status",
                "grpc-message",
                "date",
                "server",
                "connection",
                "content-length",
            ],
            "MEASURED upstream order: {s}"
        );
    }
}
