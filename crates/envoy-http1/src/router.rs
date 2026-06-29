//! Router-proxy helper module: RouterError enum + write_proxied_response shape
//! policy. Per SPEC §6 signpost 7 + parent-04 SPEC §3 cross-sub-phase rule about
//! placing HCM-internal logic in envoy-http1.

use crate::error::Http1Error;
use crate::headers as hdr;
use crate::response::{Http1Response, Response};

/// 04.3 NEW: typed errors surfaced by the router-proxy arm in `hcm.rs`.
/// Each variant carries `cluster: String` for per-cluster log attribution
/// (per SPEC §3 D5: this is what makes the `Cluster::name()` close-out
/// load-bearing — the router's `tracing::warn!(cluster = ..., ...)` log
/// lines on per-cluster proxy errors are the natural use site).
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    /// Cluster has no live endpoints (the static-cluster case is `0` endpoints
    /// at config-load — validator rejects in Task 2 — but defense-in-depth
    /// covers the case where `pick_endpoint()` returns `None` for any reason).
    #[error("no healthy endpoint available for cluster '{cluster}'")]
    NoHealthyEndpoint { cluster: String },

    /// Wraps a `Http1Error::UpstreamConnect`. Surfaces the cluster name
    /// alongside the underlying `io::Error`; the cluster name is what
    /// distinguishes per-cluster connection failures in operational logs.
    #[error("upstream connect failed for cluster '{cluster}': {source}")]
    UpstreamConnect {
        cluster: String,
        #[source]
        source: Http1Error,
    },

    /// Wraps any post-connect Http1Error (`MalformedResponseLine`,
    /// `MalformedChunkedFraming`, `UnexpectedEof`, `Io`, `HeadersTooLarge`,
    /// `BodyTooLarge`).
    #[error("upstream request failed for cluster '{cluster}': {source}")]
    UpstreamRequestFailed {
        cluster: String,
        #[source]
        source: Http1Error,
    },
}

/// 04.3 NEW: response headers envoy-rust's HCM emits on every direct_response
/// path. When a proxied response from upstream carries any of these names,
/// `write_proxied_response` REPLACES the upstream's value with envoy-rust's
/// own (matches Envoy's posture: upstream's `server: nginx/1.x` is overwritten
/// with `server: envoy`).
pub const HCM_EMITTED_HEADERS: &[&str] = &["server", "date"];

/// 04.3 NEW: the `x-envoy-upstream-service-time` header name (allow-listed
/// per BEHAVIOR_CONTRACT.md row added in Task 10). Both Envoy and envoy-rust
/// emit on every router-proxy response with their own measurement of upstream
/// latency in milliseconds.
pub const X_ENVOY_UPSTREAM_SERVICE_TIME: &str = "x-envoy-upstream-service-time";

/// 16 Task 4 (L6): the `x-envoy-attempt-count` response-header name. Emitted on
/// the downstream response by the HCM retry loop (`hcm.rs`) ONLY when the
/// matched virtual-host's `include_attempt_count_in_response` flag is true; the
/// value is the total number of upstream attempts (2 after one retry). Lives
/// here next to `X_ENVOY_UPSTREAM_SERVICE_TIME` for header-name co-location;
/// the injection itself happens in the retry loop (not in
/// `construct_proxied_response`) because it must also decorate the
/// limit-exceeded last-503 and the connect-fail synth-503.
pub const X_ENVOY_ATTEMPT_COUNT: &str = "x-envoy-attempt-count";

