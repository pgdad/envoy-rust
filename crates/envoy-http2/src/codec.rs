//! H2 codec adapter: maps envoy-config Http2ProtocolOptions onto
//! h2::server::Builder. See SPEC §3 D3.
//!
//! Thin adapter — the actual H2 codec lives in the `h2` crate. This module
//! exists to centralize the Http2ProtocolOptions → Builder field-by-field
//! mapping so the HCM and (in 05.3) the Client share the same configuration
//! shape. Only the listener-side Builder is mapped here in 05.2; the
//! client-side `h2::client::Builder` mapping lands in 05.3 alongside `client.rs`.

use envoy_config::Http2ProtocolOptions;

/// Build an `h2::server::Builder` configured per the given options. Absent
/// options leave the field at the `h2`-crate default.
pub fn build_h2_server(opts: Option<&Http2ProtocolOptions>) -> h2::server::Builder {
    let mut builder = h2::server::Builder::new();
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
}
