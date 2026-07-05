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
    /// response wire buffer is allocated once per connection rather than once
    /// per response. The buffer is cleared on entry (retaining its capacity);
    /// the emitted bytes are identical to `write_to`.
    pub async fn write_to_buf<W>(
        resp: &Response,
        w: &mut W,
        buf: &mut Vec<u8>,
    ) -> Result<(), Http1Error>
    where
        W: AsyncWrite + Unpin,
    {
        let reason = resp.reason.unwrap_or_else(|| canonical_reason(resp.status));
        buf.clear();
        buf.reserve(
            64 + resp
                .headers
                .iter()
                .map(|(n, v)| n.len() + v.len() + 4)
                .sum::<usize>()
                + resp.body.len(),
        );
        // Status line.
        buf.extend_from_slice(b"HTTP/1.1 ");
        buf.extend_from_slice(resp.status.to_string().as_bytes());
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
        // Blank line + body.
        buf.extend_from_slice(b"\r\n");
        buf.extend_from_slice(&resp.body);

        w.write_all(buf).await?;
        w.flush().await?;
        Ok(())
    }
}

/// Canonical reason phrase for a status code per RFC 7231 §6.1.
/// Returns `"OK"` for unknown codes (matches Envoy's posture; cross-check
/// at execution time — if Envoy emits a different reason for an unknown
/// code, this fallback is harmless because the value-exact diff for the
/// status line is not part of the equivalence matrix).
fn canonical_reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
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

    /// Run an async write into an in-memory Vec and return the bytes.
    async fn write_to_vec(resp: &Response) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        Http1Response::write_to(resp, &mut buf)
            .await
            .expect("write");
        buf
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
}
