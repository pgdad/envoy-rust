//! io_uring data-plane worker — EXPERIMENTAL PROTOTYPE (perf evaluation).
//!
//! One monoio (io_uring) single-threaded runtime per worker, each owning an
//! `SO_REUSEPORT` accept socket — the thread-per-core layout of `envoy-bin`'s
//! TPC mode, with the epoll/tokio data plane replaced by io_uring submission.
//! Compiled only under `--features uring` on Linux and engaged only when
//! `ENVOY_RUST_URING=1` AND the listener meets the direct-path gates
//! (H1 codec, Router-only chain, no access log, no rds, no downstream TLS).
//!
//! Byte-parity strategy: every wire byte is produced by the SAME serializers
//! the tokio path uses — `build_request_wire`, `parse_response_head` +
//! `serialize_direct_head`, `serialize_response_head` (synth responses), the
//! `VECTORED_BODY_THRESHOLD` coalescing rule — and routing/validation runs
//! through the SAME `build_response`. Counters tick at the same logical
//! points (downstream_rq_total at parse-success, per-class after write,
//! upstream_rq_total on a received response, record_response per attempt).
//!
//! PROTOTYPE LIMITATIONS (why this is env-gated off by default):
//! - retry policies are not honored (single attempt; the eligible bench
//!   config has none),
//! - chunked upstream responses are not proxied (503 + connection close;
//!   the tokio direct path falls back to the owned proxy arm),
//! - the per-worker upstream pool has no idle sweeper and does not tick
//!   `upstream_cx_http1_total` / pool overflow counters,
//! - the filter pipeline is not invoked (gated Router-only, where decode/
//!   encode are no-ops and route config application is stateless),
//! - graceful drain is coarse: on shutdown the accept loop exits and
//!   in-flight connections are dropped with the runtime.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use monoio::io::{AsyncReadRent, AsyncWriteRentExt};
use monoio::net::{TcpListener, TcpStream};

use crate::client::{DirectHead, build_request_wire, parse_response_head, serialize_direct_head};
use crate::codec::{Http1Codec, HttpVersion, Request};
use crate::error::Http1Error;
use crate::hcm::{
    BuildOutcome, HCMConfig, IDLE_READ_TIMEOUT, READ_BUFFER_INITIAL_CAPACITY, parse_content_length,
    synth_501, synth_no_healthy_upstream, synth_status,
};
use crate::headers as hdr;
use crate::response::{Response, VECTORED_BODY_THRESHOLD, serialize_response_head};

/// Everything a worker thread needs to serve one listener shard on io_uring.
pub struct UringWorker {
    pub addr: SocketAddr,
    /// Listener name for the `listener.<name>.*` stat family.
    pub listener_name: String,
    /// Per-worker HCMConfig (route table + cluster_mgr + HCM stats). The
    /// worker keeps its own upstream pool; `config.pool_mgr` is unused here.
    pub config: Arc<HCMConfig>,
    pub registry: Arc<envoy_stats::StatsRegistry>,
    pub token: tokio_util::sync::CancellationToken,
}

fn io_other<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

/// Entry point: build the io_uring runtime on the CURRENT thread and serve
/// until the cancellation token fires. Called from a dedicated
/// `std::thread` per worker (mirrors the TPC tokio worker).
pub fn run_worker(w: UringWorker) -> std::io::Result<()> {
    let mut rt = monoio::RuntimeBuilder::<monoio::IoUringDriver>::new()
        .enable_timer()
        .build()
        .map_err(io_other)?;
    rt.block_on(serve(w))
}

