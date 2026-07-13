//! Per-(cluster, endpoint) probe loop and per-probe `probe_once` helper.
//!
//! `probe_loop` is the body of every spawned task: a `tokio::time::interval`
//! ticker + `tokio::select!` on a `CancellationToken` (graceful shutdown).
//! `probe_once` performs ONE HTTP probe — `Client::connect` + `send_request`
//! wrapped in `tokio::time::timeout(timeout, ...)`. Connection failures,
//! timeouts, and out-of-range statuses all count as failure.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use envoy_cluster::EndpointHealth;
use envoy_config::Int64Range;
use envoy_http1::client::{Client, ClientStream};
use envoy_http1::codec::{HttpVersion, Request};
use envoy_stats::Counter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{MissedTickBehavior, interval, timeout};
use tokio_util::sync::CancellationToken;

/// 12.2: the periodic probe loop, one tokio task per (cluster, endpoint).
/// Single-writer to `endpoint_health` per the M2 contract (PLAN lock-in #6).
/// Graceful cancellation via the `tokio::select!` cancel branch.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn probe_loop(
    addr: SocketAddr,
    host: String,
    path: String,
    probe_timeout: Duration,
    interval_dur: Duration,
    expected_statuses: Vec<Int64Range>,
    endpoint_health: Arc<EndpointHealth>,
    attempt: Arc<Counter>,
    success: Arc<Counter>,
    failure: Arc<Counter>,
    cancel: CancellationToken,
) {
    let mut ticker = interval(interval_dur);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::debug!(addr=%addr, "active-HC probe task shutting down");
                return;
            }
            _ = ticker.tick() => {
                attempt.inc();
                match probe_once(addr, &host, &path, probe_timeout, &expected_statuses).await {
                    Ok(()) => {
                        success.inc();
                        endpoint_health.record_success();
                    }
                    Err(e) => {
                        tracing::debug!(addr=%addr, error=?e, "active-HC probe failed");
                        failure.inc();
                        endpoint_health.record_failure();
                    }
                }
            }
        }
    }
}

/// Outcome of a single probe — Ok = healthy contribution; Err = failure
/// contribution.
#[derive(Debug)]
#[allow(dead_code)] // diagnostic-only; counters + EndpointHealth carry the live signal
pub(crate) enum ProbeError {
    /// `tokio::time::timeout(probe_timeout, ...)` elapsed.
    Timeout,
    /// `Client::connect` returned an error (typically `UpstreamConnect`).
    Connect(String),
    /// `send_request` returned an error.
    Send(String),
    /// Response status not in `expected_statuses`.
    UnexpectedStatus(u16),
}

/// 12.2: one probe — connect + send_request + status check, all under one
/// per-probe `tokio::time::timeout`. Fresh connection (no `reuse_connection`
/// at phase-12 scope per parent §4).
///
/// The `Host:` header on the wire is sourced from the `host` argument: it
/// is captured by `Client::connect(addr, host)` and injected by
/// `ClientStream::send_request` when the outgoing `Request.headers` does
/// not already carry one (the existing client `host` de-dup contract). The
/// probe omits any explicit `Host:` header, so the connect-time value wins
/// — `<hc.host or cluster_name>` per PLAN lock-in #9.
pub(crate) async fn probe_once(
    addr: SocketAddr,
    host: &str,
    path: &str,
    probe_timeout: Duration,
    expected_statuses: &[Int64Range],
) -> Result<(), ProbeError> {
    let probe = async move {
        let mut stream: ClientStream = Client::connect(addr, host)
            .await
            .map_err(|e| ProbeError::Connect(e.to_string()))?;
        let req = Request {
            method: "GET".to_string(),
            path: path.to_string(),
            version: HttpVersion::Http11,
            headers: Vec::new(),
            bytes_consumed: 0,
            body: None,
        };
        let resp = stream
            .send_request(req)
            .await
            .map_err(|e| ProbeError::Send(e.to_string()))?;
        if status_acceptable(resp.status, expected_statuses) {
            Ok(())
        } else {
            Err(ProbeError::UnexpectedStatus(resp.status))
        }
    };
    match timeout(probe_timeout, probe).await {
        Ok(r) => r,
        Err(_) => Err(ProbeError::Timeout),
    }
}

