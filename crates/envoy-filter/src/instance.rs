//! `HttpFilterInstance` — the per-instance variant enum.
//!
//! Four production variants are present: `Router(RouterTerminus)` (landed
//! at 07.1), `HeaderMutation(HeaderMutationFilter)` (landed at 07.2 per
//! parent-07 SPEC §3 D8.2-D15.2), `LocalRateLimit(LocalRateLimitFilter)`
//! (landed at phase-09 Task 4 per SPEC §3 D4), and `Rbac(RbacFilter)`
//! (landed at phase-10 Task 4 per SPEC §3 D4). The phase-09 task also
//! widened `HttpFilterInstance::build` to take `&Arc<StatsRegistry>` (so the
//! LocalRateLimit arm can register its 4 stat counters) and dropped the
//! prior `_position: usize` parameter (07.2 REVIEW M1 closure per SPEC §3 D5).
//! Phase-10 Task 4 further widened the `build` signature to take
//! `hcm_stat_prefix: &str` so the Rbac arm can register its 2 stat counters
//! under the HCM-embedded `http.{hcm_stat_prefix}.rbac.{allowed,denied}`
//! namespace.

use std::sync::Arc;

use envoy_stats::StatsRegistry;

use crate::cors::CorsFilter;
use crate::error::FilterError;
use crate::fault::FaultFilter;
use crate::header_mutation::HeaderMutationFilter;
use crate::jwt_authn::JwtAuthnFilter;
use crate::local_rate_limit::LocalRateLimitFilter;
use crate::pipeline::Decision;
use crate::rbac::RbacFilter;
use crate::router::RouterTerminus;
use crate::types::{FilterRequest, FilterResponse};

#[derive(Debug, Clone)]
pub enum HttpFilterInstance {
    Router(RouterTerminus),
    HeaderMutation(HeaderMutationFilter),
    LocalRateLimit(LocalRateLimitFilter),
    /// Phase-10 Task 4: the `envoy.filters.http.rbac` filter (decode-only;
    /// short-circuits non-matching requests with a 403 via SPEC §5.6's
    /// decision matrix; 2 stat counters registered under
    /// `http.{hcm_stat_prefix}.rbac.{allowed,denied}` at build time).
    Rbac(RbacFilter),
    /// Phase-11 Task 3: the `envoy.filters.http.fault` filter (decode-side
    /// abort path; short-circuits matching requests with the configured HTTP
    /// status; 1 stat counter registered under
    /// `http.{hcm_stat_prefix}.fault.aborts_injected` at build time).
    Fault(FaultFilter),
    /// Phase-22 Task 7: the `envoy.filters.http.jwt_authn` filter (decode-side
    /// authentication gate; selects the first matching rule, verifies the
    /// `Authorization: Bearer` RS256 JWT against the provider JWKS, and
    /// short-circuits failures with a 401/403 local reply; 2 stat counters
    /// registered under `http.{hcm_stat_prefix}.jwt_authn.{allowed,denied}` at
    /// build time).
    JwtAuthn(JwtAuthnFilter),
    /// Phase-23 Task 4: the `envoy.filters.http.cors` filter (decode-side
    /// preflight short-circuit + encode-side actual-request decoration; the
    /// per-route `CorsPolicy` is supplied via `apply_route_config`; 2 stat
    /// counters registered under
    /// `http.{hcm_stat_prefix}.cors.{origin_valid,origin_invalid}` at build
    /// time).
    Cors(CorsFilter),
    /// Test-only: a filter that always returns `Decision::StopAndSend` on the
    /// DECODE side, carrying the given `FilterResponse`. Used by the H1/H2 HCM
    /// integration tests to exercise the decode-side short-circuit.
    #[cfg(feature = "test-util")]
    TestStopAndSendOnDecode(FilterResponse),
    /// Test-only: a filter that always returns `Decision::StopAndSend` on the
    /// ENCODE side.
    #[cfg(feature = "test-util")]
    TestStopAndSendOnEncode(FilterResponse),
}

