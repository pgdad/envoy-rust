//! Per-connection plaintext HTTP/1.1 client. No pooling; one TCP connection
//! per upstream request (pooling is upstream-robustness-family territory,
//! out of phase 04 per parent SPEC §4 + 04.3 SPEC §4 non-goals).
//!
//! This module is the workspace's SOLE user of `httparse::Response::parse`
//! per parent-04 SPEC §3 cross-sub-phase architectural rule 1. The 04.1
//! codec module uses `httparse::Request::parse`; this module is the only
//! consumer of the response parser.

use crate::codec::Request;
use crate::error::Http1Error;
use crate::headers as hdr;
use crate::response::Response;

use bytes::Bytes;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const RESPONSE_HEADERS_CAP: usize = 8192;
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-connection plaintext HTTP/1.1 client. Stateless; the per-stream
/// state lives on `ClientStream`.
pub struct Client;

impl Client {
    /// TCP-connect to `addr`. The `host` value is captured for the eventual
    /// `Host:` header on `send_request`. No bytes are sent during connect;
    /// the caller's first `send_request` is the first wire write.
    ///
    /// Errors: `Http1Error::UpstreamConnect { addr, source }` on any
    /// `tokio::net::TcpStream::connect` failure.
    pub async fn connect(
        addr: std::net::SocketAddr,
        host: &str,
    ) -> Result<ClientStream, Http1Error> {
        let stream = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|source| Http1Error::UpstreamConnect { addr, source })?;
        Ok(ClientStream {
            stream,
            host: host.to_string(),
            buf: bytes::BytesMut::with_capacity(8192),
        })
    }
}

/// Active per-connection state: the underlying TCP stream, the host string
/// captured at connect time (used as the `Host:` header default if the
/// outgoing request doesn't carry one), and a read buffer for the response.
#[derive(Debug)]
pub struct ClientStream {
    pub(crate) stream: tokio::net::TcpStream,
    pub(crate) host: String,
    pub(crate) buf: bytes::BytesMut,
}

