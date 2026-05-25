//! Per-connection plaintext HTTP/2 cleartext (H2C) client. No pooling.
//! Sibling of `envoy_http1::Client` from 04.3; sole user of `h2::client::*`
//! per parent-05 SPEC §3 cross-sub-phase architectural rule 1.

use crate::error::Http2Error;
use bytes::Bytes;
use envoy_http1::codec::Request;
use envoy_http1::response::Response;
use std::net::SocketAddr;

/// Per-connection H2C client. Stateless; the per-stream state lives on
/// `ClientStream`. Mirrors `envoy_http1::Client`'s shape verbatim.
pub struct Client;

impl Client {
    /// TCP-connect to `addr`, run `h2::client::handshake`, drive the resulting
    /// `Connection` on a fire-and-forget `tokio::spawn`, and return a
    /// `ClientStream` wrapping the captured `SendRequest` handle + `host`.
    pub async fn connect(addr: SocketAddr, host: &str) -> Result<ClientStream, Http2Error> {
        let tcp = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|source| Http2Error::UpstreamConnect { addr, source })?;
        let (send_request, connection) = h2::client::handshake(tcp)
            .await
            .map_err(|source| Http2Error::H2ClientHandshake { source })?;
        // Per parent §6 signpost 6 / SPEC §6 local signpost 19: drive the
        // h2::client::Connection on a fire-and-forget tokio::spawn for the
        // lifetime of the SendRequest handle. The task terminates when
        // SendRequest drops + the connection gracefully closes; post-response
        // errors are logged but do NOT propagate (the send_request call has
        // already returned by the time the connection task encounters a
        // post-response error per signpost 19).
        //
        // Note: h2::client::handshake sends the client preface + SETTINGS but
        // does NOT wait for the server's SETTINGS frame. We use a brief
        // select! window (10 ms) to detect immediate handshake failures (e.g.,
        // the server responds with HTTP/1.1 instead of H2 SETTINGS). On a valid
        // H2 server the connection future is Poll::Pending within 10 ms and the
        // timeout branch wins, at which point we spawn the connection task.
        // Box::pin keeps the future heap-allocated so ownership can be moved
        // into the tokio::spawn after the select (Pin<Box<T>>: Unpin).
        let mut connection = Box::pin(connection);
        tokio::select! {
            biased;
            conn_result = connection.as_mut() => {
                // Connection completed (failed) before the detection window.
                return Err(Http2Error::H2ClientHandshake {
                    source: conn_result
                        .err()
                        .unwrap_or_else(|| h2::Reason::CONNECT_ERROR.into()),
                });
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {
                // Connection still alive after 10 ms — normal case.
            }
        }
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::warn!(?error, "h2 client connection task ended with error");
            }
        });
        Ok(ClientStream {
            send_request,
            host: host.to_string(),
        })
    }
}

/// Active per-connection H2 client state: the `h2::client::SendRequest` handle
/// (the channel to the spawned connection task) and the host string captured
/// at connect time (used as the synthesized `:authority` pseudo-header default
/// if the outgoing request doesn't carry an explicit `Host:` header). Mirrors
/// `envoy_http1::ClientStream`'s shape; one underlying TCP connection per
/// `Client::connect` invocation, but the `SendRequest<Bytes>` handle is `Clone`
/// (per h2 v0.4 — that's the multiplexing-enabling property) so cloning a
/// `ClientStream` shares the same connection across many concurrent streams.
/// 13.2 D5 widened the field visibility to `pub(crate)` + added `Clone` so the
/// per-stream `H2PoolGuard` (`envoy_http2::pool::H2PoolGuard`) can hold a fresh
/// `SendRequest` clone — see `crates/envoy-http2/src/pool.rs`.
#[derive(Clone)]
pub struct ClientStream {
    pub(crate) send_request: h2::client::SendRequest<Bytes>,
    pub(crate) host: String,
}

impl std::fmt::Debug for ClientStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientStream")
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