impl HttpFilterInstance {
    /// Construct a per-instance filter from a parsed envoy-config
    /// `HttpFilter` entry.
    ///
    /// The validator at `envoy_config::validate_http_filters` (phase 09 Task
    /// 1 for LocalRateLimit; earlier sub-phases for Router + HeaderMutation)
    /// performs the name/typed_config consistency checks at config-load
    /// time. This constructor relies on the validator's invariants but
    /// does not duplicate the checks (defense-in-depth lives at
    /// `FilterPipeline::build_from_config`, not here).
    ///
    /// `registry` is threaded through so the `LocalRateLimit` arm can
    /// register its 4 stat counters at build time (phase 09 D6); the
    /// `Router` + `HeaderMutation` arms ignore it. Phase 09 Task 4
    /// (D5 closure of 07.2 REVIEW M1) dropped the prior `_position: usize`
    /// parameter — diagnostic position metadata is no longer threaded
    /// because no in-flight call site consumes it.
    ///
    /// `hcm_stat_prefix` is threaded through so the `Rbac` arm can register
    /// its 2 stat counters at build time under
    /// `http.{hcm_stat_prefix}.rbac.{allowed,denied}` (phase 10 D6); the
    /// `Router` / `HeaderMutation` / `LocalRateLimit` arms ignore it. The H1
    /// HCM `Http1HCMConfig::from_config` threads `&cfg.stat_prefix` at the
    /// single production call site per phase-10 PLAN lock-in #5.
    pub(crate) fn build(
        hf: &envoy_config::HttpFilter,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        match &hf.typed_config {
            envoy_config::HttpFilterTypedConfig::Router(_cfg) => {
                Ok(HttpFilterInstance::Router(RouterTerminus::new()))
            }
            envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg) => Ok(
                HttpFilterInstance::HeaderMutation(HeaderMutationFilter::build_from_config(cfg)?),
            ),
            envoy_config::HttpFilterTypedConfig::LocalRateLimit(cfg) => {
                Ok(HttpFilterInstance::LocalRateLimit(
                    LocalRateLimitFilter::build_from_config(cfg, registry)?,
                ))
            }
            envoy_config::HttpFilterTypedConfig::Rbac(cfg) => Ok(HttpFilterInstance::Rbac(
                RbacFilter::build_from_config(cfg, registry, hcm_stat_prefix)?,
            )),
            envoy_config::HttpFilterTypedConfig::Fault(cfg) => Ok(HttpFilterInstance::Fault(
                FaultFilter::build_from_config(cfg, registry, hcm_stat_prefix)?,
            )),
            envoy_config::HttpFilterTypedConfig::JwtAuthn(cfg) => Ok(HttpFilterInstance::JwtAuthn(
                JwtAuthnFilter::build_from_config(cfg, registry, hcm_stat_prefix)?,
            )),
            envoy_config::HttpFilterTypedConfig::Cors(cfg) => Ok(HttpFilterInstance::Cors(
                CorsFilter::build_from_config(cfg, registry, hcm_stat_prefix)?,
            )),
        }
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.decode_headers(req),
            HttpFilterInstance::HeaderMutation(f) => f.decode_headers(req),
            HttpFilterInstance::LocalRateLimit(f) => f.decode_headers(req),
            HttpFilterInstance::Rbac(f) => f.decode_headers(req),
            HttpFilterInstance::Fault(f) => f.decode_headers(req),
            HttpFilterInstance::JwtAuthn(f) => f.decode_headers(req),
            HttpFilterInstance::Cors(f) => f.decode_headers(req),
            #[cfg(feature = "test-util")]
            HttpFilterInstance::TestStopAndSendOnDecode(resp) => {
                Decision::StopAndSend(resp.clone())
            }
            #[cfg(feature = "test-util")]
            HttpFilterInstance::TestStopAndSendOnEncode(_) => Decision::Continue,
        }
    }

    pub(crate) fn encode_headers(&mut self, resp_arg: &mut FilterResponse) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.encode_headers(resp_arg),
            HttpFilterInstance::HeaderMutation(f) => f.encode_headers(resp_arg),
            HttpFilterInstance::LocalRateLimit(f) => f.encode_headers(resp_arg),
            HttpFilterInstance::Rbac(f) => f.encode_headers(resp_arg),
            HttpFilterInstance::Fault(f) => f.encode_headers(resp_arg),
            HttpFilterInstance::JwtAuthn(f) => f.encode_headers(resp_arg),
            HttpFilterInstance::Cors(f) => f.encode_headers(resp_arg),
            #[cfg(feature = "test-util")]
            HttpFilterInstance::TestStopAndSendOnDecode(_) => Decision::Continue,
            #[cfg(feature = "test-util")]
            HttpFilterInstance::TestStopAndSendOnEncode(resp) => {
                Decision::StopAndSend(resp.clone())
            }
        }
    }

    /// Phase-23 D2: thread the matched route's per-filter config into the
    /// per-request filter instance. No-op for every filter that does not consume
    /// per-route config (Router/HeaderMutation/LocalRateLimit/Rbac/Fault/JwtAuthn);
    /// only `Cors` reads it.
    pub(crate) fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
        if let HttpFilterInstance::Cors(f) = self {
            f.apply_route_config(route);
        }
    }
}

