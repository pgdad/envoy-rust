//! Filter chain iteration protocol.

use std::sync::Arc;

use envoy_stats::StatsRegistry;

use crate::error::FilterError;
use crate::instance::HttpFilterInstance;
use crate::types::{FilterRequest, FilterResponse};

#[derive(Debug)]
pub enum Decision {
    Continue,
    StopAndSend(FilterResponse),
}

#[derive(Debug, Clone)]
pub struct FilterPipeline {
    filters: Vec<HttpFilterInstance>,
}

impl FilterPipeline {
    /// Build a `FilterPipeline` from a parsed envoy-config `HttpFilter` list.
    ///
    /// Returns an error if the list is empty. Per-instance build is delegated
    /// to `HttpFilterInstance::build`. The parse-time validator at
    /// `envoy_config::validate_http_filters` performs the same cardinality
    /// checks earlier in the config-load path; this method's checks are
    /// defense-in-depth at the framework crate boundary.
    ///
    /// `registry` is threaded through so stats-bearing filter arms (phase-09
    /// `LocalRateLimit`) can register their counters at build time. Phase 09
    /// Task 4 (D5 closure of 07.2 REVIEW M1) dropped the prior
    /// `.enumerate()` + per-instance `position: usize` plumbing.
    ///
    /// `hcm_stat_prefix` is threaded through for phase-10 RBAC stats namespace
    /// registration under `http.{hcm_stat_prefix}.rbac.{allowed,denied}`. The
    /// H1 HCM `Http1HCMConfig::from_config` passes `&cfg.stat_prefix` at the
    /// single production call site (phase-10 PLAN lock-in #5).
    pub fn build_from_config(
        filters: &[envoy_config::HttpFilter],
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        if filters.is_empty() {
            return Err(FilterError::EmptyChain);
        }
        let mut out = Vec::with_capacity(filters.len());
        for hf in filters.iter() {
            out.push(HttpFilterInstance::build(hf, registry, hcm_stat_prefix)?);
        }
        Ok(Self { filters: out })
    }

    /// Test-only: build a `FilterPipeline` directly from a list of
    /// `HttpFilterInstance`s, bypassing config parsing. Used by the H1/H2 HCM
    /// integration tests to inject the `test-util` StopAndSend stubs.
    #[cfg(feature = "test-util")]
    pub fn test_from_instances(filters: Vec<HttpFilterInstance>) -> Self {
        Self { filters }
    }

    /// True when the chain is exactly one Router terminus. The H1 HCM uses
    /// this to gate its zero-copy proxied-response fast path: Router is a
    /// no-op on both decode and encode, so skipping the owned response-header
    /// materialization is unobservable to the filter chain.
    pub fn is_router_only(&self) -> bool {
        self.filters.len() == 1
            && matches!(
                self.filters[0],
                crate::instance::HttpFilterInstance::Router(_)
            )
    }