// H2-forbidden hop-by-hop headers: crate::H2_FORBIDDEN_HOP_BY_HOP (lib.rs).
// Per Task 2 review I2: consolidated from per-module duplicates into a single
// crate-level constant. See lib.rs for the canonical definition + rationale.

impl ClientStream {
    /// Translate `request` to an H2 frame stream and read the response back.
    /// Seven-step pipeline: (a) resolve `:authority` — explicit `Host:`
    /// (case-insensitive) wins over the captured host per parent §6 signpost
    /// 12 + SPEC §3 cross-sub-phase architectural rule 3, mirroring
    /// `envoy_http1::Client::send_request`; (b) build the `http::Request<()>`
    /// head with absolute-form URI so the h2 codec populates `:method` /
    /// `:authority` / `:path` / `:scheme: http` correctly; (c) apply request
    /// headers, lowercasing names per parent §6 signpost 11 and stripping
    /// H2-forbidden hop-by-hop names per SPEC §3 architectural rule 4
    /// (defense-in-depth — the h2 codec also rejects them); skip `Host:`
    /// (became `:authority`); (d) send via `h2::client::SendRequest::send_request`
    /// with `end_of_stream=true` for empty bodies, otherwise HEADERS +
    /// `send_data(body, end_of_stream=true)`; (e) read the response head via
    /// the returned `ResponseFuture`; (f) drain the response body from
    /// `h2::RecvStream` into `Bytes` via the same pattern 05.2 D3 uses on the
    /// listener-side body intake (per parent §6 signpost 9 the drain budget
    /// is unbounded in 05.3); (g) translate `http::Response<()>` + body bytes
    /// into the protocol-agnostic `envoy_http1::response::Response` value type
    /// per cross-sub-phase architectural rule 2.
    ///
    /// Errors map per failure site: TCP/`SocketAddr` failures handled at
    /// `Client::connect`; here `MalformedH2HeaderBlock` covers builder
    /// failures (URI/header name/value parse), `H2SendRequest` covers
    /// send-stream / response-future failures, `H2RecvBody` covers body-read
    /// / flow-control failures, `BadStatusCode` is defense-in-depth on the
    /// 100..=599 range invariant.
    pub async fn send_request(&mut self, request: Request) -> Result<Response, Http2Error> {
        // (a) :authority resolution. Explicit Host: wins over the captured
        // host. Mirrors envoy_http1::Client::send_request's host-resolution
        // posture per crates/envoy-http1/src/client.rs.
        let authority: String = request
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("host"))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| self.host.clone());

        // (b) Build the http::Request<()> head. URI is absolute-form per RFC
        // 7540 §8.1.2.3 — `:scheme://:authority:path` so the h2 codec
        // populates :scheme, :authority, :path correctly.
        let uri_str = format!("http://{authority}{}", request.path);
        let http_req = http::Request::builder()
            .method(request.method.as_str())
            .uri(uri_str.as_str())
            .version(http::Version::HTTP_2)
            .body(())
            .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
        let (mut parts, ()) = http_req.into_parts();

        // (c) Apply request headers, lowercasing names + stripping H2-forbidden
        // hop-by-hop names defensively + skipping Host: (became :authority).
        for (name, value) in &request.headers {
            let lower = name.to_ascii_lowercase();
            if lower == "host" {
                continue;
            }
            if crate::H2_FORBIDDEN_HOP_BY_HOP.contains(&lower.as_str()) {
                continue;
            }
            let header_name = http::HeaderName::from_bytes(lower.as_bytes())
                .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
            let header_value = http::HeaderValue::from_str(value)
                .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
            parts.headers.append(header_name, header_value);
        }
        let http_req_with_headers = http::Request::from_parts(parts, ());

        // (d) Decide end_of_stream from the body. If empty, end_of_stream=true
        // on the HEADERS frame; no DATA frame emitted. If non-empty, send
        // HEADERS with end_of_stream=false then DATA with end_of_stream=true.
        let body = request.body.unwrap_or_default();
        let body_is_empty = body.is_empty();

        let (response_future, mut send_stream) = self
            .send_request
            .send_request(http_req_with_headers, body_is_empty)
            .map_err(|source| Http2Error::H2SendRequest { source })?;

        if !body_is_empty {
            send_stream
                .send_data(body, true)
                .map_err(|source| Http2Error::H2SendRequest { source })?;
        }

        // (e) Read the response head.
        let http_resp = response_future
            .await
            .map_err(|source| Http2Error::H2SendRequest { source })?;
        let (resp_parts, mut recv_stream) = http_resp.into_parts();

        // (f) Drain the response body. Mirrors 05.2 D3's listener-side body
        // intake pattern (concat into a single Bytes via BytesMut); per parent
        // §6 signpost 9 the body-bytes drain budget is unbounded in 05.3.
        let mut body_bytes = bytes::BytesMut::new();
        while let Some(chunk_result) = recv_stream.data().await {
            let chunk = chunk_result.map_err(|source| Http2Error::H2RecvBody { source })?;
            body_bytes.extend_from_slice(&chunk);
            recv_stream
                .flow_control()
                .release_capacity(chunk.len())
                .map_err(|source| Http2Error::H2RecvBody { source })?;
        }

        // (g) Translate http::Response<()> + body bytes → envoy Response. The
        // status range is 100..=599 per route-walk + h2 codec validation; the
        // BadStatusCode variant is defense-in-depth (mirrors response.rs).
        let status = resp_parts.status.as_u16();
        if !(100..=599).contains(&status) {
            return Err(Http2Error::BadStatusCode { status });
        }
        let mut headers: Vec<(String, String)> = Vec::with_capacity(resp_parts.headers.len());
        for (name, value) in resp_parts.headers.iter() {
            // h2 lowercases all header names per RFC 7540; preserve as-is.
            let value_str = match value.to_str() {
                Ok(s) => s.to_string(),
                Err(_) => continue, // skip malformed (non-ASCII) values defensively
            };
            headers.push((name.as_str().to_string(), value_str));
        }
        Ok(Response {
            status,
            reason: None,
            headers,
            body: body_bytes.freeze(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use envoy_http1::codec::HttpVersion;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Spawn an in-process h2 server on a 127.0.0.1 ephemeral port. The
    /// `responder` closure builds the response (status + headers + body) given
    /// the captured request shape (method/path/authority + headers). Returns
    /// the bound addr + a `JoinHandle` whose abort is the server's lifecycle.
    async fn spawn_h2_server<F>(
        responder: F,
    ) -> (
        std::net::SocketAddr,
        Arc<Mutex<Option<http::Request<Bytes>>>>,
        tokio::task::JoinHandle<()>,
    )
    where
        F: Fn(&http::Request<Bytes>) -> http::Response<Bytes> + Send + Sync + 'static,
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let captured: Arc<Mutex<Option<http::Request<Bytes>>>> = Arc::new(Mutex::new(None));
        let captured_clone = Arc::clone(&captured);
        let responder = Arc::new(responder);
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
                let (req, mut send_response) = match result {
                    Ok(p) => p,
                    Err(_) => return,
                };
                let (parts, mut body) = req.into_parts();
                // Drain request body bytes (small body assumption — the tests
                // don't exercise multi-frame request bodies).
                let mut body_bytes = bytes::BytesMut::new();
                while let Some(chunk_result) = body.data().await {
                    let chunk = match chunk_result {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    body_bytes.extend_from_slice(&chunk);
                    let _ = body.flow_control().release_capacity(chunk.len());
                }
                let captured_req = http::Request::from_parts(parts, body_bytes.freeze());
                let resp = responder(&captured_req);
                {
                    let mut slot = captured_clone.lock().await;
                    *slot = Some(captured_req);
                }
                let (parts, body) = resp.into_parts();
                let response_head = http::Response::from_parts(parts, ());
                let mut send_stream = match send_response.send_response(response_head, false) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let _ = send_stream.send_data(body, true);
            }
        });
        (addr, captured, handle)
    }

    /// Spawn an in-process h2 server that emits the given response chunks
    /// across multiple DATA frames. Used by `send_request_drains_multi_frame_response_body`.
    async fn spawn_h2_server_chunks(
        chunks: Vec<Bytes>,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
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
            // Wrap chunks in Option so ownership can be taken exactly once;
            // subsequent loop iterations (which drive the connection to flush
            // the queued DATA frames) receive None and skip the send path.
            let mut chunks_opt = Some(chunks);
            while let Some(result) = conn.accept().await {
                let Some(chunks) = chunks_opt.take() else {
                    // Connection is being drained; no more chunks to send.
                    return;
                };
                let (_req, mut send_response) = match result {
                    Ok(p) => p,
                    Err(_) => return,
                };
                let resp = http::Response::builder().status(200).body(()).unwrap();
                let mut send_stream = match send_response.send_response(resp, false) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let n = chunks.len();
                for (i, chunk) in chunks.into_iter().enumerate() {
                    let end = i == n - 1;
                    let _ = send_stream.send_data(chunk, end);
                }
                // Do NOT early-return here. The while-loop's next iteration
                // calls conn.accept().await, which drives the h2 connection and
                // flushes the queued DATA frames to the wire. The loop exits
                // naturally when the client closes the connection.
            }
        });
        (addr, handle)
    }

    fn mk_request(method: &str, path: &str, headers: Vec<(&str, &str)>, body: Bytes) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            version: HttpVersion::Http11,
            headers: headers
                .into_iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            bytes_consumed: 0,
            body: Some(body),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_succeeds_against_in_process_h2_listener() {
        let (addr, _captured, _handle) = spawn_h2_server(|_req| {
            http::Response::builder()
                .status(200)
                .body(Bytes::from_static(b"ok"))
                .unwrap()
        })
        .await;
        let client = Client::connect(addr, "test.example").await;
        assert!(client.is_ok(), "expected connect Ok, got {client:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_returns_upstream_connect_on_refused() {
        // Bind ephemeral, then drop the listener — the addr is unbound for the
        // duration of the test (deterministic ConnectionRefused on Linux/macOS).
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let result = Client::connect(addr, "test.example").await;
        match result {
            Err(Http2Error::UpstreamConnect { addr: a, source }) => {
                assert_eq!(a, addr);
                // ConnectionRefused on Linux/macOS; some platforms may surface
                // ConnectionReset or other ErrorKinds — accept any io::Error
                // and assert the variant alone.
                let _ = source;
            }
            other => panic!("expected UpstreamConnect, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_writes_get_with_synthesized_pseudoheaders() {
        let (addr, captured, _handle) = spawn_h2_server(|_req| {
            http::Response::builder()
                .status(200)
                .body(Bytes::from_static(b""))
                .unwrap()
        })
        .await;
        let mut client = Client::connect(addr, "test.example").await.unwrap();
        let req = mk_request("GET", "/", vec![], Bytes::new());
        let _resp = client.send_request(req).await.expect("send_request");
        let captured = captured.lock().await;
        let captured = captured.as_ref().expect("h2 server captured request");
        assert_eq!(captured.method().as_str(), "GET");
        assert_eq!(captured.uri().path(), "/");
        assert_eq!(
            captured.uri().authority().map(|a| a.as_str()),
            Some("test.example")
        );
        assert_eq!(captured.uri().scheme_str(), Some("http"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_explicit_host_header_wins_over_captured_host() {
        let (addr, captured, _handle) = spawn_h2_server(|_req| {
            http::Response::builder()
                .status(200)
                .body(Bytes::from_static(b""))
                .unwrap()
        })
        .await;
        let mut client = Client::connect(addr, "test.example").await.unwrap();
        let req = mk_request("GET", "/", vec![("Host", "real.example")], Bytes::new());
        let _resp = client.send_request(req).await.expect("send_request");
        let captured = captured.lock().await;
        let captured = captured.as_ref().expect("h2 server captured request");
        // Per SPEC §3 D1: explicit Host: wins over captured host.
        assert_eq!(
            captured.uri().authority().map(|a| a.as_str()),
            Some("real.example")
        );
        // Host: row should NOT be present in the captured headers (it became
        // :authority and was stripped from the headers vec).
        assert!(
            captured.headers().iter().all(|(n, _)| n.as_str() != "host"),
            "host: row should not appear alongside :authority"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_reads_response_status_headers_body() {
        let (addr, _captured, _handle) = spawn_h2_server(|_req| {
            http::Response::builder()
                .status(200)
                .header("content-type", "text/plain")
                .body(Bytes::from_static(b"hello\n"))
                .unwrap()
        })
        .await;
        let mut client = Client::connect(addr, "test.example").await.unwrap();
        let req = mk_request("GET", "/", vec![], Bytes::new());
        let resp = client.send_request(req).await.expect("send_request");
        assert_eq!(resp.status, 200);
        assert!(
            resp.headers
                .iter()
                .any(|(n, v)| n == "content-type" && v == "text/plain"),
            "expected content-type: text/plain in headers, got {:?}",
            resp.headers
        );
        assert_eq!(resp.body.as_ref(), &b"hello\n"[..]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_drains_multi_frame_response_body() {
        let chunks = vec![
            Bytes::from_static(b"abcd"),
            Bytes::from_static(b"efgh"),
            Bytes::from_static(b"ijkl"),
        ];
        let (addr, _handle) = spawn_h2_server_chunks(chunks).await;
        let mut client = Client::connect(addr, "test.example").await.unwrap();
        let req = mk_request("GET", "/", vec![], Bytes::new());
        let resp = client.send_request(req).await.expect("send_request");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body.as_ref(), &b"abcdefghijkl"[..]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_strips_h2_forbidden_hop_by_hop_headers() {
        let (addr, captured, _handle) = spawn_h2_server(|_req| {
            http::Response::builder()
                .status(200)
                .body(Bytes::from_static(b""))
                .unwrap()
        })
        .await;
        let mut client = Client::connect(addr, "test.example").await.unwrap();
        let req = mk_request(
            "GET",
            "/",
            vec![
                ("connection", "close"),
                ("transfer-encoding", "chunked"),
                ("keep-alive", "timeout=5"),
                ("upgrade", "h2c"),
                ("proxy-connection", "close"),
                ("x-keep", "preserved"),
            ],
            Bytes::new(),
        );
        let _resp = client.send_request(req).await.expect("send_request");
        let captured = captured.lock().await;
        let captured = captured.as_ref().expect("h2 server captured request");
        for forbidden in &[
            "connection",
            "transfer-encoding",
            "keep-alive",
            "upgrade",
            "proxy-connection",
        ] {
            assert!(
                captured
                    .headers()
                    .iter()
                    .all(|(n, _)| n.as_str() != *forbidden),
                "forbidden header {forbidden} appeared in upstream request"
            );
        }
        assert!(
            captured
                .headers()
                .iter()
                .any(|(n, v)| n.as_str() == "x-keep" && v.as_bytes() == b"preserved"),
            "non-forbidden header x-keep was unexpectedly stripped"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_maps_h2_handshake_failure_to_typed_error() {
        // Spawn a TCP listener that responds to the H2C handshake with HTTP/1.1
        // bytes (not a SETTINGS frame). h2::client::handshake should reject
        // with an h2::Error mapped to Http2Error::H2ClientHandshake.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _handle = tokio::spawn(async move {
            if let Ok((mut tcp, _)) = listener.accept().await {
                use tokio::io::AsyncWriteExt;
                let _ = tcp
                    .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
                    .await;
                let _ = tcp.shutdown().await;
            }
        });
        let result = Client::connect(addr, "test.example").await;
        match result {
            Err(Http2Error::H2ClientHandshake { source: _ }) => {}
            other => panic!("expected H2ClientHandshake, got {other:?}"),
        }
    }
}
