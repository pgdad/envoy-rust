//! Wire-format HTTP/1.1 response writer.

use crate::error::Http1Error;
use bytes::Bytes;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;

/// A logical HTTP response. Caller fills in `headers` with all required
/// fields (the writer does NOT compute Content-Length); HCM's `synth_*`
/// helpers in 04.1 always populate `server`, `date`, `content-length`,
/// `content-type`, `connection` in this exact order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,                  // 100..=599
    pub reason: Option<&'static str>, // canonical reason per RFC 7231 §6.1;
    //   None falls back to a built-in table.
    pub headers: Vec<(String, String)>, // emission-order preserving.
    pub body: Bytes,                    // CL-framed in 04.1; chunked deferred.
}

pub struct Http1Response;

/// Bodies at least this large are emitted with a vectored write (head + body as
/// two `IoSlice`s), eliding the per-response memcpy of the body into the head
/// buffer. Below it the response is coalesced into one buffer and written once,
/// exactly as the pre-vectored writer did: for a small body the memcpy is
/// cheaper than the fixed cost of building the iovec array and dispatching a
/// vectored write, so coalescing is same-or-faster. The response micro-bench
/// (`tests/perf_bench.rs`) locates the crossover a few hundred bytes below this;
/// 1 KiB is a conservative round threshold at which the vectored win is
/// unambiguous (≈1.15× and rising) and well outside run-to-run noise, so the
/// change never regresses a small response.
pub(crate) const VECTORED_BODY_THRESHOLD: usize = 1024;

impl Http1Response {
    /// Serializes `resp` onto `w` as a wire-format HTTP/1.1 response:
    /// status line + headers (in emission order) + blank line + body.
    ///
    /// Allocates a fresh scratch buffer per call. On a keep-alive connection
    /// prefer [`Http1Response::write_to_buf`] with a per-connection buffer to
    /// avoid a per-response allocation.
    pub async fn write_to<W>(resp: &Response, w: &mut W) -> Result<(), Http1Error>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf: Vec<u8> = Vec::new();
        Self::write_to_buf(resp, w, &mut buf).await
    }

    /// Like [`Http1Response::write_to`] but serializes into a caller-provided
    /// scratch buffer, reused across requests on a keep-alive connection so the
    /// wire buffer is allocated once per connection rather than once per
    /// response. The buffer is cleared on entry (retaining its capacity).
    ///
    /// For a body of at least `VECTORED_BODY_THRESHOLD` bytes the body is
    /// **not copied into `buf`**: the head and the body `Bytes` are emitted with
    /// a single **vectored** write (`writev`) as two `IoSlice`s, eliminating the
    /// per-response body memcpy. On a stream that supports vectored I/O
    /// (`TcpStream`) this is one `writev` syscall — exactly as many as the
    /// coalesced single write it replaces; on a stream that does not, the drain
    /// loop falls back to sequential writes. Below the threshold (and for an
    /// empty body) the body is coalesced into `buf` and written once, identical
    /// to the pre-vectored writer. Either way the emitted bytes are
    /// byte-for-byte identical to [`write_to`].
    ///
    /// [`write_to`]: Http1Response::write_to
    pub async fn write_to_buf<W>(
        resp: &Response,
        w: &mut W,
        buf: &mut Vec<u8>,
    ) -> Result<(), Http1Error>
    where
        W: AsyncWrite + Unpin,
    {
        serialize_response_head(resp, buf);

        if resp.body.len() >= VECTORED_BODY_THRESHOLD {
            // Large body: emit head + body vectored, skipping the body memcpy.
            write_all_vectored(w, buf, &resp.body).await?;
        } else {
            // Small (or empty) body: coalesce into the head buffer and write
            // once. Byte- and cost-identical to the pre-vectored writer; a
            // sub-threshold memcpy is cheaper than the vectored path's overhead.
            buf.reserve(resp.body.len());
            buf.extend_from_slice(&resp.body);
            w.write_all(buf).await?;
        }
        w.flush().await?;
        Ok(())
    }
}

