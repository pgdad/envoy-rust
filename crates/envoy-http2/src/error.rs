//! Typed errors for the envoy-http2 crate. See SPEC §3 D3.
//!
//! The enum carries variants for each codec-edge failure mode. Source-
//! preserving variants wrap `h2::Error` via `#[source]` so the original
//! framing-level diagnostic survives the type translation. No `From<h2::Error>`
//! blanket impl — call sites pick the right variant per failure context (e.g.,
//! handshake failure vs. stream accept failure vs. body-read failure).

#[derive(Debug, thiserror::Error)]
pub enum Http2Error {
    /// `h2::server::handshake` failed (no PRI preamble; bad SETTINGS; etc.).
    #[error("HTTP/2 handshake failed: {source}")]
    H2Handshake {
        #[source]
        source: h2::Error,
    },

    /// `h2::server::Connection::accept` returned a fatal error mid-connection.
    #[error("HTTP/2 stream accept failed: {source}")]
    H2StreamAccept {
        #[source]
        source: h2::Error,
    },

    /// Reading body bytes from `h2::RecvStream` failed.
    #[error("HTTP/2 body read failed: {source}")]
    H2BodyRead {
        #[source]
        source: h2::Error,
    },

    /// The H2 request carried no `:authority` pseudo-header. envoy-rust's HCM
    /// route-walk requires `Host:` (synthesized from `:authority`) per
    /// cross-sub-phase architectural rule 3.
    #[error(
        "HTTP/2 request missing :authority pseudo-header (required for Host-driven route-walk)"
    )]
    MissingAuthority,

    /// The H2 HEADERS block carried structurally invalid pseudo-headers
    /// (e.g., missing `:method`, missing `:path`, or an unrecognized
    /// pseudo-header name); OR carried a non-pseudo header NAME containing
    /// non-token bytes; OR carried a non-pseudo header VALUE containing
    /// non-UTF-8 bytes. The h2 codec normally catches structural problems
    /// earlier, but does not pre-reject obs-text bytes in header values
    /// (HPACK literal decoding accepts arbitrary bytes), so this variant is
    /// a defense-in-depth fallback at the HEADERS-block, header-name, and
    /// header-value layers.
    #[error("HTTP/2 header block is structurally malformed")]
    MalformedH2HeaderBlock,

    /// envoy-rust attempted to emit a status code outside the valid HTTP
    /// range (100..=599) on the H2 wire. Defense-in-depth — the route-walk
    /// validates status codes at parse time, so this should be unreachable
    /// from any valid config.
    #[error("invalid HTTP status code on H2 wire: {status}")]
    BadStatusCode { status: u16 },

    /// `tokio::net::TcpStream::connect` to the upstream endpoint failed.
    /// Sibling of `envoy_http1::Http1Error::UpstreamConnect`; raised at
    /// `Client::connect`'s outermost `?`. The `addr` field carries the
    /// resolved upstream `SocketAddr` (post-`pick_endpoint`); `source` is the
    /// underlying `std::io::Error` (typically `ConnectionRefused` /
    /// `TimedOut` / `HostUnreachable`).
    #[error("upstream H2 connect to {addr} failed: {source}")]
    UpstreamConnect {
        addr: std::net::SocketAddr,
        #[source]
        source: std::io::Error,
    },

    /// `h2::client::handshake` failed (the upstream did not complete the
    /// H2C preamble exchange — e.g., responded with HTTP/1.1 instead of an
    /// H2 SETTINGS frame, or closed the connection mid-handshake).
    /// Symmetric to the listener-side `H2Handshake` variant.
    #[error("client-side H2 handshake failed: {source}")]
    H2ClientHandshake {
        #[source]
        source: h2::Error,
    },

    /// `h2::client::SendRequest::send_request` or the subsequent
    /// `ResponseFuture` await failed. Covers send-stream initialization
    /// failures, peer GOAWAY mid-request, and response-future reset/cancel.
    #[error("client-side H2 send_request failed: {source}")]
    H2SendRequest {
        #[source]
        source: h2::Error,
    },

    /// Reading body bytes from the response-side `h2::RecvStream` failed
    /// (e.g., RST_STREAM mid-body, INTERNAL_ERROR on a stream after the
    /// response head was received). Symmetric to the listener-side
    /// `H2BodyRead` variant but on the inverse direction.
    #[error("client-side H2 response body read failed: {source}")]
    H2RecvBody {
        #[source]
        source: h2::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::Http2Error;

    #[test]
    fn missing_authority_displays_descriptively() {
        let e = Http2Error::MissingAuthority;
        let s = format!("{e}");
        assert!(
            s.contains("authority"),
            "expected mention of authority: {s}"
        );
    }

    #[test]
    fn bad_status_code_displays_value() {
        let e = Http2Error::BadStatusCode { status: 999 };
        let s = format!("{e}");
        assert!(s.contains("999"), "expected mention of 999: {s}");
    }

    #[test]
    fn h2_handshake_displays_with_source() {
        // Smoke-test the Display-with-source shape used by H2Handshake /
        // H2StreamAccept / H2BodyRead. h2::Error has a public construction
        // path via `From<h2::Reason>`; that's enough to assemble a real
        // wrapped-source variant for the format-string check.
        let src: h2::Error = h2::Reason::PROTOCOL_ERROR.into();
        let e = Http2Error::H2Handshake { source: src };
        let s = format!("{e}");
        assert!(
            s.starts_with("HTTP/2 handshake failed:"),
            "expected handshake prefix: {s}"
        );
    }

    #[test]
    fn upstream_connect_displays_with_addr_and_source() {
        let addr: std::net::SocketAddr = "127.0.0.1:9001".parse().unwrap();
        let src = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let e = Http2Error::UpstreamConnect { addr, source: src };
        let s = format!("{e}");
        assert!(
            s.contains("127.0.0.1:9001"),
            "expected addr in display: {s}"
        );
        assert!(s.contains("refused"), "expected source in display: {s}");
    }

    #[test]
    fn h2_client_handshake_displays_with_source() {
        let src: h2::Error = h2::Reason::PROTOCOL_ERROR.into();
        let e = Http2Error::H2ClientHandshake { source: src };
        let s = format!("{e}");
        assert!(
            s.starts_with("client-side H2 handshake failed:"),
            "expected client-handshake prefix: {s}"
        );
    }

    #[test]
    fn h2_send_request_displays_with_source() {
        let src: h2::Error = h2::Reason::REFUSED_STREAM.into();
        let e = Http2Error::H2SendRequest { source: src };
        let s = format!("{e}");
        assert!(
            s.starts_with("client-side H2 send_request failed:"),
            "expected send_request prefix: {s}"
        );
    }

    #[test]
    fn h2_recv_body_displays_with_source() {
        let src: h2::Error = h2::Reason::INTERNAL_ERROR.into();
        let e = Http2Error::H2RecvBody { source: src };
        let s = format!("{e}");
        assert!(
            s.starts_with("client-side H2 response body read failed:"),
            "expected recv_body prefix: {s}"
        );
    }
}
