//! AccessLogRecord — POD value-type carrying the 14 fields rendered by
//! the Envoy default-format access-log emitter (plus a leading
//! `start_time` SystemTime that the emitter formats per
//! `default_format::format_iso8601`).
//!
//! Built at HCM on-response-complete time by `envoy-http1::hcm`'s
//! factored join point; consumed (by reference) by
//! `default_format::format` and by `FileSink::emit`.

use std::time::{Duration, SystemTime};

/// AccessLogRecord — value-type carrying the per-request state that
/// the Envoy default-format emitter renders. 15 fields total: a
/// leading SystemTime for `%START_TIME%`, then 14 substitution
/// targets matching the Envoy default access-log format (one per
/// token).
///
/// Built at HCM on-response-complete time; consumed by reference by
/// the default-format emitter and the FileSink. Owns its String
/// fields so it can cross spawn boundaries cheaply if future code
/// switches to spawn-based dispatch (06.2 uses synchronous-after-
/// write per parent-06 architectural Rule 4 option (b)).
///
/// Intentionally does NOT implement `Default` (per 06.2 SPEC §6
/// signpost 14) — every field must be populated explicitly at the
/// HCM record-build site so silent omissions can't ship.
#[derive(Debug, Clone)]
pub struct AccessLogRecord {
    /// Wall-clock at request arrival. Rendered by
    /// `default_format::format_iso8601` as `YYYY-MM-DDTHH:MM:SS.sssZ`.
    pub start_time: SystemTime,

    /// HTTP method token (`GET` / `POST` / etc.).
    pub method: String,

    /// Path: either `X-Envoy-Original-Path` if the request carried
    /// that header, else the request-target/`:path` pseudo-header.
    pub path: String,

    /// `"HTTP/1.1"` on the H1 dispatch path, `"HTTP/2"` on the H2
    /// dispatch path.
    pub protocol: String,

    pub response_code: u16,

    /// Always `"-"` in 06.2 (Envoy's no-flags sentinel). Future
    /// phases that surface non-`-` flag combinations will populate
    /// this field with the appropriate flag token(s).
    pub response_flags: String,

    /// Wire-byte count of the request body. Header bytes NOT counted
    /// per Envoy's `%BYTES_RECEIVED%` semantic.
    pub bytes_received: u64,

    /// Wire-byte count of the response body.
    pub bytes_sent: u64,

    /// Per-request latency from request-arrival to record-build time.
    /// Rendered as integer milliseconds via `Duration::as_millis()`.
    pub duration: Duration,

    /// Value of the response's `x-envoy-upstream-service-time`
    /// header if present (parsed as `u64` ms), else `None`.
    pub upstream_service_time: Option<Duration>,

    /// Request-side `x-forwarded-for` header value if present.
    pub forwarded_for: Option<String>,

    /// Request-side `user-agent` header value if present.
    pub user_agent: Option<String>,

    /// Request-side `x-request-id` header value if present.
    pub request_id: Option<String>,

    /// Request-side `host` header value (or `:authority` pseudo-
    /// header on H2 — the codec translates pre-record-build) if
    /// present.
    pub authority: Option<String>,

    /// Resolved upstream endpoint formatted via `SocketAddr` Display
    /// impl (e.g., `127.0.0.1:8080` for IPv4, `[::1]:8080` for
    /// IPv6). `None` on direct_response paths.
    pub upstream_host: Option<String>,

    /// Config `name` of the cluster the request was routed to, if any (a
    /// non-proxy / direct_response path → `None`). Rendered by
    /// `%UPSTREAM_CLUSTER%` (phase 43) — mirrors `upstream_host`'s
    /// `Option<String>` (`Some`→the cluster name, `None`→absent → `-` sentinel
    /// / json `null`).
    pub upstream_cluster: Option<String>,

    /// Config `name` of the matched route, if the route is named (an empty
    /// route `name` = unnamed → `None`). Rendered by the `%ROUTE_NAME%`
    /// command-operator (phase 41) — mirrors `upstream_host`'s `Option<String>`
    /// (`Some`→the name, `None`→absent → `-` sentinel / json `null`).
    pub route_name: Option<String>,