/// 12.2: success criterion per §6.2 item-5 + PLAN lock-in #10.
/// Empty `expected_statuses` = the upstream default (exactly 200).
fn status_acceptable(status: u16, expected: &[Int64Range]) -> bool {
    if expected.is_empty() {
        return status == 200;
    }
    let s = status as i64;
    expected.iter().any(|r| s >= r.start && s < r.end)
}

/// 68 (ADR-0137 PV-3): scan `buf` for the `receive` payloads in order — each
/// found as a contiguous substring at/after the previous match's end. Empty
/// `receive` ⇒ connection-only (always true once connected). Single-block
/// reduces to "substring anywhere" (the reliably-measured Envoy behavior);
/// multi-block is envoy-rust's own sequential contract, NOT an Envoy-parity claim.
fn receive_matches(receive: &[Vec<u8>], buf: &[u8]) -> bool {
    let mut offset = 0usize;
    for payload in receive {
        if payload.is_empty() {
            continue;
        }
        match find_subslice(&buf[offset..], payload) {
            Some(pos) => offset += pos + payload.len(),
            None => return false,
        }
    }
    true
}

/// First index of `needle` in `haystack`, or `None`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 68: TCP-probe failure surface (diagnostic; the counters + EndpointHealth carry
/// the live signal, mirroring the HTTP `ProbeError`).
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum TcpProbeError {
    /// `tokio::time::timeout(probe_timeout, ...)` elapsed (connect hang, or
    /// `receive` never matched — the MEASURED `active_hc_timeout` path).
    Timeout,
    /// `TcpStream::connect` failed (the MEASURED connect-refuse path).
    Connect(String),
    /// Write of the `send` payload failed.
    Send(String),
    /// The connection reached EOF before `receive` matched.
    Eof,
}

/// 68 (ADR-0137 PV-6): one TCP probe — connect → optional `send` → scan for
/// `receive`, the WHOLE thing under one `timeout(probe_timeout, ...)` (the HC
/// timeout, not the cluster connect_timeout, bounds connect). Empty `receive`
/// ⇒ a successful connect is healthy. Mirrors the HTTP `probe_once` shape.
pub(crate) async fn tcp_probe_once(
    addr: SocketAddr,
    send: &Option<Vec<u8>>,
    receive: &[Vec<u8>],
    probe_timeout: Duration,
) -> Result<(), TcpProbeError> {
    let probe = async move {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|e| TcpProbeError::Connect(e.to_string()))?;
        if let Some(bytes) = send {
            stream
                .write_all(bytes)
                .await
                .map_err(|e| TcpProbeError::Send(e.to_string()))?;
        }
        if receive.is_empty() {
            // Connection-only: connect success ⇒ healthy.
            return Ok(());
        }
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream
                .read(&mut chunk)
                .await
                .map_err(|e| TcpProbeError::Send(e.to_string()))?;
            if n == 0 {
                return Err(TcpProbeError::Eof);
            }
            buf.extend_from_slice(&chunk[..n]);
            if receive_matches(receive, &buf) {
                return Ok(());
            }
        }
    };
    match timeout(probe_timeout, probe).await {
        Ok(r) => r,
        Err(_) => Err(TcpProbeError::Timeout),
    }
}

