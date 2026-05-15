//! `AdminEndpoint` enum + per-endpoint response builders. Exact-match path
//! routing only in 06.1 per cross-sub-phase architectural rule 5.

use bytes::{Bytes, BytesMut};
use envoy_stats::StatsRegistry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminEndpoint {
    /// `GET /ready` — returns 200 "LIVE\n" once the server has bound its
    /// listeners. Phase-08's drain semantics introduce 503 PRE_INITIALIZING
    /// and 503 DRAINING states; in 06.1 the endpoint always returns 200.
    Ready,

    /// `GET /stats` — returns 200 with body in plain-text "name: value\n"
    /// per-line format (matches Envoy's default `/stats` format under
    /// `format=` absence).
    Stats,

    /// `GET /stats/prometheus` — returns 200 with body in Prometheus
    /// text-exposition format per envoy_stats::prometheus::write_exposition.
    StatsPrometheus,
}

/// Method-aware dispatch result. Introduced at phase 08.1 D4 to give every
/// endpoint a structurally-declared 405-method-allowlist surface (closes 06.1
/// REVIEW M1 structurally). 08.2 POST endpoints plug in additively via new
/// `AdminEndpoint` variants whose `allowed_method` returns `"POST"`; no
/// further refactor of `Dispatch` is needed.
#[derive(Debug, PartialEq, Eq)]
pub enum Dispatch {
    Endpoint(AdminEndpoint),
    NotFound,
    MethodNotAllowed { allow: &'static str },
}

impl AdminEndpoint {
    /// Exact-match URL path lookup. Returns `None` for unknown paths
    /// (caller produces 404). Case-sensitive per Envoy v1.33.
    pub fn from_path(path: &str) -> Option<Self> {
        match path {
            "/ready" => Some(AdminEndpoint::Ready),
            "/stats" => Some(AdminEndpoint::Stats),
            "/stats/prometheus" => Some(AdminEndpoint::StatsPrometheus),
            _ => None,
        }
    }

    /// The HTTP method this endpoint accepts. 08.1's 4 new GET endpoints
    /// (ConfigDump, ServerInfo, Clusters, Listeners) declare `"GET"` here;
    /// 08.2's POST endpoints will declare `"POST"`.
    pub fn allowed_method(&self) -> &'static str {
        match self {
            AdminEndpoint::Ready => "GET",
            AdminEndpoint::Stats => "GET",
            AdminEndpoint::StatsPrometheus => "GET",
            // Tasks 6-9 add: ConfigDump | ServerInfo | Clusters | Listeners => "GET",
        }
    }

    /// Method-aware dispatch. Returns:
    /// - `Endpoint(e)` on a method+path match,
    /// - `NotFound` on an unknown path (regardless of method),
    /// - `MethodNotAllowed { allow }` on a known path with the wrong method.
    pub fn dispatch(method: &str, path: &str) -> Dispatch {
        match AdminEndpoint::from_path(path) {
            None => Dispatch::NotFound,
            Some(endpoint) => {
                let allow = endpoint.allowed_method();
                if method == allow {
                    Dispatch::Endpoint(endpoint)
                } else {
                    Dispatch::MethodNotAllowed { allow }
                }
            }
        }
    }

    /// Render the response for this endpoint. Reads the registry only on
    /// the `Stats` / `StatsPrometheus` arms; `Ready` ignores the registry.
    pub fn render(&self, registry: &StatsRegistry) -> envoy_http1::Response {
        match self {
            AdminEndpoint::Ready => Self::render_ready(),
            AdminEndpoint::Stats => Self::render_stats(registry),
            AdminEndpoint::StatsPrometheus => Self::render_stats_prometheus(registry),
        }
    }

    fn render_ready() -> envoy_http1::Response {
        let body = Bytes::from_static(b"LIVE\n");
        envoy_http1::Response {
            status: 200,
            reason: Some("OK"),
            headers: vec![
                ("content-type".to_string(), "text/plain".to_string()),
                ("content-length".to_string(), body.len().to_string()),
            ],
            body,
        }
    }

    fn render_stats(registry: &StatsRegistry) -> envoy_http1::Response {
        let mut buf = BytesMut::new();
        for (name, handle) in registry.snapshot() {
            use envoy_stats::StatHandle;
            use std::fmt::Write as _;
            match handle {
                StatHandle::Counter(c) => {
                    let _ = writeln!(&mut buf, "{name}: {}", c.value());
                }
                StatHandle::Gauge(g) => {
                    let _ = writeln!(&mut buf, "{name}: {}", g.value());
                }
            }
        }
        let body = buf.freeze();
        envoy_http1::Response {
            status: 200,
            reason: Some("OK"),
            headers: vec![
                ("content-type".to_string(), "text/plain".to_string()),
                ("content-length".to_string(), body.len().to_string()),
            ],
            body,
        }
    }

