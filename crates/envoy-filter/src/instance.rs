//! `HttpFilterInstance` — the per-instance variant enum.
//!
//! Two variants are present: `Router(RouterTerminus)` (landed at 07.1) and
//! `HeaderMutation(HeaderMutationFilter)` (landed at 07.2 per parent-07
//! SPEC §3 D8.2-D15.2).

use crate::error::FilterError;
use crate::header_mutation::HeaderMutationFilter;
use crate::pipeline::Decision;
use crate::router::RouterTerminus;
use crate::types::{FilterRequest, FilterResponse};

#[derive(Debug, Clone)]
pub enum HttpFilterInstance {
    Router(RouterTerminus),
    HeaderMutation(HeaderMutationFilter),
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
    /// The validator at `envoy_config::validate_http_filters` (Task 4)
    /// performs the name/typed_config consistency checks at config-load
    /// time. This constructor relies on the validator's invariants but
    /// does not duplicate the checks (defense-in-depth lives at
    /// `FilterPipeline::build_from_config`, not here).
    pub(crate) fn build(
        hf: &envoy_config::HttpFilter,
        _position: usize,
    ) -> Result<Self, FilterError> {
        match &hf.typed_config {
            envoy_config::HttpFilterTypedConfig::Router(_cfg) => {
                Ok(HttpFilterInstance::Router(RouterTerminus::new()))
            }
            envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg) => Ok(
                HttpFilterInstance::HeaderMutation(HeaderMutationFilter::build_from_config(cfg)?),
            ),
        }
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.decode_headers(req),
            HttpFilterInstance::HeaderMutation(f) => f.decode_headers(req),
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

    #[test]
    fn build_router_succeeds() {
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        };
        let instance = HttpFilterInstance::build(&hf, 0).expect("Router build succeeds");
        assert!(matches!(instance, HttpFilterInstance::Router(_)));
    }
}
