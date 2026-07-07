//! Router filter — the terminus of every filter chain.
//!
//! `Router` is the filter that dispatches to the route's action
//! (`direct_response` or upstream proxy). At the filter-chain level it
//! is a no-op on both iteration sides — the actual dispatch happens
//! inside the HCM's writer-arm match after `pipeline.decode_headers`
//! returns and route-match runs.
//!
//! The validator (Task 4) guarantees Router is the last entry. On
//! decode this means `Router::decode_headers` runs LAST among all
//! filters; on encode (reverse order) this means `Router::encode_headers`
//! runs FIRST, which models Envoy's semantic of "Router produces the
//! response and other filters mutate it on the encode side".

use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

#[derive(Debug, Clone, Default)]
pub struct RouterTerminus {
    _private: (),
}

impl RouterTerminus {
    pub(crate) fn new() -> Self {
        Self { _private: () }
    }

    pub(crate) fn decode_headers(&mut self, _req: &mut FilterRequest) -> Decision {
        Decision::Continue
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn decode_headers_returns_continue_and_does_not_mutate_request() {
        let mut router = RouterTerminus::new();
        let mut req = FilterRequest {
            body: Some(Bytes::from_static(b"hello")),
            ..FilterRequest::test("GET", "/", &[("host", "example.com")])
        };
        let before = req.clone();
        let decision = router.decode_headers(&mut req);
        assert!(matches!(decision, Decision::Continue));
        assert_eq!(req, before);
    }

    #[test]
    fn encode_headers_returns_continue_and_does_not_mutate_response() {
        let mut router = RouterTerminus::new();
        let mut resp = FilterResponse {
            status: 200,
            reason: None,
            headers: vec![("content-length".to_string(), "5".to_string())],
            body: Bytes::from_static(b"hello"),
        };
        let before = resp.clone();
        let decision = router.encode_headers(&mut resp);
        assert!(matches!(decision, Decision::Continue));
        assert_eq!(resp, before);
    }

    #[test]
    fn router_terminus_is_clone_and_default() {
        let r1 = RouterTerminus::default();
        let r2 = RouterTerminus::new();
        assert_eq!(format!("{r1:?}"), format!("{r2:?}"));
        let r3 = r1.clone();
        assert_eq!(format!("{r1:?}"), format!("{r3:?}"));
    }
}
