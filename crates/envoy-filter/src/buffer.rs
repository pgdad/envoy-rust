//! `envoy.filters.http.buffer` — decode-side request-body length guard.
//!
//! §6.2-verified against envoyproxy/envoy:v1.33.0 (phase-25 PLAN-write; ADR-0063).
//!
//! ## Behaviour summary
//! - The full request body is available as `FilterRequest.body` at
//!   `decode_headers` time (H1 via phase 25.1; H2 via the codec). The filter
//!   rejects iff `body.len() > effective_max_request_bytes` (strict `>`,
//!   ADR-0063 finding 6) with a 413 `Payload Too Large` local reply (17 bytes,
//!   no trailing newline; `content-type: text/plain` stamped by the H1/H2 synth
//!   decorators — the rbac/csrf precedent). Else `Continue` (the body flows
//!   upstream via phase 25.1 on H1 / the codec on H2).
//! - The chain-level `Buffer.max_request_bytes` is the BASE limit; a per-route
//!   `BufferPerRoute` (threaded via `apply_route_config`) either DISABLES the
//!   filter for the route or OVERRIDES the limit (ADR-0063 finding 3). A route
//!   with no buffer override keeps the chain base.
//! - Decode-side only; `encode_headers` is the trivial `Continue` arm. NO stats
//!   (ADR-0063 finding 4 — Envoy emits no buffer-scoped counters).
use bytes::Bytes;

use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

const BUFFER_FILTER_NAME: &str = "envoy.filters.http.buffer";
/// ADR-0063 finding 1: the over-limit local-reply body, 17 bytes, NO newline.
const OVER_LIMIT_BODY: &[u8] = b"Payload Too Large";

/// The per-request effective policy resolved from the route (ADR-0063 finding 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effective {
    /// `BufferPerRoute { disabled: true }` — the filter is bypassed for the route.
    Disabled,
    /// The effective byte limit (chain base, or a `BufferPerRoute` override).
    Limit(u32),
}

/// The `envoy.filters.http.buffer` runtime filter. Built once per filter-chain
/// from the chain-level `Buffer` (the base limit); `apply_route_config` selects
/// the per-request effective policy each request.
#[derive(Debug, Clone)]
pub struct BufferFilter {
    /// Chain-level base limit (`Buffer.max_request_bytes`).
    base_max: u32,
    /// Effective policy for the current request (route override if present, else
    /// the chain base).
    effective: Effective,
}

impl BufferFilter {
    /// Build from the chain-level `Buffer` config. Infallible — no stats to
    /// register (ADR-0063), no validation beyond the serde-enforced required u32.
    pub(crate) fn new(cfg: &envoy_config::Buffer) -> Self {
        Self {
            base_max: cfg.max_request_bytes,
            effective: Effective::Limit(cfg.max_request_bytes),
        }
    }

    /// Select the per-request effective policy: the route's `BufferPerRoute`
    /// override if present (disable / lowered limit), else the chain base.
    pub(crate) fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
        self.effective = match route.and_then(|r| r.typed_per_filter_config.get(BUFFER_FILTER_NAME))
        {
            Some(envoy_config::PerFilterConfig::Buffer(bpr)) => {
                if bpr.disabled {
                    Effective::Disabled
                } else if let Some(b) = &bpr.buffer {
                    Effective::Limit(b.max_request_bytes)
                } else {
                    // Empty `{}` per-route → fall back to the chain base.
                    Effective::Limit(self.base_max)
                }
            }
            // No buffer per-route override (absent, or a different filter's
            // config) → the chain base still guards the route.
            _ => Effective::Limit(self.base_max),
        };
    }

    /// Decode-side entry point: reject an over-limit body with a 413.
    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        let limit = match self.effective {
            Effective::Disabled => return Decision::Continue,
            Effective::Limit(n) => n,
        };
        let body_len = req.body.as_ref().map_or(0, |b| b.len());
        // strict `>` (ADR-0063 finding 6); compare in u64 to avoid usize/u32
        // truncation on a > 4 GiB body.
        if body_len as u64 > u64::from(limit) {
            Decision::StopAndSend(over_limit_response())
        } else {
            Decision::Continue
        }
    }

    /// Buffer is decode-side only; encode is the trivial `Continue` arm (the
    /// exhaustive-match arm for the `HttpFilterInstance` wiring).
    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