impl ClientStream {
    /// Serialize and write `request` (request-line + headers + optional
    /// CL-framed body), then read the response (status line + headers +
    /// CL-framed OR chunked body). The `Host:` header is sourced from the
    /// `host` captured at connect time UNLESS `request` already carries a
    /// `Host:` header (case-insensitive match), in which case the request's
    /// value wins and the connect-time host is dropped.
    ///
    /// Per SPEC §3 D1: chunked READER is implemented in Task 7; this Task-6
    /// implementation handles only the Content-Length response path. Chunked
    /// responses surface as `Http1Error::MalformedChunkedFraming` until Task 7.
    pub async fn send_request(&mut self, request: Request) -> Result<Response, Http1Error> {
        // (a) Serialize the request.
        let mut wire: Vec<u8> = Vec::with_capacity(256 + request.body_len_estimate());
        wire.extend_from_slice(request.method.as_bytes());
        wire.push(b' ');
        wire.extend_from_slice(request.path.as_bytes());
        wire.extend_from_slice(b" HTTP/1.1\r\n");

        // Host de-dup: if request.headers already carries a Host (any case),
        // emit that one. Otherwise inject the connect-time host.
        let request_has_host = request
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(hdr::HOST));
        if !request_has_host {
            wire.extend_from_slice(b"host: ");
            wire.extend_from_slice(self.host.as_bytes());
            wire.extend_from_slice(b"\r\n");
        }
        for (name, value) in &request.headers {
            wire.extend_from_slice(name.to_ascii_lowercase().as_bytes());
            wire.extend_from_slice(b": ");
            wire.extend_from_slice(value.as_bytes());
            wire.extend_from_slice(b"\r\n");
        }
        // CL header — only emit if the request doesn't already carry one.
        let request_has_cl = request
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(hdr::CONTENT_LENGTH));
        if !request_has_cl {
            wire.extend_from_slice(b"content-length: ");
            wire.extend_from_slice(request.body_len_string().as_bytes());
            wire.extend_from_slice(b"\r\n");
        }
        wire.extend_from_slice(b"\r\n");
        // Body bytes (CL-framed; Task 6 supports CL only — chunked-request
        // forwarding from downstream is deferred per SPEC §4 non-goals).
        if let Some(body) = request.body_bytes() {
            wire.extend_from_slice(body);
        }

        self.stream.write_all(&wire).await?;
        self.stream.flush().await?;

        // (b) Read the response headers.
        loop {
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(READ_TIMEOUT, self.stream.read(&mut chunk))
                .await
                .map_err(|_| Http1Error::UnexpectedEof)??;
            if n == 0 {
                return Err(Http1Error::UnexpectedEof);
            }
            self.buf.extend_from_slice(&chunk[..n]);

            let mut hp_storage = [httparse::EMPTY_HEADER; 64];
            let mut parsed = httparse::Response::new(&mut hp_storage);
            match parsed.parse(&self.buf) {
                Ok(httparse::Status::Complete(headers_end)) => {
                    let status = parsed.code.ok_or(Http1Error::MalformedResponseLine)?;
                    let mut headers: Vec<(String, String)> =
                        Vec::with_capacity(parsed.headers.len());
                    for h in parsed.headers.iter().filter(|h| !h.name.is_empty()) {
                        let name = h.name.to_string();
                        let value = std::str::from_utf8(h.value)
                            .map(str::to_string)
                            .map_err(|_| Http1Error::MalformedResponseLine)?;
                        headers.push((name, value));
                    }

                    // Detect chunked vs CL framing.
                    let chunked = headers.iter().any(|(n, v)| {
                        n.eq_ignore_ascii_case("transfer-encoding")
                            && v.eq_ignore_ascii_case("chunked")
                    });
                    if chunked {
                        // 04.3 Task 7: real chunked reader.
                        let already = self.buf.len() - headers_end;
                        let body = read_chunked_body(
                            &mut self.stream,
                            &mut self.buf,
                            headers_end,
                            already,
                        )
                        .await?;
                        return Ok(Response {
                            status,
                            reason: None,
                            headers,
                            body: Bytes::from(body),
                        });
                    }

                    let cl: usize = headers
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case(hdr::CONTENT_LENGTH))
                        .and_then(|(_, v)| v.parse().ok())
                        .unwrap_or(0);

                    // Drain remaining body bytes from the stream + buf.
                    let already = self.buf.len() - headers_end;
                    let mut body: Vec<u8> = Vec::with_capacity(cl);
                    if already > 0 {
                        let take = already.min(cl);
                        body.extend_from_slice(&self.buf[headers_end..headers_end + take]);
                    }
                    while body.len() < cl {
                        let mut chunk = [0u8; 4096];
                        let n = tokio::time::timeout(READ_TIMEOUT, self.stream.read(&mut chunk))
                            .await
                            .map_err(|_| Http1Error::UnexpectedEof)??;
                        if n == 0 {
                            return Err(Http1Error::UnexpectedEof);
                        }
                        let need = cl - body.len();
                        body.extend_from_slice(&chunk[..n.min(need)]);
                    }

                    return Ok(Response {
                        status,
                        reason: None,
                        headers,
                        body: Bytes::from(body),
                    });
                }
                Ok(httparse::Status::Partial) => {
                    if self.buf.len() > RESPONSE_HEADERS_CAP {
                        return Err(Http1Error::HeadersTooLarge {
                            cap: RESPONSE_HEADERS_CAP,
                        });
                    }
                    continue;
                }
                Err(httparse::Error::Token)
                | Err(httparse::Error::Version)
                | Err(httparse::Error::Status) => {
                    return Err(Http1Error::MalformedResponseLine);
                }
                Err(httparse::Error::HeaderName)
                | Err(httparse::Error::HeaderValue)
                | Err(httparse::Error::NewLine) => {
                    return Err(Http1Error::MalformedHeader);
                }
                Err(httparse::Error::TooManyHeaders) => {
                    return Err(Http1Error::HeadersTooLarge {
                        cap: RESPONSE_HEADERS_CAP,
                    });
                }
            }
        }
    }
}