    fn render_stats_prometheus(registry: &StatsRegistry) -> envoy_http1::Response {
        let mut buf = BytesMut::new();
        envoy_stats::prometheus::write_exposition(registry, &mut buf);
        let body = buf.freeze();
        envoy_http1::Response {
            status: 200,
            reason: Some("OK"),
            headers: vec![
                // Mirror upstream Envoy 1.33's `/stats/prometheus`
                // content-type verbatim: `text/plain; charset=UTF-8`.
                // (The Prometheus-spec value `text/plain; version=0.0.4;
                // charset=utf-8` is what Prometheus exposition strictly
                // documents, but upstream Envoy emits the un-versioned
                // form; envoy-rust mirrors per D-3.3 doctrine — empirical
                // verification landed in 06.1 fixture 0011.)
                (
                    "content-type".to_string(),
                    "text/plain; charset=UTF-8".to_string(),
                ),
                ("content-length".to_string(), body.len().to_string()),
            ],
            body,
        }
    }
}

/// Render a 404 for unknown admin paths. Used by `AdminHandler::handle_inner`
/// when `from_path` returns `None`.
pub(crate) fn render_404() -> envoy_http1::Response {
    let body = Bytes::from_static(b"unknown admin endpoint\n");
    envoy_http1::Response {
        status: 404,
        reason: Some("Not Found"),
        headers: vec![
            ("content-type".to_string(), "text/plain".to_string()),
            ("content-length".to_string(), body.len().to_string()),
        ],
        body,
    }
}