async fn serve(w: UringWorker) -> std::io::Result<()> {
    // SO_REUSEPORT accept socket via socket2 (same options as
    // envoy-listener's bind_reuseport_socket), then into monoio.
    let listener = {
        use socket2::{Domain, Protocol, Socket, Type};
        let domain = if w.addr.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let sock = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
        sock.set_reuse_address(true)?;
        sock.set_reuse_port(true)?;
        sock.set_nonblocking(true)?;
        sock.bind(&w.addr.into())?;
        sock.listen(1024)?;
        TcpListener::from_std(sock.into())?
    };

    let cx_total = w
        .registry
        .register_counter(&format!("listener.{}.downstream_cx_total", w.listener_name))
        .map_err(io_other)?;
    let cx_active = w
        .registry
        .register_gauge(&format!(
            "listener.{}.downstream_cx_active",
            w.listener_name
        ))
        .map_err(io_other)?;
    let cx_accept_failed = w
        .registry
        .register_counter(&format!(
            "listener.{}.downstream_cx_accept_failed",
            w.listener_name
        ))
        .map_err(io_other)?;

    loop {
        monoio::select! {
            _ = w.token.cancelled() => return Ok(()),
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _peer)) => {
                        cx_total.inc();
                        cx_active.inc();
                        let _ = stream.set_nodelay(true);
                        let config = Arc::clone(&w.config);
                        let cx_active = Arc::clone(&cx_active);
                        monoio::spawn(async move {
                            if let Err(err) = serve_conn(config, stream).await {
                                tracing::debug!(error = %err, "uring connection ended with error");
                            }
                            cx_active.dec();
                        });
                    }
                    Err(err) => {
                        cx_accept_failed.inc();
                        tracing::warn!(error = %err, "uring accept failed; continuing");
                    }
                }
            }
        }
    }
}

/// Per-worker upstream connection pool: keep-alive streams keyed by endpoint.
/// Single-threaded (one pool per worker runtime) — no locking.
#[derive(Default)]
struct UpPool {
    idle: HashMap<SocketAddr, Vec<UpConn>>,
}

struct UpConn {
    stream: TcpStream,
    /// Response read-accumulation buffer (reused across requests).
    buf: Vec<u8>,
    /// Request wire buffer (reused across requests).
    wire: Vec<u8>,
    /// Rented scratch buffer for upstream reads (reused across requests).
    scratch: Vec<u8>,
}

impl UpPool {
    async fn acquire(
        &mut self,
        endpoint: SocketAddr,
        cluster: &envoy_cluster::ClusterHandle,
    ) -> std::io::Result<UpConn> {
        if let Some(list) = self.idle.get_mut(&endpoint)
            && let Some(conn) = list.pop()
        {
            return Ok(conn);
        }
        let stream = TcpStream::connect(endpoint).await?;
        let _ = stream.set_nodelay(true);
        // Same tick points as the tokio pool's connect-on-miss branch
        // (upstream_cx_active is inc'd here / dec'd on drop-or-release-… —
        // prototype: inc on connect, dec when the conn is discarded).
        cluster.cx_total().inc();
        Ok(UpConn {
            stream,
            buf: Vec::with_capacity(4096),
            wire: Vec::with_capacity(256),
            scratch: Vec::with_capacity(4096),
        })
    }

    fn release(&mut self, endpoint: SocketAddr, conn: UpConn) {
        self.idle.entry(endpoint).or_default().push(conn);
    }
}

/// Outcome of one direct upstream exchange.
enum ExchangeOk {
    /// Head serialized into the caller's buffer; body returned.
    Direct {
        status: u16,
        upstream_close: bool,
        body: Vec<u8>,
    },
}

