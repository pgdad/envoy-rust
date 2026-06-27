//! `envoy.filters.http.csrf` — decode-side cross-site-request-forgery guard.
//!
//! §6.2-verified against envoyproxy/envoy:v1.33.0 (phase-24 PLAN-write; ADR-0061).
//!
//! ## Behaviour summary
//! - The chain-level `CsrfPolicy` is an always-applied BASE; a per-route
//!   `CsrfPolicy` (threaded via `apply_route_config`) REPLACES it wholesale
//!   (ADR-0061 L6). The effective policy's `filter_enabled` gates enforcement.
//! - For `{POST,PUT,DELETE,PATCH}` (the modify set, L2): compute the
//!   scheme-stripped `host[:port]` source origin (`Origin`, fallback `Referer`)
//!   vs target (`Host`/`:authority`); valid iff source == target OR an
//!   `additional_origins` matcher matches source (L3). Invalid / missing-source
//!   → 403 `Invalid origin` (L4). Safe methods + deterministic-0% → Continue.
//! - Decode-side only; `encode_headers` is the trivial `Continue` arm.
//!
//! ## Wiring status
//! Wired into `HttpFilterInstance::Csrf` (build/decode/encode/`apply_route_config`)
//! as of Task 3 — the module-level `#![allow(dead_code)]` used at Task-2
//! introduction is no longer needed now that every item is reachable from the
//! instance dispatch.
//!
//! `header_ci` is duplicated from jwt_authn/cors (now N=3); the shared-util
//! extraction stays deferred (the standing M-track consolidation item).
use std::sync::Arc;

use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

const CSRF_FILTER_NAME: &str = "envoy.filters.http.csrf";
const MODIFY_METHODS: &[&str] = &["POST", "PUT", "DELETE", "PATCH"];
const FAILURE_BODY: &[u8] = b"Invalid origin"; // 14 bytes, no newline (ADR-0061 L4)

// ---------------------------------------------------------------------------
// Compiled policy
// ---------------------------------------------------------------------------

/// Build-time-lowered `CsrfPolicy`. `enabled` collapses `filter_enabled` to the
/// deterministic boolean (validated 0%/100% — ADR-0061 L6).
#[derive(Debug, Clone)]
struct CompiledCsrfPolicy {
    enabled: bool,
    additional_origins: Vec<envoy_config::StringMatcher>,
}

