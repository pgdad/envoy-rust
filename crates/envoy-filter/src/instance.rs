//! `HttpFilterInstance` — the per-instance variant enum.
//!
//! Three production variants are present: `Router(RouterTerminus)` (landed
//! at 07.1), `HeaderMutation(HeaderMutationFilter)` (landed at 07.2 per
//! parent-07 SPEC §3 D8.2-D15.2), and `LocalRateLimit(LocalRateLimitFilter)`
//! (landed at phase-09 Task 4 per SPEC §3 D4). The phase-09 task also
//! widened `HttpFilterInstance::build` to take `&Arc<StatsRegistry>` (so the
//! LocalRateLimit arm can register its 4 stat counters) and dropped the
//! prior `_position: usize` parameter (07.2 REVIEW M1 closure per SPEC §3 D5).

use std::sync::Arc;

use envoy_stats::StatsRegistry;

use crate::error::FilterError;
use crate::header_mutation::HeaderMutationFilter;
use crate::local_rate_limit::LocalRateLimitFilter;
use crate::pipeline::Decision;
use crate::router::RouterTerminus;
use crate::types::{FilterRequest, FilterResponse};

#[derive(Debug, Clone)]
pub enum HttpFilterInstance {
    Router(RouterTerminus),
    HeaderMutation(HeaderMutationFilter),
    LocalRateLimit(LocalRateLimitFilter),
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
    pub(crate) fn build(
        hf: &envoy_config::HttpFilter,
        registry: &Arc<StatsRegistry>,
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
        }
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.decode_headers(req),
            HttpFilterInstance::HeaderMutation(f) => f.decode_headers(req),
            HttpFilterInstance::LocalRateLimit(f) => f.decode_headers(req),
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
            #[cfg(feature = "test-util")]
            HttpFilterInstance::TestStopAndSendOnDecode(_) => Decision::Continue,
            #[cfg(feature = "test-util")]
            HttpFilterInstance::TestStopAndSendOnEncode(resp) => {
                Decision::StopAndSend(resp.clone())
            }
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

    #[test]
    fn build_router_succeeds() {
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        };
        let registry = test_registry();
        let instance = HttpFilterInstance::build(&hf, &registry).expect("Router build succeeds");
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
        let instance =
            HttpFilterInstance::build(&hf, &registry).expect("LocalRateLimit build succeeds");
        assert!(matches!(instance, HttpFilterInstance::LocalRateLimit(_)));
    }
}