/// Serialize the response HEAD (status line + headers + terminating blank
/// line) into `buf`, clearing it first. Extracted from
/// [`Http1Response::write_to_buf`] so runtime-agnostic callers (the io_uring
/// data-plane prototype) share the exact serialization — byte-for-byte the
/// same wire head as the tokio writer.
pub(crate) fn serialize_response_head(resp: &Response, buf: &mut Vec<u8>) {
    let reason = resp.reason.unwrap_or_else(|| canonical_reason(resp.status));
    buf.clear();
    // Reserve for the head only — the body may be written vectored, not copied.
    buf.reserve(
        64 + resp
            .headers
            .iter()
            .map(|(n, v)| n.len() + v.len() + 4)
            .sum::<usize>(),
    );
    // Status line.
    buf.extend_from_slice(b"HTTP/1.1 ");
    {
        // Format the 3-digit status without the former per-response
        // `to_string()` heap allocation.
        use std::io::Write as _;
        let _ = write!(buf, "{}", resp.status);
    }
    buf.push(b' ');
    buf.extend_from_slice(reason.as_bytes());
    buf.extend_from_slice(b"\r\n");
    // Headers.
    for (name, value) in &resp.headers {
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }
    // Blank line terminating the head.
    buf.extend_from_slice(b"\r\n");
}

/// Write `head` followed by `body` to `w` with vectored I/O, draining across
/// partial writes. On a stream whose `is_write_vectored()` is true (e.g.
/// `TcpStream`) the kernel gathers both slices in a single `writev`; otherwise
/// the loop advances past whatever each call accepted (which may be only the
/// first slice) until both drain. Both slices are non-empty when called (the
/// empty-body case takes a plain `write_all`), so no zero-length slice reaches
/// the syscall.
async fn write_all_vectored<W>(w: &mut W, head: &[u8], body: &[u8]) -> Result<(), Http1Error>
where
    W: AsyncWrite + Unpin,
{
    use std::io::IoSlice;
    let mut storage = [IoSlice::new(head), IoSlice::new(body)];
    let mut slices: &mut [IoSlice<'_>] = &mut storage;
    while !slices.is_empty() {
        let n = w.write_vectored(slices).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "failed to write whole response",
            )
            .into());
        }
        IoSlice::advance_slices(&mut slices, n);
    }
    Ok(())
}

/// Fast-path writer for a PRE-SERIALIZED response head (see
/// `ClientStream::send_request_direct`): emits `head` + `body` with exactly
/// the same threshold/vectored strategy as [`Http1Response::write_to_buf`],
/// so the wire byte pattern is identical. `head` is the caller's reusable
/// buffer; the sub-threshold arm appends the body into it before the single
/// write (mirroring the coalesced arm of `write_to_buf`).
pub(crate) async fn write_head_and_body<W>(
    w: &mut W,
    head: &mut Vec<u8>,
    body: &[u8],
) -> Result<(), Http1Error>
where
    W: AsyncWrite + Unpin,
{
    if body.len() >= VECTORED_BODY_THRESHOLD {
        write_all_vectored(w, head, body).await?;
    } else {
        head.extend_from_slice(body);
        w.write_all(head).await?;
    }
    w.flush().await?;
    Ok(())
}

