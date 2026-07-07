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
    /// Per-request dynamic-metadata store (namespace → key → string value),
    /// written by `envoy.filters.http.set_metadata` (phase 33) and read by the
    /// HCM record-build into `AccessLogRecord.dynamic_metadata`. Default-empty;
    /// string-only (a non-string Value enum is the §2.2 deferral). A plain
    /// `std::collections::BTreeMap` — NO new crate, NO shared Value type.
    pub dynamic_metadata:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
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

impl FilterResponse {
    /// Local-reply constructor for filters' static short-circuit responses:
    /// no headers (`content-type` / `content-length` / `server` etc. are
    /// stamped by the H1/H2 synth decorators downstream), status/reason/body
    /// passed through verbatim.
    pub(crate) fn static_reply(
        status: u16,
        reason: Option<&'static str>,
        body: &'static [u8],
    ) -> Self {
        Self {
            status,
            reason,
            headers: Vec::new(),
            body: Bytes::from_static(body),
        }
    }
}

/// Case-insensitive header lookup (first match wins). Shared by the
/// jwt_authn / cors / csrf / header_to_metadata filters.
pub(crate) fn header_ci<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

// ---------------------------------------------------------------------------
// Shared test support
// ---------------------------------------------------------------------------

#[cfg(test)]
impl FilterRequest {
    /// Canonical test request: `body: None`, empty dynamic metadata.
    pub(crate) fn test(method: &str, path: &str, headers: &[(&str, &str)]) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: None,
            dynamic_metadata: std::collections::BTreeMap::new(),
        }
    }
}

#[cfg(test)]
impl FilterResponse {
    /// Canonical empty 200 test response (no reason, no headers, empty body).
    pub(crate) fn test_200() -> Self {
        Self {
            status: 200,
            reason: None,
            headers: vec![],
            body: Bytes::new(),
        }
    }
}

/// Test-support: a Route with a `DirectResponse(200)` action carrying a single
/// `typed_per_filter_config` entry keyed by `filter_name`.
#[cfg(test)]
pub(crate) fn test_route_with_pfc(
    filter_name: &str,
    pfc: envoy_config::PerFilterConfig,
) -> envoy_config::Route {
    let mut map = std::collections::BTreeMap::new();
    map.insert(filter_name.to_string(), pfc);
    test_route_with_pfc_map(map)
}

/// Test-support: the `test_route_with_pfc` base with an arbitrary (possibly
/// empty) `typed_per_filter_config` map.
#[cfg(test)]
pub(crate) fn test_route_with_pfc_map(
    typed_per_filter_config: std::collections::BTreeMap<String, envoy_config::PerFilterConfig>,
) -> envoy_config::Route {
    envoy_config::Route {
        name: String::new(),
        r#match: envoy_config::RouteMatch {
            prefix: Some("/".to_string()),
            path: None,
            headers: vec![],
        },
        action: envoy_config::RouteAction::DirectResponse(envoy_config::DirectResponse {
            status: 200,
            body: envoy_config::DataSource {
                filename: None,
                inline_string: None,
            },
        }),
        typed_per_filter_config,
    }
}

/// Test-support: a `HeaderMatcher` with an exact-value `StringMatch` mode.
#[cfg(test)]
pub(crate) fn header_matcher_exact(name: &str, exact: &str) -> envoy_config::HeaderMatcher {
    envoy_config::HeaderMatcher {
        name: name.to_string(),
        mode: envoy_config::HeaderMatcherMode::StringMatch(envoy_config::StringMatcher {
            mode: envoy_config::StringMatcherMode::Exact(exact.to_string()),
            ignore_case: false,
        }),
        invert_match: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_request_dynamic_metadata_defaults_empty_and_is_writable() {
        let mut r = FilterRequest::test("GET", "/", &[]);
        assert!(r.dynamic_metadata.is_empty());
        r.dynamic_metadata
            .entry("ns".into())
            .or_default()
            .insert("k".into(), "v".into());
        assert_eq!(r.dynamic_metadata["ns"]["k"], "v");
    }
}