/// Construct the synthesized downstream Response value WITHOUT writing it to
/// the wire. Mirrors the pre-07.1 body of `write_proxied_response` minus the
/// wire-write call.
///
/// Used by the 07.1 Task 5 unified factored wire-write site at
/// `crates/envoy-http1/src/hcm.rs::serve_connection`: each writer-arm
/// populates `outgoing: Response` (the proxy-success arm calls this helper);
/// below the arm match, a single `Http1Response::write_to` fires once. Task 6
/// will insert `pipeline.encode_headers(&mut outgoing)` between the arm
/// match's close and the wire write.
///
/// 16 Task 4: this helper NO LONGER increments any cluster counters. The
/// `upstream_rq_total` / `upstream_rq_5xx` increments (06.3 D15.3.c) moved out
/// to the HCM retry loop in `hcm.rs` so each counter has a single source of
/// truth under retries: `upstream_rq_total` fires per ATTEMPT (lock-in L5) and
/// `upstream_rq_5xx` fires once on the COMPLETING response only (a retried-away
/// 5xx must not tick it). This helper is response-construction only; it is
/// called once per upstream-response attempt and stays side-effect-free.
///
/// Per SPEC §6 signpost 7:
/// 1. Status line forwards verbatim from upstream.
/// 2. For each upstream header: if the name is in HCM_EMITTED_HEADERS,
///    replace with envoy-rust's value (`server: envoy-rust`, `date: <fresh IMF-fixdate>`);
///    drop `connection` and `transfer-encoding` (envoy-rust authoritatively sets
///    `connection` per posture below, and the body has been decoded into a
///    known-length `Bytes` so chunked framing is not re-emitted); otherwise
///    pass through with the name lowercased (case-insensitive per RFC 7230 §3.2;
///    envoy-rust normalises egress to lowercase).
/// 3. Append `x-envoy-upstream-service-time: <elapsed_ms>`.
/// 4. Set `Connection:` per `close` flag (true → `close`, false → `keep-alive`).
/// 5. Forward the body bytes preserving the upstream's framing (CL or chunked
///    — the body bytes are already decoded into a single Bytes by client.rs's
///    chunked reader, so the downstream side always emits CL-framed in 04.3).
pub fn construct_proxied_response(
    upstream_response: Response,
    elapsed_ms: u128,
    close: bool,
) -> Response {
    // 16 Task 4: counter increments removed (moved to the HCM retry loop, see
    // the function doc above) — this helper is response-construction only and
    // no longer needs the cluster handle.
    let now_date = crate::date::now_imf_fixdate();
    let mut headers: Vec<(String, String)> =
        Vec::with_capacity(upstream_response.headers.len() + 2);

    let mut saw_server = false;
    let mut saw_date = false;
    let mut saw_cl = false;

    for (name, value) in upstream_response.headers.into_iter() {
        let lc = name.to_ascii_lowercase();
        if lc == hdr::SERVER {
            saw_server = true;
            headers.push((hdr::SERVER.to_string(), "envoy-rust".to_string()));
        } else if lc == hdr::DATE {
            saw_date = true;
            headers.push((hdr::DATE.to_string(), now_date.clone()));
        } else if lc == hdr::CONNECTION {
            // Drop any upstream Connection: header — we authoritatively set it
            // below per the downstream posture.
            continue;
        } else if lc == hdr::TRANSFER_ENCODING {
            // Drop upstream Transfer-Encoding: the body has been fully decoded
            // into a known-length Bytes by client.rs's chunked reader. Keeping
            // this header while also emitting Content-Length violates RFC 7230
            // §3.3.3 rule 3 and causes real clients to reject the response.
            continue;
        } else if lc == hdr::CONTENT_LENGTH {
            saw_cl = true;
            headers.push((hdr::CONTENT_LENGTH.to_string(), value));
        } else {
            // Pass through with the name lowercased (RFC 7230 §3.2 — header names are
            // case-insensitive; envoy-rust normalises egress to lowercase to match the
            // rest of the response.write_to wire format). Includes content-type and
            // any allow-listed headers that envoy-rust does not authoritatively set.
            headers.push((lc, value));
        }
    }
    // Inject defaults for HCM-emitted headers the upstream didn't carry.
    if !saw_server {
        headers.push((hdr::SERVER.to_string(), "envoy-rust".to_string()));
    }
    if !saw_date {
        headers.push((hdr::DATE.to_string(), now_date));
    }
    // Inject Content-Length if upstream didn't carry one (post-chunked-decode
    // body has known length).
    if !saw_cl {
        headers.push((
            hdr::CONTENT_LENGTH.to_string(),
            upstream_response.body.len().to_string(),
        ));
    }
    // Inject x-envoy-upstream-service-time per SPEC §2 + BEHAVIOR_CONTRACT.md row.
    headers.push((
        X_ENVOY_UPSTREAM_SERVICE_TIME.to_string(),
        elapsed_ms.to_string(),
    ));
    // Authoritative Connection per posture.
    headers.push((
        hdr::CONNECTION.to_string(),
        if close { "close" } else { "keep-alive" }.to_string(),
    ));

    Response {
        status: upstream_response.status,
        reason: upstream_response.reason,
        headers,
        body: upstream_response.body,
    }
}