/// 68: the periodic TCP-probe loop — the L4 sibling of `probe_loop`. Same
/// `interval` ticker + `tokio::select!` cancel branch + counter/EndpointHealth
/// wiring; only `probe_once` → `tcp_probe_once` differs.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn tcp_probe_loop(
    addr: SocketAddr,
    send: Option<Vec<u8>>,
    receive: Vec<Vec<u8>>,
    probe_timeout: Duration,
    interval_dur: Duration,
    endpoint_health: Arc<EndpointHealth>,
    attempt: Arc<Counter>,
    success: Arc<Counter>,
    failure: Arc<Counter>,
    cancel: CancellationToken,
) {
    let mut ticker = interval(interval_dur);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::debug!(addr=%addr, "active-HC TCP probe task shutting down");
                return;
            }
            _ = ticker.tick() => {
                attempt.inc();
                match tcp_probe_once(addr, &send, &receive, probe_timeout).await {
                    Ok(()) => {
                        success.inc();
                        endpoint_health.record_success();
                    }
                    Err(e) => {
                        tracing::debug!(addr=%addr, error=?e, "active-HC TCP probe failed");
                        failure.inc();
                        endpoint_health.record_failure();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_expected_statuses_accepts_only_200() {
        assert!(status_acceptable(200, &[]));
        assert!(!status_acceptable(201, &[]));
        assert!(!status_acceptable(503, &[]));
    }

    #[test]
    fn half_open_range_excludes_end() {
        let r = vec![Int64Range {
            start: 200,
            end: 201,
        }];
        assert!(status_acceptable(200, &r));
        assert!(!status_acceptable(201, &r));
    }

    #[test]
    fn multi_range_union() {
        let r = vec![
            Int64Range {
                start: 200,
                end: 300,
            },
            Int64Range {
                start: 418,
                end: 419,
            },
        ];
        assert!(status_acceptable(204, &r));
        assert!(status_acceptable(418, &r));
        assert!(!status_acceptable(419, &r));
        assert!(!status_acceptable(503, &r));
    }

    // -----------------------------------------------------------------------
    // 68: TCP probe (pure matcher + integration)
    // -----------------------------------------------------------------------

    #[test]
    fn receive_matches_single_block_substring_anywhere() {
        // MEASURED: banner "ABPINGCD", receive [PING] → healthy (substring in the middle).
        assert!(receive_matches(&[b"PING".to_vec()], b"ABPINGCD"));
        assert!(receive_matches(&[b"PING".to_vec()], b"PING"));
        assert!(!receive_matches(&[b"PONG".to_vec()], b"ABPINGCD"));
    }

    #[test]
    fn receive_matches_empty_receive_is_true() {
        // Connection-only: no receive payloads ⇒ connect success alone is healthy.
        assert!(receive_matches(&[], b""));
        assert!(receive_matches(&[], b"anything"));
    }

    #[test]
    fn receive_matches_sequential_in_order() {
        // envoy-rust's OWN documented multi-block contract (NOT an Envoy-parity claim,
        // ADR-0137 PV-3): each block found at/after the previous match end.
        assert!(receive_matches(
            &[b"AB".to_vec(), b"CD".to_vec()],
            b"AB__CD"
        ));
        assert!(!receive_matches(
            &[b"CD".to_vec(), b"AB".to_vec()],
            b"AB__CD"
        ));
    }

    #[tokio::test]
    async fn tcp_probe_connection_only_healthy() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });
        assert!(
            tcp_probe_once(addr, &None, &[], Duration::from_secs(2))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn tcp_probe_connect_refused_is_err() {
        // Reserve then drop a listener → the port refuses.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let r = tcp_probe_once(addr, &None, &[], Duration::from_secs(1)).await;
        assert!(matches!(
            r,
            Err(TcpProbeError::Connect(_)) | Err(TcpProbeError::Timeout)
        ));
    }

    #[tokio::test]
    async fn tcp_probe_receive_match_healthy() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            use tokio::io::AsyncWriteExt;
            let _ = s.write_all(b"AB").await;
            let _ = s.write_all(b"PING").await;
            let _ = s.write_all(b"CD").await;
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        let r = tcp_probe_once(addr, &None, &[b"PING".to_vec()], Duration::from_secs(2)).await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn tcp_probe_receive_mismatch_times_out() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            use tokio::io::AsyncWriteExt;
            let _ = s.write_all(b"NOPE").await;
            tokio::time::sleep(Duration::from_secs(2)).await;
        });
        let r = tcp_probe_once(addr, &None, &[b"PING".to_vec()], Duration::from_millis(400)).await;
        assert!(matches!(r, Err(TcpProbeError::Timeout)));
    }

    #[tokio::test]
    async fn tcp_probe_send_then_receive_healthy() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut b = [0u8; 16];
            let n = s.read(&mut b).await.unwrap();
            assert_eq!(&b[..n], b"hi");
            let _ = s.write_all(b"resp-OKOK-end").await;
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        let r = tcp_probe_once(
            addr,
            &Some(b"hi".to_vec()),
            &[b"OKOK".to_vec()],
            Duration::from_secs(2),
        )
        .await;
        assert!(r.is_ok());
    }
}
