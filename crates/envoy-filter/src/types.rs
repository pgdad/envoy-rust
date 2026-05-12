//! Filter-visible request and response value types.
//!
//! These structs are the canonical shape filters operate on. They are
//! deliberately a subset of the HTTP/1.1 codec's `Request` / `Response`
//! shape — filters do not see codec-specific fields like the parser's
//! `bytes_consumed` offset or the H1 `HttpVersion` discriminator. The
//! HCMs (`envoy-http1`, `envoy-http2`) construct these at the
//! filter-invocation boundary and write the (possibly mutated) values
//! back into their codec-native types.
//!
//! Re-homed into `envoy-filter` per ADR-0031 to break the
//! `envoy-filter ↔ envoy-http1` Cargo crate-level dependency cycle that
//! the parent-07 SPEC §5 signpost 7 anticipated only at module-level
//! (Cargo treats whole crates as units; the SPEC's "no cycles because
//! codec module has no dependency on hcm module" reasoning does not
//! survive at the crate-graph level). See ADR-0031 for the resolution
//! rationale.

use bytes::Bytes;

/// Filter-visible request.
///
/// Fields mirror the access surface filters need: method, path,
/// header list (emission order preserving), and body bytes. Compare
/// `envoy_http1::Request` which additionally carries `version: HttpVersion`
/// (codec-state) and `bytes_consumed: usize` (parser-state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    /// Outgoing body bytes. `None` is treated as `Bytes::new()`
    /// (Content-Length: 0) — same convention as `envoy_http1::Request`.
    pub body: Option<Bytes>,
}

/// Filter-visible response.
///
/// Identical field set to `envoy_http1::Response` (status, reason,
/// headers, body). Re-homed here so `envoy-filter` does not depend on
/// `envoy-http1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterResponse {
    pub status: u16,
    pub reason: Option<&'static str>,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
}
