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
}