async fn serve_conn(config: Arc<HCMConfig>, mut down: TcpStream) -> Result<(), Http1Error> {
    let mut pending: Vec<u8> = Vec::with_capacity(READ_BUFFER_INITIAL_CAPACITY);
    let mut rbuf: Vec<u8> = Vec::with_capacity(READ_BUFFER_INITIAL_CAPACITY);
    let mut head_buf: Vec<u8> = Vec::new();
    let mut write_buf: Vec<u8> = Vec::with_capacity(READ_BUFFER_INITIAL_CAPACITY);
    let mut pool = UpPool::default();

    loop {
        // 1. Parse a complete request head out of `pending`, reading more as
        //    needed (idle timeout → clean close; EOF with empty buffer →
        //    clean close). Mirrors hcm::serve_connection step 1/2.
        let req: Request = loop {
            match Http1Codec::parse_request(&pending)? {
                Some(r) => break r,
                None => {
                    rbuf.clear();
                    let read = monoio::time::timeout(IDLE_READ_TIMEOUT, down.read(rbuf));
                    match read.await {
                        Ok((res, b)) => {
                            rbuf = b;
                            let n = res.map_err(|source| Http1Error::Io { source })?;
                            if n == 0 {
                                if pending.is_empty() {
                                    return Ok(());
                                }
                                return Err(Http1Error::UnexpectedEof);
                            }
                            pending.extend_from_slice(&rbuf[..n]);
                        }
                        Err(_elapsed) => {
                            // The rented buffer was consumed by the cancelled
                            // read; replace it (we return anyway).
                            return Ok(());
                        }
                    }
                }
            }
        };

        config.stats.downstream_rq_total.inc();

        let close = req.headers.iter().any(|(n, v)| {
            n.eq_ignore_ascii_case(hdr::CONNECTION) && v.eq_ignore_ascii_case("close")
        }) || req.version == HttpVersion::Http10;

        let body_len = parse_content_length(&req.headers)?;
        let chunked = req.headers.iter().any(|(n, v)| {
            n.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked")
        });

        // 2. Consume the head; read the CL-framed request body (if any).
        let mut req = req;
        pending.drain(..req.bytes_consumed);
        let request_body: bytes::Bytes = if body_len > 0 {
            let mut body_buf = Vec::with_capacity(body_len.min(64 * 1024));
            let from_buf = pending.len().min(body_len);
            body_buf.extend_from_slice(&pending[..from_buf]);
            pending.drain(..from_buf);
            while body_buf.len() < body_len {
                rbuf.clear();
                match monoio::time::timeout(IDLE_READ_TIMEOUT, down.read(rbuf)).await {
                    Ok((res, b)) => {
                        rbuf = b;
                        let n = res.map_err(|source| Http1Error::Io { source })?;
                        if n == 0 {
                            return Err(Http1Error::UnexpectedEof);
                        }
                        let need = body_len - body_buf.len();
                        let take = n.min(need);
                        body_buf.extend_from_slice(&rbuf[..take]);
                        // Excess bytes beyond the body belong to the next
                        // pipelined request.
                        pending.extend_from_slice(&rbuf[take..n]);
                    }
                    Err(_elapsed) => return Ok(()),
                }
            }
            bytes::Bytes::from(body_buf)
        } else {
            bytes::Bytes::new()
        };
        req.body = Some(request_body);

        // 3. Build the response decision through the SAME routing/validation
        //    as the tokio path (400/404/501/direct-response synths + Proxy).
        //    The worker is gated Router-only, so the filter pipeline's
        //    decode/encode passes are no-ops and are skipped here.
        let outcome = if chunked {
            BuildOutcome::Synth(synth_501(close), None)
        } else {
            crate::hcm::build_response(&config, &mut req, close)
        };

        match outcome {
            BuildOutcome::Synth(mut resp, _details) => {
                write_owned(&mut down, &mut resp, &req.headers, &mut write_buf).await?;
                tick_class(&config, resp.status);
            }
            BuildOutcome::Proxy {
                cluster: cluster_name,
                retry_config: _, // prototype: single attempt (see module docs)
                include_attempt_count_in_response,
                request_hash_key,
                subset_match,
            } => {
                let cluster = config
                    .cluster_mgr
                    .get(&cluster_name)
                    .expect("validator ensures cluster present");
                let host_header = hdr::find_header(&req.headers, hdr::HOST)
                    .expect("build_response rejected missing/empty Host before Proxy")
                    .to_owned();

                let Some(endpoint) = cluster.pick_endpoint(request_hash_key, subset_match.as_ref())
                else {
                    let mut resp = synth_no_healthy_upstream(close);
                    write_owned(&mut down, &mut resp, &req.headers, &mut write_buf).await?;
                    tick_class(&config, resp.status);
                    if close {
                        return Ok(());
                    }
                    continue;
                };

                // The vhost attempt-count header forces the owned path in
                // tokio; the uring worker is only engaged when it is off for
                // the matched vhost's routes — emit it anyway for parity if
                // a route flips it (single attempt → "1").
                let start_ms = crate::date::coarse_monotonic_ms();

                let mut up = match pool.acquire(endpoint, &cluster).await {
                    Ok(up) => up,
                    Err(source) => {
                        tracing::warn!(
                            cluster = %cluster.name(),
                            addr = %endpoint,
                            error = ?source,
                            "uring upstream connect failed — returning 503",
                        );
                        let mut resp = synth_status(503, close);
                        cluster.record_response(endpoint, resp.status);
                        write_owned(&mut down, &mut resp, &req.headers, &mut write_buf).await?;
                        tick_class(&config, resp.status);
                        if close {
                            return Ok(());
                        }
                        continue;
                    }
                };

                match direct_exchange(&mut up, &req, &host_header, start_ms, close, &mut head_buf)
                    .await
                {
                    Ok(ExchangeOk::Direct {
                        status,
                        upstream_close,
                        body,
                    }) => {
                        // Same post-attempt ticks as hcm's proxy arm.
                        cluster.upstream_rq_total().inc();
                        cluster.record_response(endpoint, status);
                        if status / 100 == 5 {
                            cluster.upstream_rq_5xx().inc();
                        }
                        if upstream_close {
                            drop(up); // single-use upstream connection
                        } else {
                            pool.release(endpoint, up);
                        }
                        if include_attempt_count_in_response {
                            // Parity fallback (unreached under the gate):
                            // rebuild via the owned representation is not
                            // available here; append before the final CRLF.
                            let insert_at = head_buf.len() - 2;
                            head_buf.splice(
                                insert_at..insert_at,
                                b"x-envoy-attempt-count: 1\r\n".iter().copied(),
                            );
                        }
                        write_head_body(&mut down, &mut head_buf, &body).await?;
                        tick_class(&config, status);
                    }
                    Err(source) => {
                        drop(up); // invalidate — never return a broken stream
                        tracing::warn!(
                            cluster = %cluster.name(),
                            addr = %endpoint,
                            error = ?source,
                            "uring upstream request failed — returning 503",
                        );
                        let mut resp = synth_status(503, close);
                        cluster.record_response(endpoint, resp.status);
                        write_owned(&mut down, &mut resp, &req.headers, &mut write_buf).await?;
                        tick_class(&config, resp.status);
                    }
                }
            }
        }

        if close {
            return Ok(());
        }
    }
}