impl From<&envoy_config::CsrfPolicy> for CompiledCsrfPolicy {
    fn from(p: &envoy_config::CsrfPolicy) -> Self {
        Self {
            enabled: p.filter_enabled.default_value.selects_deterministic(),
            additional_origins: p.additional_origins.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// CsrfFilter
// ---------------------------------------------------------------------------

/// The `envoy.filters.http.csrf` runtime filter.
///
/// Instantiated once per filter-chain via `build_from_config` (which compiles
/// the chain-level base policy); then, for each request, `apply_route_config`
/// selects the effective policy — the route's `CsrfPolicy` override if present,
/// else the chain base (ADR-0061 L6). This DIFFERS from `cors`, which goes
/// inert without a route config.
#[derive(Debug, Clone)]
pub struct CsrfFilter {
    request_valid: Arc<Counter>,
    request_invalid: Arc<Counter>,
    missing_source_origin: Arc<Counter>,
    /// Compiled chain-level policy (the always-applied BASE, ADR-0061 L6).
    base_policy: CompiledCsrfPolicy,
    /// The effective policy for the current request: the route override if the
    /// matched route carries one, else a clone of `base_policy`.
    active_policy: CompiledCsrfPolicy,
}

impl CsrfFilter {
    /// Build the filter from its chain-level `CsrfPolicy`, registering the three
    /// mutually-exclusive stat counters under
    /// `http.{hcm_stat_prefix}.csrf.{request_valid,request_invalid,missing_source_origin}`.
    pub(crate) fn build_from_config(
        cfg: &envoy_config::CsrfPolicy,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        let reg = |suffix: &str| {
            registry
                .register_counter(&format!("http.{hcm_stat_prefix}.csrf.{suffix}"))
                .map_err(|e| FilterError::InvalidConfig {
                    message: format!("StatsRegistry: {e}"),
                })
        };
        let base = CompiledCsrfPolicy::from(cfg);
        Ok(Self {
            request_valid: reg("request_valid")?,
            request_invalid: reg("request_invalid")?,
            missing_source_origin: reg("missing_source_origin")?,
            active_policy: base.clone(),
            base_policy: base,
        })
    }

    /// Select the effective per-request policy (ADR-0061 L6): the route's
    /// `CsrfPolicy` override if present, else the chain-level base. A route with
    /// NO csrf override is STILL guarded by the chain base.
    pub(crate) fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
        self.active_policy = route
            .and_then(|r| r.typed_per_filter_config.get(CSRF_FILTER_NAME))
            .and_then(|pfc| match pfc {
                envoy_config::PerFilterConfig::Csrf(p) => Some(CompiledCsrfPolicy::from(p)),
                _ => None,
            })
            .unwrap_or_else(|| self.base_policy.clone());
    }

    /// Decode-side filter entry point.
    ///
    /// - Disabled (deterministic-0%) → `Continue`, no stat (L6).
    /// - Safe method (not in the modify set) → `Continue`, no stat (L2).
    /// - Missing source origin → tick `missing_source_origin`; 403 (L4).
    /// - Source matches target / an `additional_origins` matcher → tick
    ///   `request_valid`; `Continue` (L3).
    /// - Otherwise → tick `request_invalid`; 403 (L4).
    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        if !self.active_policy.enabled {
            return Decision::Continue; // deterministic-0% (L6)
        }
        if !MODIFY_METHODS.iter().any(|m| req.method == *m) {
            return Decision::Continue; // safe method (L2) — no stat
        }
        // Source origin: Origin, fallback Referer; reduced to host[:port] (L3).
        let source = header_ci(&req.headers, "origin")
            .or_else(|| header_ci(&req.headers, "referer"))
            .map(host_and_port)
            .filter(|s| !s.is_empty());
        let Some(source) = source else {
            self.missing_source_origin.inc();
            return Decision::StopAndSend(failure_response());
        };
        // missing/empty Host → "" never equals a real (non-empty) source.
        let target = header_ci(&req.headers, "host")
            .map(host_and_port)
            .unwrap_or("");
        let allowed = source == target
            || self
                .active_policy
                .additional_origins
                .iter()
                .any(|m| m.matches(source));
        if allowed {
            self.request_valid.inc();
            Decision::Continue
        } else {
            self.request_invalid.inc();
            Decision::StopAndSend(failure_response())
        }
    }

    /// CSRF is decode-side only; encode is a no-op (the exhaustive-match arm for
    /// the Task-3 `HttpFilterInstance` wiring, SC7).
    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn failure_response() -> FilterResponse {
    FilterResponse {
        status: 403,
        reason: Some("Forbidden"),
        headers: Vec::new(),
        body: Bytes::from_static(FAILURE_BODY),
    }
}

/// Reduce an origin/host value to the scheme-stripped `host[:port]` authority
/// (Envoy `Url::hostAndPort()` semantics, ADR-0061 L3). If the value carries a
/// `scheme://` prefix, return the authority up to the next `/`, `?`, or `#`
/// (or end); otherwise return the value unchanged (a bare `Host: h:p` is already
/// an authority). Borrowing — no allocation.
fn host_and_port(value: &str) -> &str {
    match value.split_once("://") {
        Some((_scheme, rest)) => rest.split(['/', '?', '#']).next().unwrap_or(""),
        None => value,
    }
}

/// Case-insensitive header lookup — duplicated from jwt_authn/cors per SC5
/// (no shared utility extraction; N=3).
fn header_ci<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{
        CsrfPolicy, DenominatorType, FractionalPercent, RuntimeFractionalPercent, StringMatcher,
        StringMatcherMode,
    };
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn fe(n: u32) -> RuntimeFractionalPercent {
        RuntimeFractionalPercent {
            default_value: FractionalPercent {
                numerator: n,
                denominator: DenominatorType::Hundred,
            },
            runtime_key: None,
        }
    }
    fn policy(n: u32, addl: &[&str]) -> CsrfPolicy {
        CsrfPolicy {
            filter_enabled: fe(n),
            additional_origins: addl
                .iter()
                .map(|s| StringMatcher {
                    mode: StringMatcherMode::Exact(s.to_string()),
                    ignore_case: false,
                })
                .collect(),
        }
    }
    fn reg() -> Arc<StatsRegistry> {
        Arc::new(StatsRegistry::new())
    }
    fn req(method: &str, headers: &[(&str, &str)]) -> FilterRequest {
        FilterRequest {
            method: method.into(),
            path: "/".into(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: None,
            dynamic_metadata: std::collections::BTreeMap::new(),
        }
    }
    fn cval(r: &Arc<StatsRegistry>, s: &str) -> u64 {
        r.register_counter(&format!("http.ingress_http.csrf.{s}"))
            .unwrap()
            .value()
    }
    fn route_with_csrf(p: CsrfPolicy) -> envoy_config::Route {
        let mut pfc_map = BTreeMap::new();
        pfc_map.insert(
            CSRF_FILTER_NAME.to_string(),
            envoy_config::PerFilterConfig::Csrf(p),
        );
        envoy_config::Route {
            name: String::new(),
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
            typed_per_filter_config: pfc_map,
        }
    }

    // host_and_port (ADR-0061 L3)
    #[test]
    fn host_and_port_strips_scheme() {
        assert_eq!(
            host_and_port("http://additional.example.com"),
            "additional.example.com"
        );
        assert_eq!(host_and_port("http://localhost:10000"), "localhost:10000");
        assert_eq!(
            host_and_port("http://localhost:10000/page?q=1"),
            "localhost:10000"
        );
        assert_eq!(host_and_port("localhost:10000"), "localhost:10000"); // bare Host, used verbatim
        assert_eq!(host_and_port(""), "");
        assert_eq!(host_and_port("http://"), ""); // scheme present, empty authority → "" (→ missing_source_origin)
    }

    // missing-vs-invalid boundary: the two non-valid stat sites in ISOLATION (a
    // swap of the two `.inc()` sites would survive the aggregate matrix test).
    #[test]
    fn missing_source_ticks_missing_not_invalid() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(100, &[]), &r, "ingress_http").unwrap();
        f.apply_route_config(None);
        // POST with a Host but NO Origin/Referer → missing source, distinct stat.
        assert!(matches!(
            f.decode_headers(&mut req("POST", &[("host", "localhost:10000")])),
            Decision::StopAndSend(_)
        ));
        assert_eq!(cval(&r, "missing_source_origin"), 1);
        assert_eq!(cval(&r, "request_invalid"), 0);
        assert_eq!(cval(&r, "request_valid"), 0);
        // An Origin with empty authority (http://) is ALSO a missing source, not invalid.
        assert!(matches!(
            f.decode_headers(&mut req(
                "POST",
                &[("host", "localhost:10000"), ("origin", "http://")]
            )),
            Decision::StopAndSend(_)
        ));
        assert_eq!(cval(&r, "missing_source_origin"), 2);
        assert_eq!(cval(&r, "request_invalid"), 0);
    }