#[cfg(feature = "test-util")]
impl HttpFilterInstance {
    /// Construct a test-only filter that emits `StopAndSend(resp)` on decode.
    pub fn test_stop_and_send_on_decode(resp: FilterResponse) -> Self {
        HttpFilterInstance::TestStopAndSendOnDecode(resp)
    }
    /// Construct a test-only filter that emits `StopAndSend(resp)` on encode.
    pub fn test_stop_and_send_on_encode(resp: FilterResponse) -> Self {
        HttpFilterInstance::TestStopAndSendOnEncode(resp)
    }
    /// Construct a test-only Router terminus instance. Used by tests that
    /// build a `FilterPipeline` via `test_from_instances` and need a
    /// Router at the terminus position.
    pub fn test_router() -> Self {
        HttpFilterInstance::Router(RouterTerminus::new())
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

    fn jwt_authn_cfg_for_test() -> envoy_config::JwtAuthnConfig {
        let mut providers = std::collections::BTreeMap::new();
        providers.insert(
            "provider1".to_string(),
            envoy_config::JwtProvider {
                issuer: "testing@secure.istio.io".to_string(),
                audiences: vec![],
                local_jwks: envoy_config::DataSource {
                    filename: None,
                    inline_string: Some(
                        r#"{"keys":[{"kty":"RSA","kid":"k1","n":"sXche4iX","e":"AQAB"}]}"#
                            .to_string(),
                    ),
                },
                forward: false,
            },
        );
        envoy_config::JwtAuthnConfig {
            providers,
            rules: vec![envoy_config::RequirementRule {
                r#match: envoy_config::RouteMatch {
                    prefix: Some("/".to_string()),
                    path: None,
                    headers: vec![],
                },
                requires: envoy_config::JwtRequirement {
                    provider_name: "provider1".to_string(),
                },
            }],
        }
    }

    #[test]
    fn build_router_succeeds() {
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        };
        let registry = test_registry();
        let instance = HttpFilterInstance::build(&hf, &registry, "test_prefix")
            .expect("Router build succeeds");
        assert!(matches!(instance, HttpFilterInstance::Router(_)));
    }

    #[test]
    fn build_local_rate_limit_succeeds() {
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.local_ratelimit".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::LocalRateLimit(
                envoy_config::LocalRateLimitConfig {
                    stat_prefix: "phase_09".to_string(),
                    token_bucket: envoy_config::TokenBucket {
                        max_tokens: 3,
                        tokens_per_fill: 0,
                        fill_interval: serde_yaml::Value::String("60s".to_string()),
                    },
                    response_headers_to_add: Vec::new(),
                    status: envoy_config::HttpStatus { code: 429 },
                },
            ),
        };
        let registry = test_registry();
        let instance = HttpFilterInstance::build(&hf, &registry, "test_prefix")
            .expect("LocalRateLimit build succeeds");
        assert!(matches!(instance, HttpFilterInstance::LocalRateLimit(_)));
    }

    #[test]
    fn build_rbac_succeeds() {
        let mut policies = std::collections::BTreeMap::new();
        policies.insert(
            "p".to_string(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Any(true)],
                principals: vec![envoy_config::Principal::Any(true)],
            },
        );
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.rbac".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Rbac(envoy_config::RbacConfig {
                rules: envoy_config::Rules {
                    action: envoy_config::Action::Allow,
                    policies,
                },
            }),
        };
        let registry = test_registry();
        let instance =
            HttpFilterInstance::build(&hf, &registry, "test_prefix").expect("Rbac build succeeds");
        assert!(matches!(instance, HttpFilterInstance::Rbac(_)));
    }

    #[test]
    fn builds_jwt_authn_instance_and_dispatches() {
        let registry = test_registry();
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.jwt_authn".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::JwtAuthn(jwt_authn_cfg_for_test()),
        };
        let mut inst = HttpFilterInstance::build(&hf, &registry, "ingress_http")
            .expect("JwtAuthn build succeeds");
        assert!(matches!(inst, HttpFilterInstance::JwtAuthn(_)));

        // missing Authorization header → filter must short-circuit with 401
        let mut req = FilterRequest {
            method: "GET".into(),
            path: "/".into(),
            headers: vec![("host".into(), "envoy.test".into())],
            body: None,
        };
        match inst.decode_headers(&mut req) {
            Decision::StopAndSend(r) => assert_eq!(r.status, 401),
            Decision::Continue => {
                panic!("expected StopAndSend(401) for missing token, got Continue")
            }
        }

        // encode_headers is a no-op for JwtAuthn — must return Continue
        let mut resp = FilterResponse {
            status: 200,
            reason: None,
            headers: vec![],
            body: bytes::Bytes::new(),
        };
        assert!(matches!(inst.encode_headers(&mut resp), Decision::Continue));
    }
}