/// One request/response exchange on a kept-alive upstream connection, using
/// the shared direct-path serializers. Chunked upstream framing is
/// unsupported in the prototype (returns an error → 503 downstream).
async fn direct_exchange(
    up: &mut UpConn,
    req: &Request,
    host: &str,
    start_ms: u128,
    close: bool,
    out: &mut Vec<u8>,
) -> Result<ExchangeOk, Http1Error> {
    up.buf.clear();

    let mut wire = std::mem::take(&mut up.wire);
    build_request_wire(&mut wire, host, req, true);
    let (res, wire_back) = up.stream.write_all(wire).await;
    up.wire = wire_back;
    res.map_err(|source| Http1Error::Io { source })?;

    let head: DirectHead = loop {
        // Accumulate via the rented scratch buffer, then extend the parse
        // buffer (one small memcpy; keeps rent-ownership semantics simple).
        up.scratch.clear();
        let scratch = std::mem::take(&mut up.scratch);
        match monoio::time::timeout(crate::client::READ_TIMEOUT, up.stream.read(scratch)).await {
            Ok((res, b)) => {
                up.scratch = b;
                let n = res.map_err(|source| Http1Error::Io { source })?;
                if n == 0 {
                    return Err(Http1Error::UnexpectedEof);
                }
                up.buf.extend_from_slice(&up.scratch[..n]);
            }
            Err(_elapsed) => return Err(Http1Error::UnexpectedEof),
        }
        if let Some(head) = parse_response_head(&up.buf)? {
            break head;
        }
    };

    if head.chunked {
        // Prototype limitation: no chunked fallback on the uring path.
        return Err(Http1Error::MalformedChunkedFraming);
    }

    // Read the CL-framed body to completion.
    while up.buf.len() - head.headers_end < head.cl {
        up.scratch.clear();
        let scratch = std::mem::take(&mut up.scratch);
        match monoio::time::timeout(crate::client::READ_TIMEOUT, up.stream.read(scratch)).await {
            Ok((res, b)) => {
                up.scratch = b;
                let n = res.map_err(|source| Http1Error::Io { source })?;
                if n == 0 {
                    return Err(Http1Error::UnexpectedEof);
                }
                up.buf.extend_from_slice(&up.scratch[..n]);
            }
            Err(_elapsed) => return Err(Http1Error::UnexpectedEof),
        }
    }
    let body = up.buf[head.headers_end..head.headers_end + head.cl].to_vec();

    let elapsed_ms = crate::date::coarse_monotonic_ms().saturating_sub(start_ms);
    serialize_direct_head(out, &up.buf, &head, body.len(), elapsed_ms, close);

    Ok(ExchangeOk::Direct {
        status: head.status,
        upstream_close: head.upstream_close,
        body,
    })
}

