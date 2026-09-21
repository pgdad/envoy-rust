//! The `envoy.filters.http.health_check` runtime filter — non-pass-through
//! mode (phase 115).
//!
//! Decode-side only. A request matching EVERY configured `HeaderMatcher` is
//! answered at the proxy — 200, empty body, one
//! `x-envoy-upstream-healthchecked-cluster` header — and never reaches the
//! route. Anything else continues down the chain. All of it MEASURED against
//! `envoyproxy/envoy:v1.33.0` (phase-115 `SPEC.md` §2 and `ADR-0201`):
//!
//! * the matcher list is AND-combined, and an EMPTY list matches everything;
//! * `:path` is matched WITH its query string, so `/healthz?x=1` does not
//!   match `exact: /healthz`;
//! * the filter is method-agnostic;
//! * the header value is the bootstrap `node.cluster`, never a request echo;
//! * the `http.<stat_prefix>.health_check.*` counters are all registered at
//!   build time, and an intercept ticks `request_total` and `ok`.

use std::sync::Arc;

use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// The response header every intercepted probe carries.
pub const X_ENVOY_UPSTREAM_HEALTHCHECKED_CLUSTER: &str = "x-envoy-upstream-healthchecked-cluster";

/// `%RESPONSE_CODE_DETAILS%` of an intercepted probe.
pub const HEALTH_CHECK_OK: &str = "health_check_ok";

/// The eight counters upstream registers under `http.<stat_prefix>.health_check.`
/// (MEASURED). Only `request_total` and `ok` move in non-pass-through mode;
/// the rest stay 0 (their triggers are CF-115-1 / CF-115-2 / CF-115-5).
const STAT_NAMES: [&str; 8] = [
    "cached_response",
    "degraded",
    "failed",
    "failed_cluster_empty",
    "failed_cluster_not_found",
    "failed_cluster_unhealthy",
    "ok",
    "request_total",
];

/// The one pseudo-header the filter can match. Neither codec puts
/// pseudo-headers into `FilterRequest::headers`, so a `:path` matcher is
/// evaluated against a one-entry view holding `FilterRequest::path` verbatim.
/// Every other `:`-prefixed name is rejected at config load (CF-115-6).
const PATH_PSEUDO_HEADER: &str = ":path";

/// The `envoy.filters.http.health_check` runtime filter.
#[derive(Debug, Clone)]
pub struct HealthCheckFilter {
    headers: Vec<envoy_config::HeaderMatcher>,
    local_cluster: String,
    request_total: Arc<Counter>,
    ok: Arc<Counter>,
}

impl HealthCheckFilter {
    /// Lower a config, compiling any `SafeRegex` on an owned clone so a bad
    /// pattern is build-fatal rather than a first-request panic, and register
    /// the eight `http.{hcm_stat_prefix}.health_check.*` counters.
    pub fn build_from_config(
        cfg: &envoy_config::HealthCheckFilterConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        let mut headers = cfg.headers.clone();
        for m in &mut headers {
            m.compile_safe_regexes()
                .map_err(|e| FilterError::InvalidConfig {
                    message: e.to_string(),
                })?;
        }
        let mut counters = Vec::with_capacity(STAT_NAMES.len());
        for name in STAT_NAMES {
            counters.push(crate::error::register_counter(
                registry,
                &format!("http.{hcm_stat_prefix}.health_check.{name}"),
            )?);
        }
        // Bound by NAME, not index: the two always tick together, so no test
        // could tell a swapped pair apart (phase-115 `REVIEW.md` F-5).
        let counter = |name: &str| {
            let i = STAT_NAMES
                .iter()
                .position(|n| *n == name)
                .expect("a STAT_NAMES entry");
            Arc::clone(&counters[i])
        };
        Ok(Self {
            headers,
            local_cluster: cfg.local_cluster.clone(),
            request_total: counter("request_total"),
            ok: counter("ok"),
        })
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        if !self.matches(req) {
            return Decision::Continue;
        }
        self.request_total.inc();
        self.ok.inc();
        Decision::StopAndSend(FilterResponse {
            status: 200,
            reason: None,
            headers: vec![(
                X_ENVOY_UPSTREAM_HEALTHCHECKED_CLUSTER.to_string(),
                self.local_cluster.clone(),
            )],
            body: Bytes::new(),
            details: Some(HEALTH_CHECK_OK),
            headers_only: true,
        })
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }

