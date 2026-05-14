//! `HttpFilterInstance` — the per-instance variant enum.
//!
//! At 07.1 the only variant is `Router` (holding `RouterTerminus`).
//! Phase 07.2 adds `HeaderMutation(HeaderMutationFilter)` per parent-07
//! SPEC §3 D8.2-D15.2.

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::router::RouterTerminus;
use crate::types::{FilterRequest, FilterResponse};

#[derive(Debug, Clone)]
pub enum HttpFilterInstance {
    Router(RouterTerminus),
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
        position: usize,
    ) -> Result<Self, FilterError> {
        match &hf.typed_config {
            envoy_config::HttpFilterTypedConfig::Router(_cfg) => {
                Ok(HttpFilterInstance::Router(RouterTerminus::new()))
            }
            // Task 3 replaces this stub with HeaderMutationFilter::build_from_config.
            envoy_config::HttpFilterTypedConfig::HeaderMutation(_cfg) => {
                Err(FilterError::UnsupportedFilterType {
                    position,
                    name: hf.name.clone(),
                })
            }
        }
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.decode_headers(req),
        }
    }

    pub(crate) fn encode_headers(&mut self, resp: &mut FilterResponse) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.encode_headers(resp),
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