/// The 413 over-limit local reply (ADR-0063 finding 1). `content-type`,
/// `content-length`, `server`, `date`(, `connection`) are stamped by the H1/H2
/// synth decorators downstream of the pipeline (the rbac/csrf precedent).
fn over_limit_response() -> FilterResponse {
    FilterResponse {
        status: 413,
        reason: Some("Payload Too Large"),
        headers: Vec::new(),
        body: Bytes::from_static(OVER_LIMIT_BODY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn req_with_body(method: &str, path: &str, body: &[u8]) -> FilterRequest {
        FilterRequest {
            method: method.into(),
            path: path.into(),
            headers: vec![],
            body: if body.is_empty() {
                None
            } else {
                Some(Bytes::copy_from_slice(body))
            },
        }
    }

    fn route_with_buffer(pr: envoy_config::BufferPerRoute) -> envoy_config::Route {
        let mut pfc = BTreeMap::new();
        pfc.insert(
            BUFFER_FILTER_NAME.to_string(),
            envoy_config::PerFilterConfig::Buffer(pr),
        );
        envoy_config::Route {
            r#match: envoy_config::RouteMatch {
                prefix: Some("/".to_string()),
                path: None,
                headers: vec![],
            },
            action: envoy_config::RouteAction::DirectResponse(envoy_config::DirectResponse {
                status: 200,
                body: envoy_config::DataSource {
                    filename: None,
                    inline_string: None,
                },
            }),
            typed_per_filter_config: pfc,
        }
    }

    fn filter(max: u32) -> BufferFilter {
        BufferFilter::new(&envoy_config::Buffer {
            max_request_bytes: max,
        })
    }

    #[test]
    fn within_limit_continues() {
        let mut f = filter(10);
        f.apply_route_config(None);
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"hello")),
            Decision::Continue
        ));
    }

    #[test]
    fn at_limit_continues_strict_gt() {
        // ADR-0063 finding 6: reject is strictly `>`; exactly-limit → Continue.
        let mut f = filter(5);
        f.apply_route_config(None);
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"hello")), // 5 == 5
            Decision::Continue
        ));
    }

    #[test]
    fn over_limit_rejects_413_payload_too_large() {
        let mut f = filter(10);
        f.apply_route_config(None);
        match f.decode_headers(&mut req_with_body("POST", "/", b"hello world!!")) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 413);
                assert_eq!(resp.reason, Some("Payload Too Large"));
                assert_eq!(&resp.body[..], b"Payload Too Large");
                assert_eq!(resp.body.len(), 17);
            }
            _ => panic!("expected 413"),
        }
    }

    #[test]
    fn per_route_disabled_bypasses() {
        let mut f = filter(10);
        let route = route_with_buffer(envoy_config::BufferPerRoute {
            disabled: true,
            buffer: None,
        });
        f.apply_route_config(Some(&route));
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"way over the limit")),
            Decision::Continue
        ));
    }

    #[test]
    fn per_route_lowered_limit_rejects() {
        let mut f = filter(100);
        let route = route_with_buffer(envoy_config::BufferPerRoute {
            disabled: false,
            buffer: Some(envoy_config::Buffer {
                max_request_bytes: 4,
            }),
        });
        f.apply_route_config(Some(&route));
        // 5 > 4 → reject even though the chain base (100) would allow it.
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"hello")),
            Decision::StopAndSend(_)
        ));
    }

    #[test]
    fn per_route_empty_falls_back_to_chain_base() {
        let mut f = filter(10);
        let route = route_with_buffer(envoy_config::BufferPerRoute {
            disabled: false,
            buffer: None,
        });
        f.apply_route_config(Some(&route));
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"hello world!!")), // 13 > 10
            Decision::StopAndSend(_)
        ));
    }

    #[test]
    fn get_no_body_passes() {
        let mut f = filter(10);
        f.apply_route_config(None);
        assert!(matches!(
            f.decode_headers(&mut req_with_body("GET", "/", b"")),
            Decision::Continue
        ));
    }

    #[test]
    fn zero_limit_rejects_any_nonempty_body() {
        // Residual disposition: max_request_bytes: 0 → reject iff body.len() > 0.
        let mut f = filter(0);
        f.apply_route_config(None);
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"x")),
            Decision::StopAndSend(_)
        ));
        assert!(matches!(
            f.decode_headers(&mut req_with_body("GET", "/", b"")),
            Decision::Continue
        ));
    }
}
