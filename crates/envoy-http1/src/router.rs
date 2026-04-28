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

/// Build the synthesized downstream response from the upstream response,
/// applying the header allow-list policy + injecting `x-envoy-upstream-service-time`
/// + setting `Connection:` per the captured-pre-drain posture, and write the
///   wire bytes via Http1Response::write_to.
///
/// Per SPEC §6 signpost 7:
/// 1. Status line forwards verbatim from upstream.
/// 2. For each upstream header: if the name is in HCM_EMITTED_HEADERS,
///    replace with envoy-rust's value (`server: envoy-rust`, `date: <fresh IMF-fixdate>`);
///    otherwise pass verbatim.
/// 3. Append `x-envoy-upstream-service-time: <elapsed_ms>`.
/// 4. Set `Connection:` per `close` flag (true → `close`, false → `keep-alive`).
/// 5. Forward the body bytes preserving the upstream's framing (CL or chunked
///    — the body bytes are already decoded into a single Bytes by client.rs's
///    chunked reader, so the downstream side always emits CL-framed in 04.3).
pub async fn write_proxied_response<W>(
    downstream: &mut W,
    upstream_response: Response,
    elapsed_ms: u128,
    close: bool,
) -> Result<(), Http1Error>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let now_date = crate::date::format_imf_fixdate(std::time::SystemTime::now());
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
        } else if lc == hdr::CONTENT_LENGTH {
            saw_cl = true;
            headers.push((hdr::CONTENT_LENGTH.to_string(), value));
        } else {
            // Pass through with lowercase name (HTTP header names are
            // case-insensitive; envoy-rust normalises to lowercase on egress).
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

    let resp = Response {
        status: upstream_response.status,
        reason: upstream_response.reason,
        headers,
        body: upstream_response.body,
    };
    Http1Response::write_to(&resp, downstream).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

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

    /// Run write_proxied_response into an in-memory Vec and parse out the
    /// resulting downstream wire bytes.
    async fn drive_proxy(upstream_resp: Response, elapsed_ms: u128, close: bool) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        write_proxied_response(&mut buf, upstream_resp, elapsed_ms, close)
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
        let buf_close = drive_proxy(
            Response {
                status: 200,
                reason: None,
                headers: up.headers.clone(),
                body: up.body.clone(),
            },
            1,
            true, // close = true
        )
        .await;
        let s_close = String::from_utf8_lossy(&buf_close);
        assert!(
            s_close.contains("connection: close\r\n"),
            "close: {s_close}"
        );
        assert!(
            !s_close.contains("connection: keep-alive\r\n"),
            "must not pass upstream Connection: {s_close}"
        );

        let buf_keep = drive_proxy(up, 1, false).await; // close = false
        let s_keep = String::from_utf8_lossy(&buf_keep);
        assert!(
            s_keep.contains("connection: keep-alive\r\n"),
            "keep-alive: {s_keep}"
        );
    }
}
