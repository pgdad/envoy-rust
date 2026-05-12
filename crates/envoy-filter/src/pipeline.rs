//! Filter chain iteration protocol.

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
    /// to `HttpFilterInstance::build` (Task 3). The parse-time validator at
    /// `envoy_config::validate_http_filters` performs the same cardinality
    /// checks earlier in the config-load path; this method's checks are
    /// defense-in-depth at the framework crate boundary.
    pub fn build_from_config(filters: &[envoy_config::HttpFilter]) -> Result<Self, FilterError> {
        if filters.is_empty() {
            return Err(FilterError::EmptyChain);
        }
        let mut out = Vec::with_capacity(filters.len());
        for (position, hf) in filters.iter().enumerate() {
            out.push(HttpFilterInstance::build(hf, position)?);
        }
        Ok(Self { filters: out })
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

    #[test]
    fn build_from_config_rejects_empty_list() {
        let filters: Vec<envoy_config::HttpFilter> = Vec::new();
        let err = FilterPipeline::build_from_config(&filters).unwrap_err();
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
        let pipeline =
            FilterPipeline::build_from_config(&filters).expect("single-Router build succeeds");
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
        let mut pipeline = FilterPipeline::build_from_config(&filters).unwrap();
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
        let mut pipeline = FilterPipeline::build_from_config(&filters).unwrap();
        let mut resp = test_response();
        let decision = pipeline.encode_headers(&mut resp);
        assert!(matches!(decision, Decision::Continue));
    }

    fn test_request() -> FilterRequest {
        FilterRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: vec![("host".to_string(), "localhost".to_string())],
            body: None,
        }
    }

    fn test_response() -> FilterResponse {
        FilterResponse {
            status: 200,
            reason: None,
            headers: vec![("content-length".to_string(), "0".to_string())],
            body: bytes::Bytes::new(),
        }
    }
}