    /// Envoy's response-code-details string for the reply (e.g.
    /// `direct_response` / `via_upstream`), set by the HCM per response-path; an
    /// `Option<String>` mirroring `route_name` — present → quoted/rendered,
    /// absent → `-` sentinel / json `null`. Rendered by `%RESPONSE_CODE_DETAILS%`
    /// (phase 42).
    pub response_code_details: Option<String>,

    /// Per-request dynamic metadata (namespace → key → string value), copied
    /// from the pipeline's `FilterRequest.dynamic_metadata` at the HCM
    /// record-build site (H1 hcm.rs ~1189, H2 hcm.rs ~888). Rendered by the
    /// `%DYNAMIC_METADATA(namespace:key)%` command-operator (phase 33).
    pub dynamic_metadata:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::UNIX_EPOCH;

    #[test]
    fn record_dynamic_metadata_defaults_empty_and_carries_values() {
        let empty = AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: None,
            upstream_host: None,
            upstream_cluster: None,
            route_name: None,
            response_code_details: None,
            dynamic_metadata: BTreeMap::new(),
        };
        assert!(empty.dynamic_metadata.is_empty());

        let mut dm: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        dm.entry("envoy.test".into())
            .or_default()
            .insert("tier".into(), "prod".into());
        let populated = AccessLogRecord {
            dynamic_metadata: dm,
            ..empty
        };
        assert_eq!(populated.dynamic_metadata["envoy.test"]["tier"], "prod");
    }

    #[test]
    fn record_route_name_defaults_and_carries_value() {
        let absent = AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: None,
            upstream_host: None,
            upstream_cluster: None,
            route_name: None,
            response_code_details: None,
            dynamic_metadata: BTreeMap::new(),
        };
        assert!(absent.route_name.is_none());

        let named = AccessLogRecord {
            route_name: Some("myroute".into()),
            ..absent
        };
        assert_eq!(named.route_name.as_deref(), Some("myroute"));
    }

    #[test]
    fn record_response_code_details_defaults_and_carries_value() {
        let absent = AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: None,
            upstream_host: None,
            upstream_cluster: None,
            route_name: None,
            response_code_details: None,
            dynamic_metadata: BTreeMap::new(),
        };
        assert!(absent.response_code_details.is_none());

        let detailed = AccessLogRecord {
            response_code_details: Some("direct_response".into()),
            ..absent
        };
        assert_eq!(
            detailed.response_code_details.as_deref(),
            Some("direct_response")
        );
    }

    #[test]
    fn record_upstream_cluster_defaults_and_carries_value() {
        let absent = AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: None,
            upstream_host: None,
            upstream_cluster: None,
            route_name: None,
            response_code_details: None,
            dynamic_metadata: BTreeMap::new(),
        };
        assert!(absent.upstream_cluster.is_none());

        let clustered = AccessLogRecord {
            upstream_cluster: Some("my_backend_cluster".into()),
            ..absent
        };
        assert_eq!(
            clustered.upstream_cluster.as_deref(),
            Some("my_backend_cluster")
        );
    }

    #[test]
    fn record_construction_full() {
        let record = AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: Some("envoy-rust.test".into()),
            upstream_host: None,
            upstream_cluster: None,
            route_name: None,
            response_code_details: None,
            dynamic_metadata: BTreeMap::new(),
        };
        let dbg = format!("{:?}", record);
        assert!(dbg.contains("method: \"GET\""), "debug output: {}", dbg);
        assert!(
            dbg.contains("authority: Some(\"envoy-rust.test\")"),
            "debug output: {}",
            dbg
        );
    }

    #[test]
    fn record_clone_is_deep_for_strings() {
        let original = AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: None,
            upstream_host: None,
            upstream_cluster: None,
            route_name: None,
            response_code_details: None,
            dynamic_metadata: BTreeMap::new(),
        };
        let mut clone = original.clone();
        clone.method = "POST".into();
        assert_eq!(original.method, "GET");
        assert_eq!(clone.method, "POST");
    }
}