/// Render a 405 for method-not-allowed responses. Used by
/// `AdminHandler::handle_inner` via `Dispatch::MethodNotAllowed { allow }`.
///
/// Phase 08.1 D4 widens the previously-fixed `Allow:` header value to a
/// per-endpoint dynamic value sourced from the `Dispatch::MethodNotAllowed`
/// arm. The body is regenerated dynamically too — closes 06.1 REVIEW M1
/// structurally: every endpoint variant declares its own allowed method.
pub(crate) fn render_405(allow: &'static str) -> envoy_http1::Response {
    let body = Bytes::from(format!("Method not allowed. Allow: {allow}\n"));
    envoy_http1::Response {
        status: 405,
        // Task 1's reason_for_status renders "Method Not Allowed" when reason
        // is None; leaving it None lets the helper supply the canonical text.
        reason: None,
        headers: vec![
            ("content-type".to_string(), "text/plain".to_string()),
            ("content-length".to_string(), body.len().to_string()),
            ("allow".to_string(), allow.to_string()),
        ],
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_path_ready_matches_exact() {
        assert_eq!(
            AdminEndpoint::from_path("/ready"),
            Some(AdminEndpoint::Ready)
        );
        assert_eq!(AdminEndpoint::from_path("/ready/"), None);
        assert_eq!(AdminEndpoint::from_path("/Ready"), None);
        assert_eq!(AdminEndpoint::from_path("/ready/foo"), None);
    }

    #[test]
    fn from_path_stats_matches_exact() {
        assert_eq!(
            AdminEndpoint::from_path("/stats"),
            Some(AdminEndpoint::Stats)
        );
    }

    #[test]
    fn from_path_stats_prometheus_matches_exact() {
        assert_eq!(
            AdminEndpoint::from_path("/stats/prometheus"),
            Some(AdminEndpoint::StatsPrometheus)
        );
    }

    #[test]
    fn from_path_unknown_returns_none() {
        assert_eq!(AdminEndpoint::from_path("/clusters"), None);
        assert_eq!(AdminEndpoint::from_path(""), None);
        assert_eq!(AdminEndpoint::from_path("/"), None);
    }

    #[test]
    fn render_ready_returns_200_live() {
        let reg = StatsRegistry::new();
        let resp = AdminEndpoint::Ready.render(&reg);
        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, Some("OK"));
        assert_eq!(&resp.body[..], b"LIVE\n");
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| k == "content-type" && v == "text/plain")
        );
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| k == "content-length" && v == "5")
        );
    }

    #[test]
    fn render_stats_text_format() {
        let reg = StatsRegistry::new();
        let c = reg
            .register_counter("listener.foo.downstream_cx_total")
            .unwrap();
        c.add(7);
        let resp = AdminEndpoint::Stats.render(&reg);
        assert_eq!(resp.status, 200);
        let body_str = std::str::from_utf8(&resp.body).unwrap();
        assert!(body_str.contains("listener.foo.downstream_cx_total: 7\n"));
    }

    #[test]
    fn render_stats_prometheus_format() {
        let reg = StatsRegistry::new();
        let c = reg
            .register_counter("listener.foo.downstream_cx_total")
            .unwrap();
        c.add(7);
        let resp = AdminEndpoint::StatsPrometheus.render(&reg);
        assert_eq!(resp.status, 200);
        let body_str = std::str::from_utf8(&resp.body).unwrap();
        assert!(body_str.contains("# TYPE envoy_listener_foo_downstream_cx_total counter\n"));
        assert!(body_str.contains("envoy_listener_foo_downstream_cx_total 7\n"));
    }

    #[test]
    fn render_response_carries_correct_content_type() {
        let reg = StatsRegistry::new();
        let stats = AdminEndpoint::Stats.render(&reg);
        assert!(
            stats
                .headers
                .iter()
                .any(|(k, v)| k == "content-type" && v == "text/plain")
        );

        let prom = AdminEndpoint::StatsPrometheus.render(&reg);
        assert!(
            prom.headers
                .iter()
                .any(|(k, v)| k == "content-type" && v == "text/plain; charset=UTF-8")
        );
    }

    #[test]
    fn render_404_body_and_status() {
        let r = render_404();
        assert_eq!(r.status, 404);
        assert_eq!(r.reason, Some("Not Found"));
        assert_eq!(&r.body[..], b"unknown admin endpoint\n");
    }

    #[test]
    fn render_405_carries_allow_get_header() {
        let r = render_405("GET");
        assert_eq!(r.status, 405);
        assert!(r.headers.iter().any(|(k, v)| k == "allow" && v == "GET"));
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::{AdminEndpoint, Dispatch};

    #[test]
    fn get_known_path_returns_endpoint() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/ready"),
            Dispatch::Endpoint(AdminEndpoint::Ready)
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/stats"),
            Dispatch::Endpoint(AdminEndpoint::Stats)
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/stats/prometheus"),
            Dispatch::Endpoint(AdminEndpoint::StatsPrometheus)
        ));
    }

    #[test]
    fn unknown_path_returns_not_found_regardless_of_method() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/nope"),
            Dispatch::NotFound
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/nope"),
            Dispatch::NotFound
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("DELETE", "/"),
            Dispatch::NotFound
        ));
    }

    #[test]
    fn known_path_wrong_method_returns_method_not_allowed_with_get_in_allow() {
        match AdminEndpoint::dispatch("POST", "/ready") {
            Dispatch::MethodNotAllowed { allow } => assert_eq!(allow, "GET"),
            other => panic!("expected MethodNotAllowed; got {other:?}"),
        }
        match AdminEndpoint::dispatch("PUT", "/stats") {
            Dispatch::MethodNotAllowed { allow } => assert_eq!(allow, "GET"),
            other => panic!("expected MethodNotAllowed; got {other:?}"),
        }
        match AdminEndpoint::dispatch("DELETE", "/stats/prometheus") {
            Dispatch::MethodNotAllowed { allow } => assert_eq!(allow, "GET"),
            other => panic!("expected MethodNotAllowed; got {other:?}"),
        }
    }

    #[test]
    fn method_match_is_case_sensitive_exact() {
        // Envoy's admin API treats HTTP method names case-sensitively (uppercase
        // canonical per RFC 7230). Mixed-case methods are NOT recognized.
        assert!(matches!(
            AdminEndpoint::dispatch("get", "/ready"),
            Dispatch::MethodNotAllowed { .. }
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("Get", "/ready"),
            Dispatch::MethodNotAllowed { .. }
        ));
    }

    #[test]
    fn each_endpoint_declares_its_allowed_method() {
        // Compile-time tautology: if any variant fails to declare ALLOWED, this
        // fails to compile.
        assert_eq!(AdminEndpoint::Ready.allowed_method(), "GET");
        assert_eq!(AdminEndpoint::Stats.allowed_method(), "GET");
        assert_eq!(AdminEndpoint::StatsPrometheus.allowed_method(), "GET");
    }

    #[test]
    fn dispatch_is_disjoint_from_from_path() {
        // from_path is retained as a thin convenience but does NOT route through
        // dispatch. Direct unit test that both surfaces remain available.
        assert!(AdminEndpoint::from_path("/ready").is_some());
        assert!(AdminEndpoint::from_path("/nope").is_none());
    }
}
