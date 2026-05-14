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
        }
    }

    pub(crate) fn encode_headers(&mut self, resp: &mut FilterResponse) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.encode_headers(resp),
            HttpFilterInstance::HeaderMutation(f) => f.encode_headers(resp),
        }
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