    // chain-base: route WITHOUT override is guarded by the chain policy (L6)
    #[test]
    fn chain_base_guards_without_route_override() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(100, &[]), &r, "ingress_http").unwrap();
        f.apply_route_config(None); // no route override → chain base applies
        let d = f.decode_headers(&mut req(
            "POST",
            &[
                ("host", "localhost:10000"),
                ("origin", "http://evil.example.com"),
            ],
        ));
        match d {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 403);
                assert_eq!(&resp.body[..], b"Invalid origin");
            }
            _ => panic!("expected 403"),
        }
        assert_eq!(cval(&r, "request_invalid"), 1);
    }

    // route-replace: chain=100 + route override 0% → passthrough (L6)
    #[test]
    fn route_override_zero_disables_enforcing_chain() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(100, &[]), &r, "ingress_http").unwrap();
        let route = route_with_csrf(policy(0, &[]));
        f.apply_route_config(Some(&route));
        assert!(matches!(
            f.decode_headers(&mut req(
                "POST",
                &[("host", "h"), ("origin", "http://evil")]
            )),
            Decision::Continue
        ));
    }

    // route-replace: chain=0 + route override 100% → enforce (L6)
    #[test]
    fn route_override_hundred_enables_disabled_chain() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(0, &[]), &r, "ingress_http").unwrap();
        let route = route_with_csrf(policy(100, &[]));
        f.apply_route_config(Some(&route));
        assert!(matches!(
            f.decode_headers(&mut req(
                "POST",
                &[("host", "h"), ("origin", "http://evil")]
            )),
            Decision::StopAndSend(_)
        ));
    }

    // safe methods passthrough, no stat (L2)
    #[test]
    fn safe_methods_pass_without_stat() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(100, &[]), &r, "ingress_http").unwrap();
        f.apply_route_config(None);
        for m in ["GET", "HEAD", "OPTIONS", "TRACE"] {
            assert!(
                matches!(
                    f.decode_headers(&mut req(m, &[("host", "h"), ("origin", "http://evil")])),
                    Decision::Continue
                ),
                "{m}"
            );
        }
        assert_eq!(cval(&r, "request_valid"), 0);
        assert_eq!(cval(&r, "request_invalid"), 0);
        assert_eq!(cval(&r, "missing_source_origin"), 0);
    }

    // modify set guarded (L2)
    #[test]
    fn modify_methods_guarded() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(100, &[]), &r, "ingress_http").unwrap();
        f.apply_route_config(None);
        for m in ["POST", "PUT", "DELETE", "PATCH"] {
            assert!(
                matches!(
                    f.decode_headers(&mut req(m, &[("host", "h"), ("origin", "http://evil")])),
                    Decision::StopAndSend(_)
                ),
                "{m}"
            );
        }
    }

    // same-origin valid; additional allowed; Referer fallback; Origin precedence; missing-source (L3,L5)
    #[test]
    fn origin_matrix_and_mutually_exclusive_stats() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(
            &policy(100, &["additional.example.com"]),
            &r,
            "ingress_http",
        )
        .unwrap();
        f.apply_route_config(None);
        let host = ("host", "localhost:10000");
        assert!(matches!(
            f.decode_headers(&mut req(
                "POST",
                &[host, ("origin", "http://localhost:10000")]
            )),
            Decision::Continue
        )); // same
        assert!(matches!(
            f.decode_headers(&mut req(
                "POST",
                &[host, ("origin", "http://additional.example.com")]
            )),
            Decision::Continue
        )); // additional
        assert!(matches!(
            f.decode_headers(&mut req(
                "POST",
                &[host, ("referer", "http://localhost:10000/p")]
            )),
            Decision::Continue
        )); // referer fallback same
        assert!(matches!(
            f.decode_headers(&mut req(
                "POST",
                &[
                    host,
                    ("origin", "http://localhost:10000"),
                    ("referer", "http://evil/p")
                ]
            )),
            Decision::Continue
        )); // origin precedence
        assert!(matches!(
            f.decode_headers(&mut req("POST", &[host, ("referer", "http://evil/p")])),
            Decision::StopAndSend(_)
        )); // referer evil
        assert!(matches!(
            f.decode_headers(&mut req("POST", &[host])),
            Decision::StopAndSend(_)
        )); // missing source
        assert_eq!(cval(&r, "request_valid"), 4);
        assert_eq!(cval(&r, "request_invalid"), 1);
        assert_eq!(cval(&r, "missing_source_origin"), 1);
    }

    // deterministic-0% chain → passthrough (L6)
    #[test]
    fn filter_enabled_zero_passes_through() {
        let r = reg();
        let mut f = CsrfFilter::build_from_config(&policy(0, &[]), &r, "ingress_http").unwrap();
        f.apply_route_config(None);
        assert!(matches!(
            f.decode_headers(&mut req(
                "POST",
                &[("host", "h"), ("origin", "http://evil")]
            )),
            Decision::Continue
        ));
    }
}