/// Read a chunked-encoding response body from `stream`, having already read
/// `already` bytes past the headers into `buf` starting at offset `headers_end`.
/// Returns the decoded body bytes (chunks concatenated; trailers discarded).
///
/// Wire format per RFC 7230 §4.1:
///   chunk        = chunk-size CRLF chunk-data CRLF
///   last-chunk   = "0" CRLF [trailer-part] CRLF
///   chunk-size   = 1*HEXDIG
///
/// 04.3 ignores trailers (per SPEC §4 non-goals — trailer forwarding deferred).
/// On any framing violation returns `Http1Error::MalformedChunkedFraming`.
async fn read_chunked_body(
    stream: &mut tokio::net::TcpStream,
    buf: &mut bytes::BytesMut,
    headers_end: usize,
    _already: usize,
) -> Result<Vec<u8>, Http1Error> {
    use tokio::io::AsyncReadExt;

    let mut out: Vec<u8> = Vec::new();
    let mut pos = headers_end;

    loop {
        // Ensure at least one CRLF visible after `pos`.
        let crlf_offset = loop {
            if let Some(off) = find_crlf(&buf[pos..]) {
                break off;
            }
            // Need more bytes.
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
                .await
                .map_err(|_| Http1Error::MalformedChunkedFraming)??;
            if n == 0 {
                return Err(Http1Error::MalformedChunkedFraming);
            }
            buf.extend_from_slice(&chunk[..n]);
        };

        // Parse chunk-size as hex (with optional ;ext extensions per RFC).
        let size_line = std::str::from_utf8(&buf[pos..pos + crlf_offset])
            .map_err(|_| Http1Error::MalformedChunkedFraming)?
            .trim();
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let chunk_size =
            usize::from_str_radix(size_hex, 16).map_err(|_| Http1Error::MalformedChunkedFraming)?;

        pos += crlf_offset + 2; // skip size line + CRLF

        if chunk_size == 0 {
            // Last chunk. RFC 7230 allows trailer-part before the final CRLF;
            // 04.3 reads (and discards) until the next CRLF (the empty-line
            // sentinel). For simplicity, assume zero trailers — read one CRLF
            // and we're done. If the response has trailers, the framing is
            // technically valid but body content is intact (we've already
            // read all chunk bytes); the `0\r\n\r\n` shape covers the no-trailer
            // case which fixture 0008 + the test response use.
            //
            // Defensive read: if the next 2 bytes are CRLF, accept; else read
            // until CRLF then require another CRLF (single-pass trailer skip).
            while buf.len() < pos + 2 {
                let mut chunk = [0u8; 64];
                let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
                    .await
                    .map_err(|_| Http1Error::MalformedChunkedFraming)??;
                if n == 0 {
                    return Err(Http1Error::MalformedChunkedFraming);
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            if &buf[pos..pos + 2] != b"\r\n" {
                // Trailers present — skip until empty CRLF line.
                loop {
                    let crlf = match find_crlf(&buf[pos..]) {
                        Some(off) => off,
                        None => {
                            let mut chunk = [0u8; 256];
                            let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
                                .await
                                .map_err(|_| Http1Error::MalformedChunkedFraming)??;
                            if n == 0 {
                                return Err(Http1Error::MalformedChunkedFraming);
                            }
                            buf.extend_from_slice(&chunk[..n]);
                            continue;
                        }
                    };
                    pos += crlf + 2;
                    if crlf == 0 {
                        break; // empty line — end of trailers
                    }
                }
            }
            // `pos` is not used after this point; return immediately.
            return Ok(out);
        }

        // Read exactly `chunk_size` body bytes + 2 trailing CRLF.
        while buf.len() < pos + chunk_size + 2 {
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
                .await
                .map_err(|_| Http1Error::MalformedChunkedFraming)??;
            if n == 0 {
                return Err(Http1Error::MalformedChunkedFraming);
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        out.extend_from_slice(&buf[pos..pos + chunk_size]);
        if &buf[pos + chunk_size..pos + chunk_size + 2] != b"\r\n" {
            return Err(Http1Error::MalformedChunkedFraming);
        }
        pos += chunk_size + 2;
    }
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_succeeds_against_in_process_acceptor() {
        // Bind an in-process acceptor on an ephemeral port. Client::connect
        // should TCP-connect cleanly and return a ClientStream.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // Spawn an acceptor that just drops every connection.
        tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
            }
        });
        let stream = Client::connect(addr, "envoy-rust.test")
            .await
            .expect("connect");
        assert_eq!(stream.host, "envoy-rust.test");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_returns_upstream_connect_on_refused_port() {
        // 127.0.0.1:1 is kernel-refused on every Linux box. macOS may differ
        // but the failure mode is still a connect-time io::Error which the
        // map_err arm wraps in UpstreamConnect.
        let addr: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
        let err = Client::connect(addr, "envoy-rust.test")
            .await
            .expect_err("connect must fail");
        match err {
            Http1Error::UpstreamConnect {
                addr: got_addr,
                source,
            } => {
                assert_eq!(got_addr, addr);
                // The exact io::ErrorKind varies by OS (ConnectionRefused on
                // Linux, ConnectionRefused or AddrNotAvailable on macOS); just
                // assert there's some Display output.
                assert!(!source.to_string().is_empty());
            }
            other => panic!("expected UpstreamConnect, got: {other:?}"),
        }
    }

    // ── 04.3 Task 6 send_request Content-Length tests ─────────────────────────

    use crate::codec::{HttpVersion, Request};

    /// Build a minimal Request with method, path, and headers.
    /// Body defaults to None.
    fn req(method: &str, path: &str, headers: &[(&str, &str)]) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            version: HttpVersion::Http11,
            headers: headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            bytes_consumed: 0, // not used by send_request
            body: None,        // 04.3 NEW
        }
    }

    /// Spawn an in-process acceptor that reads bytes into a Vec, sends a
    /// fixed response, and closes. Returns the listener address + a
    /// JoinHandle producing the captured request bytes.
    async fn capturing_acceptor(
        response: &'static [u8],
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let h = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut sock, _) = listener.accept().await.unwrap();
            // Read request bytes for ~500ms or until the read returns,
            // whichever comes first. Using a fixed-size read loop is good
            // enough for tests; real production servers would parse Content-Length.
            let mut buf = vec![0u8; 8192];
            let n =
                tokio::time::timeout(std::time::Duration::from_millis(500), sock.read(&mut buf))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or(0);
            buf.truncate(n);
            // Write response.
            let _ = sock.write_all(response).await;
            let _ = sock.shutdown().await;
            buf
        });
        (addr, h)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_writes_serialized_request_bytes() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let (addr, capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        let request = req("GET", "/", &[("user-agent", "test")]);
        let _resp = client.send_request(request).await.expect("send_request");
        let captured = capture.await.unwrap();
        let s = String::from_utf8_lossy(&captured);
        assert!(s.starts_with("GET / HTTP/1.1\r\n"), "request line: {s:?}");
        // Host header injected from connect's `host` (case-preserved as
        // emitted; lower-case here per send_request's emission convention).
        assert!(
            s.contains("host: envoy-rust.test\r\n"),
            "missing injected host: {s:?}"
        );
        assert!(
            s.contains("user-agent: test\r\n"),
            "missing user-agent: {s:?}"
        );
        assert!(
            s.contains("content-length: 0\r\n"),
            "missing content-length: {s:?}"
        );
        assert!(s.ends_with("\r\n\r\n"), "wire end: {s:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_uses_request_host_when_provided() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let (addr, capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        // Outgoing request explicitly carries Host: explicit.example. The
        // connect-time `host` ("envoy-rust.test") should be IGNORED — the
        // explicit value wins per SPEC §6 signpost 5.
        let request = req("GET", "/", &[("Host", "explicit.example")]);
        let _resp = client.send_request(request).await.expect("send_request");
        let captured = capture.await.unwrap();
        let s = String::from_utf8_lossy(&captured);
        assert!(
            s.contains("host: explicit.example\r\n"),
            "request must use explicit Host: {s:?}"
        );
        assert!(
            !s.contains("host: envoy-rust.test\r\n"),
            "request must NOT inject the connect-time host when an explicit one is present: {s:?}"
        );
        // case-insensitive de-dup: only one host header.
        let host_count = s.matches("host:").count() + s.matches("Host:").count();
        assert_eq!(host_count, 1, "exactly one Host header: {s:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_reads_cl_response_body() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        let (addr, _capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        let request = req("GET", "/", &[]);
        let resp = client.send_request(request).await.expect("send_request");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body.as_ref(), b"hello");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_returns_malformed_response_line_on_garbage() {
        let response: &[u8] = b"NOT AN HTTP RESPONSE";
        let (addr, _capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        let request = req("GET", "/", &[]);
        let err = client
            .send_request(request)
            .await
            .expect_err("garbage upstream must fail");
        assert!(
            matches!(err, Http1Error::MalformedResponseLine),
            "got: {err:?}"
        );
    }

    // ── 04.3 Task 7 chunked-encoding reader tests ─────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_reads_chunked_response_body() {
        // Two chunks ("hello" 5 bytes + " world" 6 bytes) terminated by 0-size.
        let response: &[u8] =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        let (addr, _capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        let request = req("GET", "/", &[]);
        let resp = client.send_request(request).await.expect("send_request");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body.as_ref(), b"hello world");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_request_returns_malformed_chunked_on_bad_size_line() {
        // "XYZ" is not a valid hex chunk size.
        let response: &[u8] =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nXYZ\r\nhello\r\n";
        let (addr, _capture) = capturing_acceptor(response).await;
        let mut client = Client::connect(addr, "envoy-rust.test").await.unwrap();
        let request = req("GET", "/", &[]);
        let err = client
            .send_request(request)
            .await
            .expect_err("malformed chunk size must fail");
        assert!(
            matches!(err, Http1Error::MalformedChunkedFraming),
            "got: {err:?}"
        );
    }
}