    /// AND over every matcher (`Iterator::all`, so an empty list is `true`).
    fn matches(&self, req: &FilterRequest) -> bool {
        let path_view = [(PATH_PSEUDO_HEADER.to_string(), req.path.clone())];
        self.headers.iter().all(|m| {
            if m.name.eq_ignore_ascii_case(PATH_PSEUDO_HEADER) {
                m.matches(&path_view)
            } else {
                m.matches(&req.headers)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::header_matcher_exact;
    use envoy_config::{HeaderMatcher, HealthCheckFilterConfig};

    fn cfg(headers: Vec<HeaderMatcher>) -> HealthCheckFilterConfig {
        HealthCheckFilterConfig {
            pass_through_mode: false,
            headers,
            cache_time: None,
            cluster_min_healthy_percentages: None,
            local_cluster: "hc-node-cluster".to_string(),
        }
    }

    fn filter(headers: Vec<HeaderMatcher>) -> HealthCheckFilter {
        let registry = Arc::new(StatsRegistry::new());
        HealthCheckFilter::build_from_config(&cfg(headers), &registry, "ingress_http")
            .expect("builds")
    }

    fn healthz() -> Vec<HeaderMatcher> {
        vec![header_matcher_exact(":path", "/healthz")]
    }

    /// `Some(response)` when intercepted, `None` when the request continues.
    fn run(
        f: &mut HealthCheckFilter,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
    ) -> Option<FilterResponse> {
        match f.decode_headers(&mut FilterRequest::test(method, path, headers)) {
            Decision::StopAndSend(resp) => Some(resp),
            Decision::Continue => None,
        }
    }

    #[test]
    fn matched_probe_is_answered_locally() {
        let resp = run(&mut filter(healthz()), "GET", "/healthz", &[]).expect("intercepted");
        assert_eq!(resp.status, 200);
        assert!(resp.body.is_empty());
        assert_eq!(
            resp.headers,
            vec![(
                "x-envoy-upstream-healthchecked-cluster".to_string(),
                "hc-node-cluster".to_string()
            )]
        );
        assert_eq!(resp.details, Some("health_check_ok"));
    }

    /// ADR-0202: the intercept is a HEADERS-ONLY reply — the codec frames it
    /// (H1 non-HEAD `content-length: 0`, H1 HEAD `transfer-encoding: chunked`,
    /// H2 no framing header), never from `body.len()`.
    #[test]
    fn intercept_is_a_headers_only_reply() {
        for method in ["GET", "HEAD"] {
            let resp = run(&mut filter(healthz()), method, "/healthz", &[]).expect("intercepted");
            assert!(resp.headers_only, "{method}");
        }
        assert!(!FilterResponse::static_reply(403, None, b"denied").headers_only);
    }

    #[test]
    fn path_matcher_sees_the_query_string() {
        assert!(run(&mut filter(healthz()), "GET", "/healthz?x=1", &[]).is_none());
    }

    #[test]
    fn exact_path_is_exact_and_case_sensitive() {
        let mut f = filter(healthz());
        assert!(run(&mut f, "GET", "/healthz/", &[]).is_none());
        assert!(run(&mut f, "GET", "/healthZ", &[]).is_none());
        assert!(run(&mut f, "GET", "/other", &[]).is_none());
    }

    /// The `:path` view is keyed case-insensitively, like every header name
    /// (phase-115 `REVIEW.md` F-3, the filter half).
    #[test]
    fn path_pseudo_header_name_is_case_insensitive() {
        let mut f = filter(vec![header_matcher_exact(":PATH", "/healthz")]);
        assert!(run(&mut f, "GET", "/healthz", &[]).is_some());
        assert!(run(&mut f, "GET", "/other", &[]).is_none());
    }

    #[test]
    fn filter_is_method_agnostic() {
        assert!(run(&mut filter(healthz()), "POST", "/healthz", &[]).is_some());
    }

    #[test]
    fn empty_matcher_list_matches_everything() {
        assert!(run(&mut filter(vec![]), "GET", "/anything", &[]).is_some());
    }

    #[test]
    fn matchers_fold_as_and() {
        let mut both = healthz();
        both.push(header_matcher_exact("x-probe", "yes"));
        let mut f = filter(both);
        assert!(run(&mut f, "GET", "/healthz", &[]).is_none());
        assert!(run(&mut f, "GET", "/healthz", &[("x-probe", "yes")]).is_some());
        assert!(run(&mut f, "GET", "/other", &[("x-probe", "yes")]).is_none());
    }

    #[test]
    fn header_value_is_not_an_echo_of_the_request() {
        let resp = run(
            &mut filter(healthz()),
            "GET",
            "/healthz",
            &[("x-envoy-upstream-healthchecked-cluster", "SENTINEL")],
        )
        .expect("intercepted");
        assert_eq!(resp.headers[0].1, "hc-node-cluster");
    }

    #[test]
    fn bad_regex_is_build_fatal() {
        let bad = HeaderMatcher {
            name: "x-a".to_string(),
            mode: envoy_config::HeaderMatcherMode::SafeRegexMatch(envoy_config::SafeRegex {
                regex: "(".to_string(),
                compiled: None,
            }),
            invert_match: false,
        };
        let registry = Arc::new(StatsRegistry::new());
        assert!(HealthCheckFilter::build_from_config(&cfg(vec![bad]), &registry, "p").is_err());
    }

    #[test]
    fn registers_eight_counters_and_ticks_two_per_intercept() {
        let registry = Arc::new(StatsRegistry::new());
        let mut f =
            HealthCheckFilter::build_from_config(&cfg(healthz()), &registry, "ingress_http")
                .expect("builds");
        let names: Vec<String> = registry.snapshot().into_iter().map(|(n, _)| n).collect();
        for name in STAT_NAMES {
            let full = format!("http.ingress_http.health_check.{name}");
            assert!(names.contains(&full), "{full} registered at build time");
        }
        let value = |name: &str| {
            registry
                .register_counter(&format!("http.ingress_http.health_check.{name}"))
                .expect("same-kind re-register returns the handle")
                .value()
        };
        run(&mut f, "GET", "/other", &[]);
        assert_eq!(value("request_total"), 0, "a fall-through is not counted");
        run(&mut f, "GET", "/healthz", &[]);
        run(&mut f, "POST", "/healthz", &[]);
        assert_eq!((value("request_total"), value("ok")), (2, 2));
        assert_eq!(value("failed"), 0);
    }

    #[test]
    fn encode_headers_is_noop() {
        let mut resp = FilterResponse::test_200();
        assert!(matches!(
            filter(healthz()).encode_headers(&mut resp),
            Decision::Continue
        ));
    }
}
