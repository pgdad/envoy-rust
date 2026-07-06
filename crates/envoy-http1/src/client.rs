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
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(30);

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
        // Disable Nagle on the upstream socket. Same rationale as the
        // downstream side in envoy-listener: small request/response pairs
        // would otherwise eat a delayed-ACK stall on every round trip.
        let _ = stream.set_nodelay(true);
        Ok(ClientStream {
            stream,
            host: host.to_string(),
            buf: bytes::BytesMut::with_capacity(4096),
            wire: Vec::with_capacity(256),
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
    /// Reusable request-serialization buffer. Cleared at the top of each
    /// `send_request*`; on a pooled keep-alive stream the allocation
    /// amortizes across every request the stream carries.
    pub(crate) wire: Vec<u8>,
}

/// Parsed facts about an upstream response head, expressed as byte-offset
/// spans into the read buffer it was parsed from. Produced by
/// [`parse_response_head`]; consumed by [`serialize_direct_head`] (and the
/// chunked fallback arm, which materializes owned headers from the spans).
/// Runtime-agnostic on purpose: the io_uring data-plane prototype shares
/// these helpers with the tokio path so both emit byte-identical responses.
pub(crate) struct DirectHead {
    pub(crate) status: u16,
    pub(crate) headers_end: usize,
    pub(crate) nspans: usize,
    pub(crate) spans: [(u32, u32, u32, u32); 64],
    pub(crate) chunked: bool,
    pub(crate) cl: usize,
    pub(crate) upstream_close: bool,
}

/// Parse an upstream response head out of `buf`. Returns `Ok(None)` when the
/// buffer does not yet hold a complete head (caller reads more and retries),
/// enforcing `RESPONSE_HEADERS_CAP` on the partial. Header locations are
/// returned as u32 byte-offset spans into `buf` (name_start, name_len,
/// value_start, value_len) so no per-header allocation happens; the framing
/// facts (chunked / content-length / connection: close) are derived in the
/// same pass.
pub(crate) fn parse_response_head(buf: &[u8]) -> Result<Option<DirectHead>, Http1Error> {
    let mut hp_storage = [httparse::EMPTY_HEADER; 64];
    let mut parsed = httparse::Response::new(&mut hp_storage);
    match parsed.parse(buf) {
        Ok(httparse::Status::Complete(headers_end)) => {
            let status = parsed.code.ok_or(Http1Error::MalformedResponseLine)?;
            let base = buf.as_ptr() as usize;
            let mut spans = [(0u32, 0u32, 0u32, 0u32); 64];
            let mut nspans = 0usize;
            let mut chunked = false;
            let mut cl: usize = 0;
            let mut upstream_close = false;
            for h in parsed.headers.iter().filter(|h| !h.name.is_empty()) {
                // UTF-8 validation parity with the owned path (which rejects
                // non-UTF-8 values with MalformedResponseLine).
                let value =
                    std::str::from_utf8(h.value).map_err(|_| Http1Error::MalformedResponseLine)?;
                if h.name.eq_ignore_ascii_case("transfer-encoding")
                    && value.eq_ignore_ascii_case("chunked")
                {
                    chunked = true;
                } else if h.name.eq_ignore_ascii_case(hdr::CONTENT_LENGTH) {
                    cl = value.parse().unwrap_or(0);
                } else if h.name.eq_ignore_ascii_case(hdr::CONNECTION)
                    && value.eq_ignore_ascii_case("close")
                {
                    upstream_close = true;
                }
                let ns = (h.name.as_ptr() as usize - base) as u32;
                let vs = (h.value.as_ptr() as usize - base) as u32;
                spans[nspans] = (ns, h.name.len() as u32, vs, h.value.len() as u32);
                nspans += 1;
            }
            Ok(Some(DirectHead {
                status,
                headers_end,
                nspans,
                spans,
                chunked,
                cl,
                upstream_close,
            }))
        }
        Ok(httparse::Status::Partial) => {
            if buf.len() > RESPONSE_HEADERS_CAP {
                return Err(Http1Error::HeadersTooLarge {
                    cap: RESPONSE_HEADERS_CAP,
                });
            }
            Ok(None)
        }
        Err(httparse::Error::Token)
        | Err(httparse::Error::Version)
        | Err(httparse::Error::Status) => Err(Http1Error::MalformedResponseLine),
        Err(httparse::Error::HeaderName)
        | Err(httparse::Error::HeaderValue)
        | Err(httparse::Error::NewLine) => Err(Http1Error::MalformedHeader),
        Err(httparse::Error::TooManyHeaders) => Err(Http1Error::HeadersTooLarge {
            cap: RESPONSE_HEADERS_CAP,
        }),
    }
}

/// Serialize the TRANSFORMED downstream response head from the parsed spans
/// of an upstream response — byte-for-byte identical to
/// `construct_proxied_response` + `Http1Response::write_to_buf` for the
/// Router-only / no-access-log configuration that gates the direct path:
/// status line with canonical reason; upstream headers in order with names
/// lowercased; `server` replaced with `envoy-rust`; `date` replaced with the
/// cached IMF-fixdate; `connection`/`transfer-encoding` dropped;
/// `content-length` passed through (or synthesized from `body_len`); then
/// `x-envoy-upstream-service-time: <elapsed_ms>` and the authoritative
/// `Connection:` per `close`.
pub(crate) fn serialize_direct_head(
    out: &mut Vec<u8>,
    buf: &[u8],
    head: &DirectHead,
    body_len: usize,
    elapsed_ms: u128,
    close: bool,
) {
    use std::io::Write as _;
    let now_date = crate::date::now_imf_fixdate();
    out.clear();
    out.reserve(96 + head.headers_end);
    out.extend_from_slice(b"HTTP/1.1 ");
    let _ = write!(out, "{}", head.status);
    out.push(b' ');
    out.extend_from_slice(crate::response::canonical_reason(head.status).as_bytes());
    out.extend_from_slice(b"\r\n");
    let (mut saw_server, mut saw_date, mut saw_cl) = (false, false, false);
    for &(ns, nl, vs, vl) in &head.spans[..head.nspans] {
        let name = &buf[ns as usize..(ns + nl) as usize];
        let value = &buf[vs as usize..(vs + vl) as usize];
        if name.eq_ignore_ascii_case(b"server") {
            saw_server = true;
            out.extend_from_slice(b"server: envoy-rust\r\n");
        } else if name.eq_ignore_ascii_case(b"date") {
            saw_date = true;
            out.extend_from_slice(b"date: ");
            out.extend_from_slice(now_date.as_bytes());
            out.extend_from_slice(b"\r\n");
        } else if name.eq_ignore_ascii_case(b"connection")
            || name.eq_ignore_ascii_case(b"transfer-encoding")
        {
            continue;
        } else if name.eq_ignore_ascii_case(b"content-length") {
            saw_cl = true;
            out.extend_from_slice(b"content-length: ");
            out.extend_from_slice(value);
            out.extend_from_slice(b"\r\n");
        } else {
            out.extend(name.iter().map(u8::to_ascii_lowercase));
            out.extend_from_slice(b": ");
            out.extend_from_slice(value);
            out.extend_from_slice(b"\r\n");
        }
    }
    if !saw_server {
        out.extend_from_slice(b"server: envoy-rust\r\n");
    }
    if !saw_date {
        out.extend_from_slice(b"date: ");
        out.extend_from_slice(now_date.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    if !saw_cl {
        out.extend_from_slice(b"content-length: ");
        let _ = write!(out, "{}", body_len);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"x-envoy-upstream-service-time: ");
    let _ = write!(out, "{}", elapsed_ms);
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(if close {
        b"connection: close\r\n".as_slice()
    } else {
        b"connection: keep-alive\r\n".as_slice()
    });
    out.extend_from_slice(b"\r\n");
}

/// Result of [`ClientStream::send_request_direct`].
pub enum DirectOutcome {
    /// The transformed response head was serialized into the caller's `out`
    /// buffer; `body` carries the (already fully read) response body.
    Direct {
        status: u16,
        upstream_close: bool,
        body: Bytes,
    },
    /// Chunked upstream framing — owned fallback; treat exactly like a
    /// [`ClientStream::send_request_borrowed`] result.
    Fallback(Response),
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
        self.send_request_borrowed(&request, false).await
    }

    /// Zero-copy proxied-response fast path. Sends `request` like
    /// [`send_request_borrowed`] and then, instead of materializing an owned
    /// `Response` header vec, serializes the TRANSFORMED downstream response
    /// head straight into `out` from the parsed byte ranges of the upstream
    /// response — byte-for-byte identical to
    /// `construct_proxied_response` + `Http1Response::write_to_buf` for the
    /// Router-only / no-access-log HCM configuration that gates it:
    /// status line with canonical reason; upstream headers in order with
    /// names lowercased; `server` replaced with `envoy-rust`; `date` replaced
    /// with the cached IMF-fixdate; `connection`/`transfer-encoding` dropped;
    /// `content-length` passed through (or synthesized); then
    /// `x-envoy-upstream-service-time` and the authoritative `Connection:`
    /// per `close`. Chunked upstream framing takes the owned fallback
    /// (`DirectOutcome::Fallback`) — the caller proceeds exactly as with
    /// [`send_request_borrowed`].
    ///
    /// `start_ms` is the attempt start in coarse-monotonic milliseconds
    /// (`date::coarse_monotonic_ms`); elapsed-ms is computed after the full
    /// response is read, matching the caller-side timing of the owned path.
    pub async fn send_request_direct(
        &mut self,
        request: &Request,
        strip_hop_headers: bool,
        start_ms: u128,
        close: bool,
        out: &mut Vec<u8>,
    ) -> Result<DirectOutcome, Http1Error> {
        self.buf.clear();

        let mut wire = std::mem::take(&mut self.wire);
        build_request_wire(&mut wire, &self.host, request, strip_hop_headers);
        let write_result = self.stream.write_all(&wire).await;
        self.wire = wire;
        write_result?;
        self.stream.flush().await?;

        loop {
            self.buf.reserve(4096);
            let n = tokio::time::timeout(READ_TIMEOUT, self.stream.read_buf(&mut self.buf))
                .await
                .map_err(|_| Http1Error::UnexpectedEof)??;
            if n == 0 {
                return Err(Http1Error::UnexpectedEof);
            }

            // Parse into byte-offset spans (no borrow of `self.buf` survives —
            // the body read below can extend the buffer freely).
            let Some(head) = parse_response_head(&self.buf)? else {
                continue;
            };
            let headers_end = head.headers_end;
            let cl = head.cl;

            if head.chunked {
                // Rare path: chunked upstream framing mutates `self.buf`, so
                // fall back to the owned representation (identical to
                // `send_request_borrowed`'s chunked arm).
                let mut headers: Vec<(String, String)> = Vec::with_capacity(head.nspans);
                for &(ns, nl, vs, vl) in &head.spans[..head.nspans] {
                    let name = std::str::from_utf8(&self.buf[ns as usize..(ns + nl) as usize])
                        .map_err(|_| Http1Error::MalformedResponseLine)?
                        .to_string();
                    let value = std::str::from_utf8(&self.buf[vs as usize..(vs + vl) as usize])
                        .map_err(|_| Http1Error::MalformedResponseLine)?
                        .to_string();
                    headers.push((name, value));
                }
                let already = self.buf.len() - headers_end;
                let body = read_chunked_body(&mut self.stream, &mut self.buf, headers_end, already)
                    .await?;
                return Ok(DirectOutcome::Fallback(Response {
                    status: head.status,
                    reason: None,
                    headers,
                    body: Bytes::from(body),
                }));
            }

            // CL-framed body (identical read strategy to the owned path).
            let already = self.buf.len() - headers_end;
            let mut body: Vec<u8> = Vec::with_capacity(cl);
            if already > 0 {
                let take = already.min(cl);
                body.extend_from_slice(&self.buf[headers_end..headers_end + take]);
            }
            while body.len() < cl {
                let need = cl - body.len();
                let mut limited = (&mut self.stream).take(need as u64);
                let n = tokio::time::timeout(READ_TIMEOUT, limited.read_buf(&mut body))
                    .await
                    .map_err(|_| Http1Error::UnexpectedEof)??;
                if n == 0 {
                    return Err(Http1Error::UnexpectedEof);
                }
            }

            // Serialize the transformed head. Byte-parity contract with
            // `construct_proxied_response` + `write_to_buf` (see doc above).
            let elapsed_ms = crate::date::coarse_monotonic_ms().saturating_sub(start_ms);
            serialize_direct_head(out, &self.buf, &head, body.len(), elapsed_ms, close);

            return Ok(DirectOutcome::Direct {
                status: head.status,
                upstream_close: head.upstream_close,
                body: Bytes::from(body),
            });
        }
    }

    /// Borrowed-request variant of [`send_request`]: serializes directly from
    /// `&Request` so the retry-loop caller can keep ONE owned request and
    /// replay it across attempts without cloning method/path/headers/body per
    /// attempt. When `strip_hop_headers` is true, `Connection:` and
    /// `Transfer-Encoding:` headers are skipped during serialization — the
    /// same effect as the former caller-side `retain` (SPEC §3 D1 one-shot
    /// upstream posture / RFC 7230 §3.3.3), without materializing a filtered
    /// header Vec.
    pub async fn send_request_borrowed(
        &mut self,
        request: &Request,
        strip_hop_headers: bool,
    ) -> Result<Response, Http1Error> {
        // Reset the read buffer. After a prior response, leftover bytes (the
        // already-consumed response headers + body bytes that came in the
        // same read chunk) remain in `self.buf`; without clearing, the
        // response-parse loop below would re-parse the stale response. Safe
        // for first-use too — clear() on an empty buf is a no-op.
        self.buf.clear();

        // (a) Serialize the request into the reusable per-stream buffer.
        let mut wire = std::mem::take(&mut self.wire);
        build_request_wire(&mut wire, &self.host, request, strip_hop_headers);

        let write_result = self.stream.write_all(&wire).await;
        self.wire = wire; // return the buffer for reuse before error-checking
        write_result?;
        self.stream.flush().await?;

        // (b) Read the response headers. `read_buf` appends straight into
        // `self.buf` — the former stack-chunk + extend_from_slice path paid a
        // 4 KiB zero-init plus a memcpy per read.
        loop {
            self.buf.reserve(4096);
            let n = tokio::time::timeout(READ_TIMEOUT, self.stream.read_buf(&mut self.buf))
                .await
                .map_err(|_| Http1Error::UnexpectedEof)??;
            if n == 0 {
                return Err(Http1Error::UnexpectedEof);
            }

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
                        // Read straight into the body Vec (capacity-bounded via
                        // take) — avoids the former per-read 4 KiB stack
                        // zero-init + memcpy. A keep-alive upstream never sends
                        // more than `cl` body bytes before our next request, but
                        // guard with `take` so a misbehaving peer can't overfill.
                        let need = cl - body.len();
                        let mut limited = (&mut self.stream).take(need as u64);
                        let n = tokio::time::timeout(READ_TIMEOUT, limited.read_buf(&mut body))
                            .await
                            .map_err(|_| Http1Error::UnexpectedEof)??;
                        if n == 0 {
                            return Err(Http1Error::UnexpectedEof);
                        }
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

/// Shared request-wire serializer for [`ClientStream::send_request_borrowed`]
/// and [`ClientStream::send_request_direct`]: request-line + headers (+ Host
/// injection, hop-header strip, synthetic content-length) + body, appended to
/// a cleared `wire`. Extracted so the two send paths cannot drift.
pub(crate) fn build_request_wire(
    wire: &mut Vec<u8>,
    host: &str,
    request: &Request,
    strip_hop_headers: bool,
) {
    wire.clear();
    wire.reserve(256 + request.body_len_estimate());
    wire.extend_from_slice(request.method.as_bytes());
    wire.push(b' ');
    wire.extend_from_slice(request.path.as_bytes());
    wire.extend_from_slice(b" HTTP/1.1\r\n");

    let skip = |name: &str| {
        strip_hop_headers
            && (name.eq_ignore_ascii_case(hdr::CONNECTION)
                || name.eq_ignore_ascii_case(hdr::TRANSFER_ENCODING))
    };

    // Host de-dup: if request.headers already carries a Host (any case),
    // emit that one. Otherwise inject the connect-time host.
    let request_has_host = request
        .headers
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case(hdr::HOST));
    if !request_has_host {
        wire.extend_from_slice(b"host: ");
        wire.extend_from_slice(host.as_bytes());
        wire.extend_from_slice(b"\r\n");
    }
    for (name, value) in &request.headers {
        if skip(name) {
            continue;
        }
        // Lowercase the name byte-by-byte into the wire buffer — the
        // former `to_ascii_lowercase()` allocated a String per header.
        wire.extend(name.as_bytes().iter().map(u8::to_ascii_lowercase));
        wire.extend_from_slice(b": ");
        wire.extend_from_slice(value.as_bytes());
        wire.extend_from_slice(b"\r\n");
    }
    // CL header — emit synthetic content-length only when the request
    // does not carry an explicit Content-Length AND the body is
    // non-empty. RFC 7230 §3.3.2 + Envoy v1.33 parity per ADR-0025
    // ("a user agent SHOULD NOT send a Content-Length header field
    // when the request message does not contain a payload body").
    let request_has_cl = request
        .headers
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case(hdr::CONTENT_LENGTH));
    let body_is_nonempty = request.body_bytes().is_some_and(|b| !b.is_empty());
    if !request_has_cl && body_is_nonempty {
        use std::io::Write as _;
        wire.extend_from_slice(b"content-length: ");
        let _ = write!(wire, "{}", request.body_len_estimate());
        wire.extend_from_slice(b"\r\n");
    }
    wire.extend_from_slice(b"\r\n");
    // Body bytes (CL-framed; Task 6 supports CL only — chunked-request
    // forwarding from downstream is deferred per SPEC §4 non-goals).
    if let Some(body) = request.body_bytes() {
        wire.extend_from_slice(body);
    }
}

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
        // 05.4 NEW per ADR-0025: empty-body GET requests do NOT carry
        // a synthetic content-length: 0 header (RFC 7230 §3.3.2 + Envoy
        // v1.33 parity). The previous assertion expected the spurious
        // header; the new assertion confirms it is suppressed.
        assert!(
            !s.contains("content-length: 0\r\n"),
            "spurious content-length: 0 must NOT be emitted on empty-body GET: {s:?}"
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