/// Emit a pre-serialized head + body with the same coalescing threshold as
/// the tokio writer (`write_head_and_body`): sub-threshold bodies are
/// appended to the head and written once; larger bodies are written as two
/// sequential writes (wire-equivalent to the tokio path's writev).
async fn write_head_body(
    down: &mut TcpStream,
    head: &mut Vec<u8>,
    body: &[u8],
) -> Result<(), Http1Error> {
    if body.len() >= VECTORED_BODY_THRESHOLD {
        let h = std::mem::take(head);
        let (res, h) = down.write_all(h).await;
        *head = h;
        res.map_err(|source| Http1Error::Io { source })?;
        let (res, _b) = down.write_all(body.to_vec()).await;
        res.map_err(|source| Http1Error::Io { source })?;
    } else {
        head.extend_from_slice(body);
        let h = std::mem::take(head);
        let (res, h) = down.write_all(h).await;
        *head = h;
        res.map_err(|source| Http1Error::Io { source })?;
    }
    Ok(())
}

/// Serialize + write an owned synth `Response` (same head serializer and
/// coalescing rule as `Http1Response::write_to_buf`).
///
/// 110.1: this is the io_uring worker's LOCAL-REPLY WIRE FUNNEL, and the
/// gRPC transform lives INSIDE it deliberately. All four call sites write a
/// synthetic local reply; the proxied path uses `write_head_body` instead.
/// Installing the transform here rather than at the call sites makes the
/// coverage structural — a fifth local-reply site added later cannot forget it.
///
/// The tokio path has its OWN funnel in `hcm.rs`'s `serve_connection`, which
/// bypasses this function entirely; a transform installed at only one of the
/// two silently misses the other.
///
/// `resp` is `&mut` because the transform rewrites it in place. Every caller
/// therefore ticks `tick_class` AFTER this returns, so the per-class counter
/// sees the TRANSFORMED status (measurement N-2), while
/// `cluster.record_response` stays BEFORE it so outlier detection still
/// records the ORIGINAL upstream-health status.
async fn write_owned(
    down: &mut TcpStream,
    resp: &mut Response,
    req_headers: &[(String, String)],
    buf: &mut Vec<u8>,
) -> Result<(), Http1Error> {
    crate::grpc::apply_grpc_local_reply(resp, req_headers);
    serialize_response_head(resp, buf);
    write_head_body(down, buf, &resp.body).await
}

fn tick_class(config: &HCMConfig, status: u16) {
    match status / 100 {
        2 => config.stats.downstream_rq_2xx.inc(),
        3 => config.stats.downstream_rq_3xx.inc(),
        4 => config.stats.downstream_rq_4xx.inc(),
        5 => config.stats.downstream_rq_5xx.inc(),
        _ => {}
    }
}
