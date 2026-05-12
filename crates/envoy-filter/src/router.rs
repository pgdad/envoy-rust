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
use envoy_http1::{Request, Response};

#[derive(Debug, Clone, Default)]
pub struct RouterTerminus {
    _private: (),
}

impl RouterTerminus {
    pub(crate) fn new() -> Self {
        Self { _private: () }
    }

    pub(crate) fn decode_headers(&mut self, _req: &mut Request) -> Decision {
        Decision::Continue
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut Response) -> Decision {
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
        let mut req = Request {
            method: "GET".to_string(),
            path: "/".to_string(),
            version: envoy_http1::codec::HttpVersion::Http11,
            headers: vec![("host".to_string(), "example.com".to_string())],
            bytes_consumed: 0,
            body: Some(Bytes::from_static(b"hello")),
        };
        let before = (
            req.method.clone(),
            req.path.clone(),
            req.version,
            req.headers.clone(),
            req.bytes_consumed,
            req.body.clone(),
        );
        let decision = router.decode_headers(&mut req);
        assert!(matches!(decision, Decision::Continue));
        assert_eq!(req.method, before.0);
        assert_eq!(req.path, before.1);
        assert_eq!(req.version, before.2);
        assert_eq!(req.headers, before.3);
        assert_eq!(req.bytes_consumed, before.4);
        assert_eq!(req.body, before.5);
    }

    #[test]
    fn encode_headers_returns_continue_and_does_not_mutate_response() {
        let mut router = RouterTerminus::new();
        let mut resp = Response {
            status: 200,
            reason: None,
            headers: vec![("content-length".to_string(), "5".to_string())],
            body: Bytes::from_static(b"hello"),
        };
        let before = (
            resp.status,
            resp.reason,
            resp.headers.clone(),
            resp.body.clone(),
        );
        let decision = router.encode_headers(&mut resp);
        assert!(matches!(decision, Decision::Continue));
        assert_eq!(resp.status, before.0);
        assert_eq!(resp.reason, before.1);
        assert_eq!(resp.headers, before.2);
        assert_eq!(resp.body, before.3);
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
