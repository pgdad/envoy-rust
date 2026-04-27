//! Error type for envoy-http1.

#[derive(Debug, thiserror::Error)]
pub enum Http1Error {
    #[error("malformed request line")]
    MalformedRequestLine,

    #[error("malformed header (bad token, missing colon, etc.)")]
    MalformedHeader,

    #[error("request headers exceed cap of {cap} bytes")]
    HeadersTooLarge { cap: usize },

    #[error("request body exceeds cap of {cap} bytes")]
    BodyTooLarge { cap: usize },

    #[error("unexpected EOF mid-message")]
    UnexpectedEof,

    #[error("io: {source}")]
    Io {
        #[source]
        source: std::io::Error,
    },

    /// 04.3 NEW: TCP-connect to an upstream cluster endpoint failed (e.g.,
    /// `ConnectionRefused`, `ETIMEDOUT`). Wraps the underlying `io::Error`.
    /// Surfaces from `Client::connect`; the router-proxy arm (Task 8 / Task 9)
    /// wraps this in `RouterError::UpstreamConnect { cluster, source }`.
    #[error("connecting to upstream {addr}: {source}")]
    UpstreamConnect {
        addr: std::net::SocketAddr,
        #[source]
        source: std::io::Error,
    },

    /// 04.3 NEW: upstream's HTTP/1.1 response status line was malformed.
    /// `httparse::Response::parse` returned a `Token` / `Version` / etc. error
    /// (mirrors `MalformedRequestLine`'s posture for outgoing requests).
    /// Surfaces from `ClientStream::send_request`'s response-parse step.
    #[error("malformed upstream response line")]
    MalformedResponseLine,

    /// 04.3 NEW: upstream's chunked-encoding framing violated RFC 7230 §4.1
    /// (e.g., non-hex chunk size, missing CRLF after chunk data, unexpected
    /// EOF mid-chunk). Surfaces from the chunked-encoding response reader
    /// in `client.rs` (Task 7).
    #[error("malformed chunked-encoding framing in upstream response")]
    MalformedChunkedFraming,
}

impl From<std::io::Error> for Http1Error {
    fn from(source: std::io::Error) -> Self {
        Self::Io { source }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_connect_display_includes_addr_and_source() {
        // 04.3 NEW: smoke-test the Display impl on Http1Error::UpstreamConnect.
        // The error surfaces in tracing::warn! log lines, so the human-readable
        // shape matters operationally.
        let addr: std::net::SocketAddr = "127.0.0.1:9999".parse().unwrap();
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let err = Http1Error::UpstreamConnect {
            addr,
            source: io_err,
        };
        let s = err.to_string();
        assert!(
            s.contains("connecting to upstream") && s.contains("127.0.0.1:9999"),
            "unexpected Display output: {s}"
        );
    }
}