    /// Phase-23 D2: fan the matched route's per-filter config out to each filter
    /// instance before the decode pass. Inert for all non-CORS filters → the
    /// 07.1 foundation-slice property (all pre-existing fixtures unchanged).
    pub fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
        for filter in self.filters.iter_mut() {
            filter.apply_route_config(route);
        }
    }

    /// Iterate the filter chain in **declaration order** on the decode side.
    ///
    /// Per parent-07 SPEC §6 Rule 6: decode walks `filters.iter_mut()`.
    /// First `StopAndSend` short-circuits remaining iteration.
    pub fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        for filter in self.filters.iter_mut() {
            match filter.decode_headers(req) {
                Decision::Continue => continue,
                Decision::StopAndSend(resp) => return Decision::StopAndSend(resp),
            }
        }
        Decision::Continue
    }

    /// Iterate the filter chain in **reverse declaration order** on the
    /// encode side.
    ///
    /// Per parent-07 SPEC §6 Rule 6: encode walks `filters.iter_mut().rev()`.
    /// This matches Envoy v1.33's documented filter-chain semantics where
    /// the Router filter produces the response (so it fires first on encode)
    /// and other filters mutate it on the way out.
    pub fn encode_headers(&mut self, resp: &mut FilterResponse) -> Decision {
        for filter in self.filters.iter_mut().rev() {
            match filter.encode_headers(resp) {
                Decision::Continue => continue,
                Decision::StopAndSend(replacement) => return Decision::StopAndSend(replacement),
            }
        }
        Decision::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_stats::StatsRegistry;
    use std::sync::Arc;

    fn test_registry() -> Arc<StatsRegistry> {
        Arc::new(StatsRegistry::new())
    }

    #[test]
    fn build_from_config_rejects_empty_list() {
        let filters: Vec<envoy_config::HttpFilter> = Vec::new();
        let err = FilterPipeline::build_from_config(&filters, &test_registry(), "test_prefix")
            .unwrap_err();
        assert!(matches!(err, FilterError::EmptyChain));
    }

    #[test]
    fn build_from_config_with_single_router_succeeds() {
        let filters = vec![envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        }];
        let pipeline = FilterPipeline::build_from_config(&filters, &test_registry(), "test_prefix")
            .expect("single-Router build succeeds");
        assert_eq!(pipeline.filters.len(), 1);
    }

    #[test]
    fn decode_headers_on_single_router_returns_continue() {
        let filters = vec![envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        }];
        let mut pipeline =
            FilterPipeline::build_from_config(&filters, &test_registry(), "test_prefix").unwrap();
        let mut req = test_request();
        let decision = pipeline.decode_headers(&mut req);
        assert!(matches!(decision, Decision::Continue));
    }

    #[test]
    fn encode_headers_on_single_router_returns_continue() {
        let filters = vec![envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        }];
        let mut pipeline =
            FilterPipeline::build_from_config(&filters, &test_registry(), "test_prefix").unwrap();
        let mut resp = test_response();
        let decision = pipeline.encode_headers(&mut resp);
        assert!(matches!(decision, Decision::Continue));
    }

    #[test]
    fn build_from_config_wires_fault_then_router() {
        use envoy_config::{
            DenominatorType, FaultAbort, FaultConfig, FractionalPercent, HttpFilter,
            HttpFilterTypedConfig,
        };
        let registry = Arc::new(StatsRegistry::new());
        let filters = vec![
            HttpFilter {
                name: "envoy.filters.http.fault".to_string(),
                typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                    abort: FaultAbort {
                        http_status: 503,
                        percentage: FractionalPercent {
                            numerator: 100,
                            denominator: DenominatorType::Hundred,
                        },
                    },
                    headers: vec![],
                }),
            },
            HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(envoy_config::RouterConfig {}),
            },
        ];
        let mut pipeline = FilterPipeline::build_from_config(&filters, &registry, "ingress_http")
            .expect("builds fault + router pipeline");
        // 100% abort with no gate → every request is short-circuited with 503.
        let mut req = FilterRequest::test("GET", "/", &[]);
        match pipeline.decode_headers(&mut req) {
            Decision::StopAndSend(resp) => assert_eq!(resp.status, 503),
            Decision::Continue => panic!("expected fault abort, got Continue"),
        }
    }

    fn test_request() -> FilterRequest {
        FilterRequest::test("GET", "/", &[("host", "localhost")])
    }

    fn test_response() -> FilterResponse {
        FilterResponse {
            status: 200,
            reason: None,
            headers: vec![("content-length".to_string(), "0".to_string())],
            body: bytes::Bytes::new(),
        }
    }

    // ---- Task 4: apply_route_config fan-out tests ----

    fn cors_router_pipeline() -> FilterPipeline {
        let filters = vec![
            envoy_config::HttpFilter {
                name: "envoy.filters.http.cors".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Cors(
                    envoy_config::CorsConfig::default(),
                ),
            },
            envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            },
        ];
        FilterPipeline::build_from_config(&filters, &test_registry(), "ingress_http")
            .expect("cors + router pipeline builds")
    }

    fn route_with_cors_policy() -> envoy_config::Route {
        serde_yaml::from_str(
            r#"
match:
  prefix: "/"
route:
  cluster: backend
typed_per_filter_config:
  envoy.filters.http.cors:
    "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy
    allow_origin_string_match:
      - exact: "http://a.test"
    allow_methods: "GET"
"#,
        )
        .expect("route with cors policy parses")
    }

    fn preflight_request() -> FilterRequest {
        FilterRequest::test(
            "OPTIONS",
            "/",
            &[
                ("origin", "http://a.test"),
                ("access-control-request-method", "GET"),
            ],
        )
    }

    #[test]
    fn apply_route_config_then_preflight_short_circuits() {
        let mut pipeline = cors_router_pipeline();
        let route = route_with_cors_policy();
        pipeline.apply_route_config(Some(&route));
        let mut req = preflight_request();
        match pipeline.decode_headers(&mut req) {
            Decision::StopAndSend(r) => assert_eq!(r.status, 200),
            Decision::Continue => panic!("expected StopAndSend(200) for allowed preflight"),
        }
    }

    #[test]
    fn apply_route_config_none_leaves_cors_inert() {
        let mut pipeline = cors_router_pipeline();
        pipeline.apply_route_config(None);
        let mut req = preflight_request();
        // No policy → cors is inert → Router Continue → pipeline Continue
        assert!(
            matches!(pipeline.decode_headers(&mut req), Decision::Continue),
            "inert CORS (no policy) + Router must return Continue for preflight"
        );
    }
}
