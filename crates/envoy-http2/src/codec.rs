//! H2 codec adapter: maps envoy-config Http2ProtocolOptions onto
//! h2::server::Builder. See SPEC §3 D3.
//!
//! Thin adapter — the actual H2 codec lives in the `h2` crate. This module
//! exists to centralize the Http2ProtocolOptions → Builder field-by-field
//! mapping so the HCM and (in 05.3) the Client share the same configuration
//! shape. Only the listener-side Builder is mapped here in 05.2; the
//! client-side `h2::client::Builder` mapping lands in 05.3 alongside `client.rs`.

use envoy_config::Http2ProtocolOptions;

/// Default bound on the decoded size of an inbound header list (the
/// `SETTINGS_MAX_HEADER_LIST_SIZE` we advertise AND enforce on receipt).
///
/// Upstream Envoy bounds request headers at `max_request_headers_kb`
/// (HCM-level, default **60 KiB**); the `h2` crate's receive-side default is
/// 16 MiB (`framed_read::DEFAULT_SETTINGS_MAX_HEADER_LIST_SIZE`) — a ~273×
/// wider memory-amplification window per stream than Envoy grants, and wildly
/// asymmetric with envoy-http1's 8 KiB request-headers cap. envoy-config's
/// `Http2ProtocolOptions` has no field for this (Envoy's knob lives on the
/// HCM, not on `Http2ProtocolOptions`), so the bound is a constant applied
/// unconditionally. Test-guarded by
/// `hcm::tests::h2_oversized_request_header_list_is_rejected`.
pub const DEFAULT_MAX_HEADER_LIST_SIZE: u32 = 60 * 1024;

/// Build an `h2::server::Builder` configured per the given options. Absent
/// options leave the field at the `h2`-crate default, EXCEPT
/// `max_header_list_size`, which is always pinned to
/// [`DEFAULT_MAX_HEADER_LIST_SIZE`] (see its doc for why).
pub fn build_h2_server(opts: Option<&Http2ProtocolOptions>) -> h2::server::Builder {
    let mut builder = h2::server::Builder::new();
    builder.max_header_list_size(DEFAULT_MAX_HEADER_LIST_SIZE);
    if let Some(o) = opts {
        if let Some(v) = o.max_concurrent_streams {
            builder.max_concurrent_streams(v);
        }
        // h2's setter is named `initial_window_size` (no `_stream_` infix);
        // envoy-config retains the proto-canonical `initial_stream_window_size`.
        if let Some(v) = o.initial_stream_window_size {
            builder.initial_window_size(v);
        }
        if let Some(v) = o.initial_connection_window_size {
            builder.initial_connection_window_size(v);
        }
        if let Some(v) = o.max_frame_size {
            builder.max_frame_size(v);
        }
    }
    builder
}

/// Thin wrapper around `h2::server::handshake`. Used by external test
/// helpers (`tests/helpers/http2-echo-server/`) so they can consume
/// `envoy_http2` instead of `h2` directly per parent-05 SPEC §6 signpost 7
/// (mirrors 04.3's `http1-echo-server` consuming `envoy_http1` over direct
/// `httparse`). Production code uses `build_h2_server` + `Builder::handshake`
/// directly; this re-export only exists to satisfy the architectural rule
/// that only `envoy-http2` depends on `h2` workspace-wide.
pub async fn server_handshake(
    tcp: tokio::net::TcpStream,
) -> Result<h2::server::Connection<tokio::net::TcpStream, bytes::Bytes>, crate::Http2Error> {
    h2::server::handshake(tcp)
        .await
        .map_err(|source| crate::Http2Error::H2Handshake { source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::Http2ProtocolOptions;

    #[test]
    fn build_h2_server_applies_protocol_options() {
        let opts = Http2ProtocolOptions {
            max_concurrent_streams: Some(50),
            initial_stream_window_size: Some(131072),
            initial_connection_window_size: Some(262144),
            max_frame_size: Some(32768),
        };
        // The function returns a configured builder; we cannot easily
        // introspect the builder's private fields, so we just verify the
        // call compiles and returns a builder that can be subsequently
        // used (smoke test). The actual behavioral verification is in the
        // hcm.rs `h2_protocol_options_max_concurrent_streams_applied` test
        // (Task 9), which observes the wire effect.
        let _builder = build_h2_server(Some(&opts));
        let _builder_default = build_h2_server(None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn server_handshake_accepts_h2_connection() {
        // End-to-end smoke: spawn a 127.0.0.1 listener, do a parallel
        // h2::client::handshake from a separate task, assert the server-side
        // server_handshake returns Ok.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            server_handshake(tcp).await
        });
        let client_task = tokio::spawn(async move {
            let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
            h2::client::handshake(tcp).await
        });
        let (server_result, client_result) = tokio::join!(server_task, client_task);
        assert!(server_result.unwrap().is_ok());
        assert!(client_result.unwrap().is_ok());
    }
}