/// Canonical reason phrase for a status code per RFC 7231 §6.1.
/// Returns `"OK"` for unknown codes (matches Envoy's posture; cross-check
/// at execution time — if Envoy emits a different reason for an unknown
/// code, this fallback is harmless because the value-exact diff for the
/// status line is not part of the equivalence matrix).
pub(crate) fn canonical_reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        // 76.2: MEASURED on the wire against envoyproxy/envoy:v1.33.0 —
        // `HTTP/1.1 303 See Other` / `307 Temporary Redirect` /
        // `308 Permanent Redirect`. Before 76.2 all three fell through to
        // `_ => "OK"`, so a `SEE_OTHER` redirect emitted `HTTP/1.1 303 OK`.
        // The differential fixture CANNOT catch this — the harness parses the
        // status CODE only — so these three are pinned in-process.
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Payload Too Large",
        414 => "URI Too Long",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        505 => "HTTP Version Not Supported",
        _ => "OK",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::IoSlice;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::AsyncWrite;

    /// Run an async write into an in-memory Vec and return the bytes.
    async fn write_to_vec(resp: &Response) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        Http1Response::write_to(resp, &mut buf)
            .await
            .expect("write");
        buf
    }

    /// A sink that records exactly how the writer called it: how many plain
    /// vs. vectored writes, and the reassembled bytes. `max_chunk` caps the
    /// bytes accepted per poll (to force partial writes across the head/body
    /// slice boundary); `usize::MAX` accepts everything each call.
    struct MockSink {
        data: Vec<u8>,
        max_chunk: usize,
        write_calls: usize,
        write_vectored_calls: usize,
        vectored_supported: bool,
    }

    impl MockSink {
        fn new(max_chunk: usize, vectored_supported: bool) -> Self {
            Self {
                data: Vec::new(),
                max_chunk,
                write_calls: 0,
                write_vectored_calls: 0,
                vectored_supported,
            }
        }
    }

    impl AsyncWrite for MockSink {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            self.write_calls += 1;
            let n = buf.len().min(self.max_chunk);
            let bytes = buf[..n].to_vec();
            self.data.extend_from_slice(&bytes);
            Poll::Ready(Ok(n))
        }

        fn poll_write_vectored(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            bufs: &[IoSlice<'_>],
        ) -> Poll<std::io::Result<usize>> {
            self.write_vectored_calls += 1;
            let mut remaining = self.max_chunk;
            let mut total = 0usize;
            for s in bufs {
                if remaining == 0 {
                    break;
                }
                let take = s.len().min(remaining);
                let bytes = s[..take].to_vec();
                self.data.extend_from_slice(&bytes);
                remaining -= take;
                total += take;
            }
            Poll::Ready(Ok(total))
        }

        fn is_write_vectored(&self) -> bool {
            self.vectored_supported
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn writes_status_line_headers_body() {
        let resp = Response {
            status: 200,
            reason: None,
            headers: vec![
                ("server".to_string(), "envoy-rust".to_string()),
                (
                    "date".to_string(),
                    "Sun, 06 Nov 1994 08:49:37 GMT".to_string(),
                ),
                ("content-length".to_string(), "3".to_string()),
                ("content-type".to_string(), "text/plain".to_string()),
                ("connection".to_string(), "keep-alive".to_string()),
            ],
            body: Bytes::from_static(b"ok\n"),
        };
        let buf = write_to_vec(&resp).await;
        let expected: &[u8] = b"HTTP/1.1 200 OK\r\nserver: envoy-rust\r\ndate: Sun, 06 Nov 1994 08:49:37 GMT\r\ncontent-length: 3\r\ncontent-type: text/plain\r\nconnection: keep-alive\r\n\r\nok\n";
        assert_eq!(buf, expected, "wire bytes match");
    }

    #[tokio::test]
    async fn writes_204_with_no_body() {
        let resp = Response {
            status: 204,
            reason: None,
            headers: vec![
                ("server".to_string(), "envoy-rust".to_string()),
                ("connection".to_string(), "keep-alive".to_string()),
            ],
            body: Bytes::new(),
        };
        let buf = write_to_vec(&resp).await;
        let expected: &[u8] =
            b"HTTP/1.1 204 No Content\r\nserver: envoy-rust\r\nconnection: keep-alive\r\n\r\n";
        assert_eq!(buf, expected);
    }

    #[tokio::test]
    async fn write_to_buf_matches_write_to_and_reuses_buffer() {
        let a = Response {
            status: 200,
            reason: None,
            headers: vec![
                ("server".to_string(), "envoy-rust".to_string()),
                ("content-length".to_string(), "3".to_string()),
                ("connection".to_string(), "keep-alive".to_string()),
            ],
            body: Bytes::from_static(b"ok\n"),
        };
        let b = Response {
            status: 404,
            reason: None,
            headers: vec![
                ("server".to_string(), "envoy-rust".to_string()),
                ("content-length".to_string(), "0".to_string()),
                ("connection".to_string(), "close".to_string()),
            ],
            body: Bytes::new(),
        };

        // Reference bytes via the allocating path.
        let a_ref = write_to_vec(&a).await;
        let b_ref = write_to_vec(&b).await;

        // Drive both through one reused buffer, in sequence, into a shared sink;
        // each call must emit exactly the reference bytes (buffer is cleared on
        // entry, so no bleed-through from the previous, larger response).
        let mut scratch: Vec<u8> = Vec::new();

        let mut out_a: Vec<u8> = Vec::new();
        Http1Response::write_to_buf(&a, &mut out_a, &mut scratch)
            .await
            .expect("write a");
        assert_eq!(out_a, a_ref, "write_to_buf(a) == write_to(a)");

        let mut out_b: Vec<u8> = Vec::new();
        Http1Response::write_to_buf(&b, &mut out_b, &mut scratch)
            .await
            .expect("write b");
        assert_eq!(out_b, b_ref, "write_to_buf(b) == write_to(b) on reused buf");
    }

    /// Build a response whose body is `len` bytes of `x`.
    fn response_with_body(status: u16, len: usize) -> Response {
        Response {
            status,
            reason: None,
            headers: vec![
                ("server".to_string(), "envoy-rust".to_string()),
                ("content-length".to_string(), len.to_string()),
                ("connection".to_string(), "keep-alive".to_string()),
            ],
            body: Bytes::from(vec![b'x'; len]),
        }
    }

    /// A body at/above the vectored threshold is emitted vectored (head + body
    /// as two slices), the body is never copied into the head buffer, and the
    /// reassembled wire bytes are identical to the coalesced `write_to`.
    #[tokio::test]
    async fn large_body_writes_head_and_body_vectored() {
        let resp = response_with_body(200, 2048);
        assert!(resp.body.len() >= VECTORED_BODY_THRESHOLD);
        let reference = write_to_vec(&resp).await;

        let mut sink = MockSink::new(usize::MAX, true);
        let mut buf: Vec<u8> = Vec::new();
        Http1Response::write_to_buf(&resp, &mut sink, &mut buf)
            .await
            .expect("write");

        // Bytes on the wire match the coalesced reference exactly.
        assert_eq!(sink.data, reference, "vectored bytes == coalesced bytes");
        // The vectored path was taken (one writev), not a plain write.
        assert_eq!(sink.write_vectored_calls, 1, "one writev");
        assert_eq!(sink.write_calls, 0, "no plain write");
        // The body was NOT copied into the head buffer: buf holds only the head,
        // which ends at the blank-line separator and excludes the body bytes.
        assert!(buf.ends_with(b"\r\n\r\n"), "buf is head-only");
        assert_eq!(
            buf.len() + resp.body.len(),
            reference.len(),
            "buf holds head only; body length is the remainder",
        );
    }

    /// The drain loop reassembles correctly when the sink accepts only a few
    /// bytes per vectored call, forcing partial writes that cross the head/body
    /// slice boundary.
    #[tokio::test]
    async fn vectored_write_drains_across_partial_writes() {
        let resp = response_with_body(200, 2048);
        let reference = write_to_vec(&resp).await;

        // 7 bytes per call guarantees many calls and a boundary crossing.
        let mut sink = MockSink::new(7, true);
        let mut buf: Vec<u8> = Vec::new();
        Http1Response::write_to_buf(&resp, &mut sink, &mut buf)
            .await
            .expect("write");

        assert_eq!(
            sink.data, reference,
            "reassembled bytes match across partials"
        );
        assert!(
            sink.write_vectored_calls > 1,
            "partial writes forced multiple writev calls (got {})",
            sink.write_vectored_calls,
        );
    }

    /// A body below the vectored threshold is coalesced into the head buffer and
    /// written once (no writev), byte-identical to the pre-vectored writer.
    #[tokio::test]
    async fn small_body_coalesced_single_write() {
        let resp = response_with_body(200, 13);
        assert!(resp.body.len() < VECTORED_BODY_THRESHOLD);
        let reference = write_to_vec(&resp).await;

        let mut sink = MockSink::new(usize::MAX, true);
        let mut buf: Vec<u8> = Vec::new();
        Http1Response::write_to_buf(&resp, &mut sink, &mut buf)
            .await
            .expect("write");

        assert_eq!(sink.data, reference, "coalesced bytes == reference");
        assert_eq!(sink.write_calls, 1, "one plain write");
        assert_eq!(sink.write_vectored_calls, 0, "no writev below threshold");
        // The whole response (head + body) is in the buffer.
        assert_eq!(buf.len(), reference.len(), "buf holds head + body");
    }

    /// An empty-body response takes the plain-write path (no zero-length body
    /// slice reaches the syscall) and still emits identical bytes.
    #[tokio::test]
    async fn empty_body_takes_plain_write_path() {
        let resp = Response {
            status: 204,
            reason: None,
            headers: vec![
                ("server".to_string(), "envoy-rust".to_string()),
                ("connection".to_string(), "keep-alive".to_string()),
            ],
            body: Bytes::new(),
        };
        let reference = write_to_vec(&resp).await;

        let mut sink = MockSink::new(usize::MAX, true);
        let mut buf: Vec<u8> = Vec::new();
        Http1Response::write_to_buf(&resp, &mut sink, &mut buf)
            .await
            .expect("write");

        assert_eq!(sink.data, reference);
        assert_eq!(sink.write_calls, 1, "one plain write");
        assert_eq!(sink.write_vectored_calls, 0, "no writev for empty body");
    }

    /// 76.2: the three redirect reason phrases MEASURED on the wire against
    /// `envoyproxy/envoy:v1.33.0`. Before 76.2 all three fell through to
    /// `_ => "OK"`, so a `SEE_OTHER` redirect emitted `HTTP/1.1 303 OK`.
    /// The differential fixture CANNOT catch this — the harness parses the
    /// status CODE only — so this in-process pin is the ONLY guard.
    #[test]
    fn canonical_reason_covers_the_three_redirect_codes() {
        assert_eq!(canonical_reason(303), "See Other");
        assert_eq!(canonical_reason(307), "Temporary Redirect");
        assert_eq!(canonical_reason(308), "Permanent Redirect");
        // Guard the two that already worked, so a careless table edit is caught.
        assert_eq!(canonical_reason(301), "Moved Permanently");
        assert_eq!(canonical_reason(302), "Found");
    }
}
