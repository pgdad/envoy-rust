//! The `envoy.filters.http.fault` runtime filter — abort path (phase 11).
//!
//! Decode-side filter: on a request matching the optional header gate (AND
//! semantics over a `Vec<HeaderMatcher>`) when the deterministic percentage
//! selects (0%/100% only at phase-11 scope), short-circuits via
//! `Decision::StopAndSend` with the operator-configured HTTP status + the
//! source-hardcoded abort body. The standard response headers are decorated by
//! the HCM filter-synth decoration helpers (H1: `decorate_filter_synth_response`;
//! H2: `decorate_filter_synth_response_h2`, phase-11 D6).

use std::sync::Arc;

use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// Upstream Envoy v1.33's source-hardcoded fault-abort body (18 bytes;
/// §6.2-verified at phase-11 state-2 PLAN-write against `envoyproxy/envoy:v1.33.0`).
const FAULT_ABORT_BODY: &[u8] = b"fault filter abort";

/// The `envoy.filters.http.fault` runtime filter (abort path).
#[derive(Debug, Clone)]
pub struct FaultFilter {
    abort_status: u16,
    /// `true` iff the percentage is 100% (per `FractionalPercent::selects_deterministic`);
    /// computed once at build time — no per-request randomness.
    abort_selects: bool,
    /// Optional gate; empty ⇒ the fault applies to all requests.
    header_gate: Vec<envoy_config::HeaderMatcher>,
    aborts_injected: Arc<Counter>,
}

impl FaultFilter {
    /// Lower an `envoy_config::FaultConfig` into the runtime filter + register
    /// the abort counter under `http.{hcm_stat_prefix}.fault.aborts_injected`.
    pub(crate) fn build_from_config(
        cfg: &envoy_config::FaultConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        let aborts_injected = registry
            .register_counter(&format!("http.{hcm_stat_prefix}.fault.aborts_injected"))
            .map_err(|e| FilterError::InvalidConfig {
                message: format!("StatsRegistry: {e}"),
            })?;
        Ok(Self {
            abort_status: cfg.abort.http_status,
            abort_selects: cfg.abort.percentage.selects_deterministic(),
            header_gate: cfg.headers.clone(),
            aborts_injected,
        })
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        if header_gate_matches(&self.header_gate, req) && self.abort_selects {
            self.aborts_injected.inc();
            return Decision::StopAndSend(FilterResponse {
                status: self.abort_status,
                reason: None,
                headers: vec![],
                body: Bytes::from_static(FAULT_ABORT_BODY),
            });
        }
        Decision::Continue
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        // Decode-only filter at phase-11 scope (response-rate-limit defers).
        Decision::Continue
    }
}

/// All listed matchers must match (AND semantics) per upstream. An empty gate
/// returns `true` (`Iterator::all` over an empty slice) — no gate ⇒ all requests.
fn header_gate_matches(gate: &[envoy_config::HeaderMatcher], req: &FilterRequest) -> bool {
    gate.iter().all(|m| m.matches(&req.headers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{
        DenominatorType, FaultAbort, FaultConfig, FractionalPercent, HeaderMatcher,
        HeaderMatcherMode, StringMatcher, StringMatcherMode,
    };
    use envoy_stats::StatsRegistry;
    use std::sync::Arc;

    fn cfg(numerator: u32, headers: Vec<HeaderMatcher>) -> FaultConfig {
        FaultConfig {
            abort: FaultAbort {
                http_status: 503,
                percentage: FractionalPercent {
                    numerator,
                    denominator: DenominatorType::Hundred,
                },
            },
            headers,
        }
    }

    fn header_matcher_exact(name: &str, value: &str) -> HeaderMatcher {
        HeaderMatcher {
            name: name.to_string(),
            mode: HeaderMatcherMode::StringMatch(StringMatcher {
                mode: StringMatcherMode::Exact(value.to_string()),
                ignore_case: false,
            }),
            invert_match: false,
        }
    }

    fn req(headers: Vec<(String, String)>) -> FilterRequest {
        FilterRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers,
            body: None,
            dynamic_metadata: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn abort_100_percent_no_gate_aborts_every_request() {
        let registry = Arc::new(StatsRegistry::new());
        let mut f =
            FaultFilter::build_from_config(&cfg(100, vec![]), &registry, "ingress_http").unwrap();
        let mut r = req(vec![]);
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 503);
                assert_eq!(resp.body.as_ref(), b"fault filter abort");
                assert_eq!(resp.body.len(), 18);
                assert!(
                    resp.headers.is_empty(),
                    "filter adds no headers; HCM decorates"
                );
            }
            Decision::Continue => panic!("expected abort"),
        }
    }

    #[test]
    fn abort_0_percent_never_aborts() {
        let registry = Arc::new(StatsRegistry::new());
        let mut f =
            FaultFilter::build_from_config(&cfg(0, vec![]), &registry, "ingress_http").unwrap();
        let mut r = req(vec![]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
    }

    #[test]
    fn header_gate_match_aborts_miss_passes() {
        let registry = Arc::new(StatsRegistry::new());
        let gate = vec![header_matcher_exact("x-fault", "abort")];
        let mut f =
            FaultFilter::build_from_config(&cfg(100, gate), &registry, "ingress_http").unwrap();

        // Gate matches → abort.
        let mut r_match = req(vec![("x-fault".to_string(), "abort".to_string())]);
        assert!(matches!(
            f.decode_headers(&mut r_match),
            Decision::StopAndSend(_)
        ));

        // Gate misses (no header) → pass.
        let mut r_miss = req(vec![]);
        assert!(matches!(f.decode_headers(&mut r_miss), Decision::Continue));
    }

    #[test]
    fn aborts_injected_counter_increments_once_per_abort_only() {
        let registry = Arc::new(StatsRegistry::new());
        let mut f =
            FaultFilter::build_from_config(&cfg(100, vec![]), &registry, "ingress_http").unwrap();
        let _ = f.decode_headers(&mut req(vec![]));
        let _ = f.decode_headers(&mut req(vec![]));
        // register_counter is idempotent — returns the existing handle.
        let counter = registry
            .register_counter("http.ingress_http.fault.aborts_injected")
            .expect("counter registered");
        assert_eq!(counter.value(), 2, "one increment per abort, never on pass");
    }

    #[test]
    fn encode_headers_is_noop() {
        let registry = Arc::new(StatsRegistry::new());
        let mut f =
            FaultFilter::build_from_config(&cfg(100, vec![]), &registry, "ingress_http").unwrap();
        let mut resp = FilterResponse {
            status: 200,
            reason: None,
            headers: vec![],
            body: bytes::Bytes::new(),
        };
        assert!(matches!(f.encode_headers(&mut resp), Decision::Continue));
    }
}
