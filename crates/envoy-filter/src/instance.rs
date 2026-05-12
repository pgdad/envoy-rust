//! Placeholder for `HttpFilterInstance` enum.
//!
//! Task 2 ships this stub so `pipeline.rs` compiles. Task 3 replaces
//! the stub with the real Router-only enum + `RouterTerminus` filter
//! type. The `build` constructor accepts any `Router`-typed config and
//! (at Task 2 scope) rejects nothing because `Router` is the only variant.

use crate::error::FilterError;
use crate::pipeline::Decision;
use envoy_http1::{Request, Response};

#[derive(Debug, Clone)]
pub enum HttpFilterInstance {
    /// Task-2 placeholder. Holds nothing. Replaced at Task 3 with the
    /// real `Router(RouterTerminus)` variant.
    Router,
}

impl HttpFilterInstance {
    pub(crate) fn build(
        hf: &envoy_config::HttpFilter,
        _position: usize,
    ) -> Result<Self, FilterError> {
        match &hf.typed_config {
            envoy_config::HttpFilterTypedConfig::Router(_cfg) => Ok(HttpFilterInstance::Router),
        }
    }

    pub(crate) fn decode_headers(&mut self, _req: &mut Request) -> Decision {
        match self {
            HttpFilterInstance::Router => Decision::Continue,
        }
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut Response) -> Decision {
        match self {
            HttpFilterInstance::Router => Decision::Continue,
        }
    }
}