/// Pre-07.1 helper: construct + write the proxied response in one call.
///
/// At Task 5 this becomes a thin wrapper around `construct_proxied_response`
/// + `Http1Response::write_to`. Retained because pre-existing tests
///   (`write_proxied_response_increments_upstream_rq_total_on_200` /
///   `_5xx_on_503` and the wire-output tests above) call it directly.
pub async fn write_proxied_response<W>(
    downstream: &mut W,
    cluster: &envoy_cluster::ClusterHandle,
    upstream_response: Response,
    elapsed_ms: u128,
    close: bool,
) -> Result<(), Http1Error>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let _ = cluster;
    let resp = construct_proxied_response(upstream_response, elapsed_ms, close);
    Http1Response::write_to(&resp, downstream).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::sync::Arc;

    fn upstream(status: u16, headers: Vec<(&str, &str)>, body: &[u8]) -> Response {
        Response {
            status,
            reason: None,
            headers: headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body: Bytes::copy_from_slice(body),
        }
    }

    /// Build a test ClusterHandle via from_bootstrap with a fresh registry.
    /// Returns `(handle, registry)` so tests can re-register counters to get
    /// the same Arc the cluster holds (idempotent same-kind contract).
    async fn mk_test_cluster() -> (
        envoy_cluster::ClusterHandle,
        Arc<envoy_stats::StatsRegistry>,
    ) {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners: []
  clusters:
    - name: test
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: test
        endpoints:
          - lb_endpoints:
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 10000 } } }
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("parse");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("cluster mgr");
        let handle = mgr.get("test").expect("cluster present");
        (handle, registry)
    }

    /// Run write_proxied_response into an in-memory Vec and return the bytes.
    async fn drive_proxy(upstream_resp: Response, elapsed_ms: u128, close: bool) -> Vec<u8> {
        let (cluster, _registry) = mk_test_cluster().await;
        let mut buf: Vec<u8> = Vec::new();
        write_proxied_response(&mut buf, &cluster, upstream_resp, elapsed_ms, close)
            .await
            .expect("write_proxied_response");
        buf
    }

    #[tokio::test]
    async fn proxied_response_appends_x_envoy_upstream_service_time() {
        // Upstream returns 200 with simple headers; assert downstream wire
        // carries x-envoy-upstream-service-time with the integer ms value.
        let up = upstream(
            200,
            vec![("Content-Type", "text/plain"), ("Content-Length", "5")],
            b"hello",
        );
        let buf = drive_proxy(up, 42, false).await;
        let s = String::from_utf8_lossy(&buf);
        assert!(
            s.contains("x-envoy-upstream-service-time: 42\r\n"),
            "got: {s}"
        );
    }

    #[tokio::test]
    async fn proxied_response_overwrites_server_and_date_headers() {
        // Upstream emits non-envoy server + a fixed-date stamp. envoy-rust
        // overwrites both with its own values per HCM_EMITTED_HEADERS policy.
        let up = upstream(
            200,
            vec![
                ("Server", "upstream-software/1.0"),
                ("Date", "Thu, 01 Jan 1970 00:00:00 GMT"),
                ("Content-Length", "5"),
                ("Content-Type", "text/plain"),
            ],
            b"hello",
        );
        let buf = drive_proxy(up, 1, false).await;
        let s = String::from_utf8_lossy(&buf);
        assert!(
            s.contains("server: envoy-rust\r\n"),
            "server overwrite: {s}"
        );
        assert!(
            !s.contains("upstream-software"),
            "must not pass upstream Server: {s}"
        );
        assert!(s.contains("date: "), "fresh date: {s}");
        assert!(
            !s.contains("Thu, 01 Jan 1970"),
            "must not pass upstream Date: {s}"
        );
        // The body + content-length + content-type pass through verbatim.
        assert!(s.contains("content-type: text/plain\r\n"), "ct: {s}");
        assert!(s.contains("content-length: 5\r\n"), "cl: {s}");
        assert!(s.ends_with("\r\nhello"), "body: {s}");
    }

    #[tokio::test]
    async fn proxied_response_sets_connection_per_posture() {
        let up = upstream(
            200,
            vec![("Content-Length", "0"), ("Connection", "keep-alive")],
            b"",
        );
        let buf_close = drive_proxy(up.clone(), 1, true).await;
        let s_close = String::from_utf8_lossy(&buf_close);
        assert!(
            s_close.contains("connection: close\r\n"),
            "close: {s_close}"
        );
        assert!(
            !s_close.contains("connection: keep-alive\r\n"),
            "must not pass upstream Connection: {s_close}"
        );

        let buf_keep = drive_proxy(up, 1, false).await;
        let s_keep = String::from_utf8_lossy(&buf_keep);
        assert!(
            s_keep.contains("connection: keep-alive\r\n"),
            "keep-alive: {s_keep}"
        );
    }

    #[tokio::test]
    async fn proxied_response_strips_upstream_transfer_encoding() {
        // Upstream emitted Transfer-Encoding: chunked; client.rs's chunked reader
        // decoded the body but left the header in upstream_response.headers.
        // write_proxied_response MUST strip transfer-encoding (RFC 7230 §3.3.3
        // forbids T-E + Content-Length combo). The synthesized response carries
        // Content-Length: <body.len()> only.
        let up = upstream(
            200,
            vec![
                ("Transfer-Encoding", "chunked"),
                ("Content-Type", "text/plain"),
            ],
            b"hello",
        );
        let buf = drive_proxy(up, 1, false).await;
        let s = String::from_utf8_lossy(&buf);
        assert!(
            !s.to_ascii_lowercase().contains("transfer-encoding"),
            "transfer-encoding must be stripped: {s}"
        );
        assert!(s.contains("content-length: 5\r\n"), "synthesized CL: {s}");
        assert!(s.ends_with("\r\nhello"), "body: {s}");
    }

    // ── 16 Task 4: single-source-of-truth — counters NOT incremented here ──
    //
    // 06.3 D15.3.c originally incremented `upstream_rq_total` / `upstream_rq_5xx`
    // inside `construct_proxied_response`. Phase 16 Task 4 moved those increments
    // to the HCM retry loop (per-attempt total; completing-response-only 5xx) so
    // each counter has a single increment site under retries. These tests now
    // assert the helper is side-effect-FREE — the increments are exercised
    // end-to-end by the `hcm.rs` retry tests instead.

    /// 16 Task 4: `write_proxied_response` (200) no longer ticks
    /// `upstream_rq_total` / `upstream_rq_5xx` — the counters moved to the HCM.
    #[tokio::test]
    async fn write_proxied_response_does_not_increment_counters_on_200() {
        let (cluster, registry) = mk_test_cluster().await;
        let rq_total = registry
            .register_counter("cluster.test.upstream_rq_total")
            .unwrap();
        let rq_5xx = registry
            .register_counter("cluster.test.upstream_rq_5xx")
            .unwrap();
        let up = upstream(200, vec![("Content-Length", "2")], b"ok");
        let mut buf: Vec<u8> = Vec::new();
        write_proxied_response(&mut buf, &cluster, up, 1, false)
            .await
            .expect("write");
        assert_eq!(rq_total.value(), 0, "upstream_rq_total moved to HCM loop");
        assert_eq!(rq_5xx.value(), 0, "upstream_rq_5xx moved to HCM loop");
    }

    /// 16 Task 4: `write_proxied_response` (503) no longer ticks either counter.
    #[tokio::test]
    async fn write_proxied_response_does_not_increment_counters_on_503() {
        let (cluster, registry) = mk_test_cluster().await;
        let rq_total = registry
            .register_counter("cluster.test.upstream_rq_total")
            .unwrap();
        let rq_5xx = registry
            .register_counter("cluster.test.upstream_rq_5xx")
            .unwrap();
        let up = upstream(503, vec![("Content-Length", "0")], b"");
        let mut buf: Vec<u8> = Vec::new();
        write_proxied_response(&mut buf, &cluster, up, 1, false)
            .await
            .expect("write");
        assert_eq!(rq_total.value(), 0, "upstream_rq_total moved to HCM loop");
        assert_eq!(rq_5xx.value(), 0, "upstream_rq_5xx moved to HCM loop");
    }

    // ── 07.1 Task 5: construct_proxied_response factored helper tests ──

    /// 07.1 Task 5: `construct_proxied_response` returns a Response with the
    /// upstream status, the elapsed-ms x-envoy-upstream-service-time header,
    /// the synthesized content-length, and Connection: keep-alive when
    /// `close = false`.
    #[tokio::test]
    async fn construct_proxied_response_returns_response_with_status_200() {
        let (_cluster, _registry) = mk_test_cluster().await;
        let up = upstream(200, vec![("content-type", "text/plain")], b"hello");
        let resp = construct_proxied_response(up, 7, false);
        assert_eq!(resp.status, 200);
        // x-envoy-upstream-service-time injected with the elapsed_ms value.
        assert!(
            resp.headers
                .iter()
                .any(|(n, v)| n.eq_ignore_ascii_case(X_ENVOY_UPSTREAM_SERVICE_TIME) && v == "7"),
            "x-envoy-upstream-service-time: 7 must be present"
        );
        // content-length injected from body.len() (5 bytes for "hello").
        assert!(
            resp.headers
                .iter()
                .any(|(n, v)| n.eq_ignore_ascii_case("content-length") && v == "5"),
            "content-length: 5 must be present"
        );
        // Connection: keep-alive when close = false.
        assert!(
            resp.headers
                .iter()
                .any(|(n, v)| n.eq_ignore_ascii_case("connection") && v == "keep-alive"),
            "connection: keep-alive must be present"
        );
    }

    /// 16 Task 4: `construct_proxied_response` is now side-effect-free — it
    /// does NOT tick `upstream_rq_total` / `upstream_rq_5xx` (moved to HCM).
    #[tokio::test]
    async fn construct_proxied_response_does_not_increment_counters() {
        let (cluster, _registry) = mk_test_cluster().await;
        let up = upstream(200, vec![], b"");
        let _resp = construct_proxied_response(up, 1, false);
        assert_eq!(cluster.upstream_rq_total().value(), 0);
        assert_eq!(cluster.upstream_rq_5xx().value(), 0);
    }

    /// 16 Task 4: `construct_proxied_response` on 503 still does NOT tick the
    /// counters, and sets `Connection: close` when `close = true`.
    #[tokio::test]
    async fn construct_proxied_response_no_counters_on_503() {
        let (cluster, _registry) = mk_test_cluster().await;
        let up = upstream(503, vec![], b"");
        let resp = construct_proxied_response(up, 5, true);
        assert_eq!(resp.status, 503);
        assert_eq!(cluster.upstream_rq_total().value(), 0);
        assert_eq!(cluster.upstream_rq_5xx().value(), 0);
        // Connection: close when close = true.
        assert!(
            resp.headers
                .iter()
                .any(|(n, v)| n.eq_ignore_ascii_case("connection") && v == "close"),
            "connection: close must be present"
        );
    }
}
