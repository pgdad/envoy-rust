//! `envoy.filters.http.cors` — origin allow-matching, the decode-side
//! preflight short-circuit, and the encode-side actual-request decoration.
//!
//! §6.2-verified against envoyproxy/envoy:v1.33.0 (phase-23 PLAN-write).
//!
//! ## Behaviour summary
//! - **Preflight** (`OPTIONS` + `Origin` + `Access-Control-Request-Method`):
//!   if the origin matches the configured allow-list → 200 short-circuit with
//!   the six conditional CORS response headers.  If the origin does NOT match
//!   → proxy through unchanged (Envoy does NOT short-circuit disallowed
//!   preflights).
//! - **Actual request** (any allowed-origin non-preflight): `decode_headers`
//!   stashes the origin; `encode_headers` decorates the upstream response with
//!   `access-control-allow-origin` + optionally `access-control-allow-credentials`
//!   + optionally `access-control-expose-headers`.
//! - **Disallowed / no-origin**: pass-through, no decoration.
//!
//! ## Wiring status
//! The filter is intentionally NOT wired into `HttpFilterInstance` at Task-3
//! scope; the `HttpFilterInstance::Cors` variant + `apply_route_config`
//! fan-out land in Task 4.  Dead-code lints for the public-crate items are
//! suppressed here until the Task-4 wiring activates them.
use std::sync::Arc;

use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

const CORS_FILTER_NAME: &str = "envoy.filters.http.cors";

// ---------------------------------------------------------------------------
// Compiled policy
// ---------------------------------------------------------------------------

/// Build-time-lowered `CorsPolicy`.  `allow_credentials` is unwrapped to
/// `bool` (defaults `false`).
#[derive(Debug, Clone)]
struct CompiledCorsPolicy {
    allow_origin: Vec<envoy_config::StringMatcher>,
    allow_methods: Option<String>,
    allow_headers: Option<String>,
    expose_headers: Option<String>,
    max_age: Option<String>,
    allow_credentials: bool,
}

impl From<&envoy_config::CorsPolicy> for CompiledCorsPolicy {
    fn from(p: &envoy_config::CorsPolicy) -> Self {
        Self {
            allow_origin: p.allow_origin_string_match.clone(),
            allow_methods: p.allow_methods.clone(),
            allow_headers: p.allow_headers.clone(),
            expose_headers: p.expose_headers.clone(),
            max_age: p.max_age.clone(),
            allow_credentials: p.allow_credentials.unwrap_or(false),
        }
    }
}

// ---------------------------------------------------------------------------
// CorsFilter
// ---------------------------------------------------------------------------

/// The `envoy.filters.http.cors` runtime filter.
///
/// Instantiated once per filter-chain via `build_from_config`; then, for each
/// request, `apply_route_config` slots in the per-route `CorsPolicy`
/// (or `None` when the route carries no cors policy), and the filter becomes
/// inert.
#[derive(Debug, Clone)]
pub struct CorsFilter {
    origin_valid: Arc<Counter>,
    origin_invalid: Arc<Counter>,
    /// Set per-request by `apply_route_config`.  `None` → filter is inert.
    active_policy: Option<CompiledCorsPolicy>,
    /// Set during `decode_headers` for an allowed non-preflight request.
    /// Consumed in `encode_headers` to decorate the upstream response.
    decorate_origin: Option<String>,
}

impl CorsFilter {
    /// Build the filter from its (currently empty) filter-chain config,
    /// registering the two stat counters under
    /// `http.{hcm_stat_prefix}.cors.{origin_valid,origin_invalid}`.
    pub(crate) fn build_from_config(
        _cfg: &envoy_config::CorsConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        let reg = |suffix: &str| {
            registry
                .register_counter(&format!("http.{hcm_stat_prefix}.cors.{suffix}"))
                .map_err(|e| FilterError::InvalidConfig {
                    message: format!("StatsRegistry: {e}"),
                })
        };
        Ok(Self {
            origin_valid: reg("origin_valid")?,
            origin_invalid: reg("origin_invalid")?,
            active_policy: None,
            decorate_origin: None,
        })
    }

    /// Select the per-route CORS policy for the current request.
    ///
    /// Called by the filter-pipeline fan-out (Task 4) after route resolution.
    /// When the matched route carries a `typed_per_filter_config` entry keyed
    /// `"envoy.filters.http.cors"` the entry is lowered into a
    /// `CompiledCorsPolicy`; otherwise `active_policy` stays `None` and the
    /// filter is inert for this request.
    pub(crate) fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
        self.active_policy = route
            .and_then(|r| r.typed_per_filter_config.get(CORS_FILTER_NAME))
            .and_then(|pfc| match pfc {
                envoy_config::PerFilterConfig::Cors(p) => Some(CompiledCorsPolicy::from(p)),
                _ => None,
            });
    }

    /// Decode-side filter entry point.
    ///
    /// - No active policy → `Continue`.
    /// - No `origin` header → `Continue` (L4; no stat tick).
    /// - Origin present + matched → tick `origin_valid`; if preflight → short-
    ///   circuit 200; else stash origin for encode decoration.
    /// - Origin present + unmatched → tick `origin_invalid`; `Continue`.
    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        let Some(ref policy) = self.active_policy else {
            return Decision::Continue;
        };
        let Some(origin) = header_ci(&req.headers, "origin").map(str::to_owned) else {
            return Decision::Continue;
        };

        let allowed = policy.allow_origin.iter().any(|m| m.matches(&origin));

        if allowed {
            self.origin_valid.inc();
        } else {
            self.origin_invalid.inc();
        }

        let is_preflight = req.method.eq_ignore_ascii_case("OPTIONS")
            && header_ci(&req.headers, "access-control-request-method").is_some();

        if is_preflight && allowed {
            return Decision::StopAndSend(build_preflight_response(policy, &origin));
        }

        if allowed && !is_preflight {
            self.decorate_origin = Some(origin);
        }

        Decision::Continue
    }

    /// Encode-side filter entry point.
    ///
    /// When an allowed non-preflight origin was stashed in `decode_headers`,
    /// push the actual-request decoration headers (allow-origin always;
    /// allow-credentials and expose-headers when configured) onto the upstream
    /// response.  Always returns `Continue` (never replaces the response).
    pub(crate) fn encode_headers(&mut self, resp: &mut FilterResponse) -> Decision {
        if let (Some(policy), Some(origin)) =
            (self.active_policy.as_ref(), self.decorate_origin.take())
        {
            resp.headers
                .push(("access-control-allow-origin".to_string(), origin));
            if policy.allow_credentials {
                resp.headers.push((
                    "access-control-allow-credentials".to_string(),
                    "true".to_string(),
                ));
            }
            if let Some(ref expose) = policy.expose_headers {
                resp.headers
                    .push(("access-control-expose-headers".to_string(), expose.clone()));
            }
        }
        Decision::Continue
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the 200 preflight short-circuit response.
///
/// Header emission order (all conditional on config):
/// 1. `access-control-allow-origin`  — always when origin allowed
/// 2. `access-control-allow-credentials` — if `allow_credentials`
/// 3. `access-control-allow-methods`  — if set
/// 4. `access-control-allow-headers`  — if set
/// 5. `access-control-max-age`        — if set
/// 6. `access-control-expose-headers` — if set
fn build_preflight_response(policy: &CompiledCorsPolicy, origin: &str) -> FilterResponse {
    let mut headers: Vec<(String, String)> = Vec::new();

    headers.push((
        "access-control-allow-origin".to_string(),
        origin.to_string(),
    ));
    if policy.allow_credentials {
        headers.push((
            "access-control-allow-credentials".to_string(),
            "true".to_string(),
        ));
    }
    if let Some(ref methods) = policy.allow_methods {
        headers.push(("access-control-allow-methods".to_string(), methods.clone()));
    }
    if let Some(ref hdrs) = policy.allow_headers {
        headers.push(("access-control-allow-headers".to_string(), hdrs.clone()));
    }
    if let Some(ref age) = policy.max_age {
        headers.push(("access-control-max-age".to_string(), age.clone()));
    }
    if let Some(ref expose) = policy.expose_headers {
        headers.push(("access-control-expose-headers".to_string(), expose.clone()));
    }

    FilterResponse {
        status: 200,
        reason: Some("OK"),
        headers,
        body: Bytes::new(),
    }
}

/// Case-insensitive header lookup — duplicated from `jwt_authn` per SC2
/// (no shared utility extraction).
fn header_ci<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{StringMatcher, StringMatcherMode};
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;

    // ---- helpers ----

    fn registry() -> Arc<StatsRegistry> {
        Arc::new(StatsRegistry::new())
    }

    /// A policy with:
    /// - allowed origin: `http://allowed.example.com`
    /// - allow_methods: "GET,POST"
    /// - allow_headers: "content-type,x-custom"
    /// - expose_headers: "x-response-time"
    /// - max_age: "600"
    /// - allow_credentials: true
    fn policy() -> envoy_config::CorsPolicy {
        envoy_config::CorsPolicy {
            allow_origin_string_match: vec![StringMatcher {
                mode: StringMatcherMode::Exact("http://allowed.example.com".to_string()),
                ignore_case: false,
            }],
            allow_methods: Some("GET,POST".to_string()),
            allow_headers: Some("content-type,x-custom".to_string()),
            expose_headers: Some("x-response-time".to_string()),
            max_age: Some("600".to_string()),
            allow_credentials: Some(true),
        }
    }

    /// Construct a `CorsFilter` via `build_from_config`, then set its
    /// `active_policy` directly (we are in the same module so the field is
    /// accessible).
    fn filter_with(reg: &Arc<StatsRegistry>, p: &envoy_config::CorsPolicy) -> CorsFilter {
        let mut f = CorsFilter::build_from_config(
            &envoy_config::CorsConfig::default(),
            reg,
            "ingress_http",
        )
        .expect("build succeeds");
        f.active_policy = Some(CompiledCorsPolicy::from(p));
        f
    }

    fn req(method: &str, headers: Vec<(&str, &str)>) -> FilterRequest {
        FilterRequest {
            method: method.to_string(),
            path: "/".to_string(),
            headers: headers
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: None,
            dynamic_metadata: std::collections::BTreeMap::new(),
        }
    }

    fn empty_resp() -> FilterResponse {
        FilterResponse {
            status: 200,
            reason: None,
            headers: vec![],
            body: Bytes::new(),
        }
    }

    fn origin_valid_value(reg: &Arc<StatsRegistry>) -> u64 {
        reg.register_counter("http.ingress_http.cors.origin_valid")
            .unwrap()
            .value()
    }

    fn origin_invalid_value(reg: &Arc<StatsRegistry>) -> u64 {
        reg.register_counter("http.ingress_http.cors.origin_invalid")
            .unwrap()
            .value()
    }

    // ---- test 1: preflight allowed → 200 short-circuit with all 6 headers ----

    #[test]
    fn preflight_allowed_short_circuits_200_with_headers() {
        let reg = registry();
        let p = policy();
        let mut f = filter_with(&reg, &p);

        let mut r = req(
            "OPTIONS",
            vec![
                ("origin", "http://allowed.example.com"),
                ("access-control-request-method", "GET"),
            ],
        );
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 200);
                assert_eq!(resp.body.len(), 0, "preflight body must be empty");

                let h = |name: &str| -> Option<&str> {
                    resp.headers
                        .iter()
                        .find(|(k, _)| k.as_str() == name)
                        .map(|(_, v)| v.as_str())
                };
                assert_eq!(
                    h("access-control-allow-origin"),
                    Some("http://allowed.example.com")
                );
                assert_eq!(h("access-control-allow-credentials"), Some("true"));
                assert_eq!(h("access-control-allow-methods"), Some("GET,POST"));
                assert_eq!(
                    h("access-control-allow-headers"),
                    Some("content-type,x-custom")
                );
                assert_eq!(h("access-control-max-age"), Some("600"));
                assert_eq!(h("access-control-expose-headers"), Some("x-response-time"));
            }
            Decision::Continue => panic!("expected StopAndSend for allowed preflight"),
        }
        assert_eq!(origin_valid_value(&reg), 1);
        assert_eq!(origin_invalid_value(&reg), 0);
    }

    // ---- test 2: preflight disallowed origin → Continue ----

    #[test]
    fn preflight_disallowed_origin_continues() {
        let reg = registry();
        let p = policy();
        let mut f = filter_with(&reg, &p);

        let mut r = req(
            "OPTIONS",
            vec![
                ("origin", "http://evil.example.com"),
                ("access-control-request-method", "GET"),
            ],
        );
        assert!(
            matches!(f.decode_headers(&mut r), Decision::Continue),
            "disallowed-origin preflight must proxy through"
        );
        assert_eq!(origin_invalid_value(&reg), 1);
        assert_eq!(origin_valid_value(&reg), 0);
    }

    // ---- test 3: OPTIONS without ACRM is an actual request, not a preflight ----

    #[test]
    fn options_without_acrm_is_actual_request_not_preflight() {
        let reg = registry();
        let p = policy();
        let mut f = filter_with(&reg, &p);

        // decode: OPTIONS + allowed origin but NO access-control-request-method
        let mut r = req("OPTIONS", vec![("origin", "http://allowed.example.com")]);
        assert!(
            matches!(f.decode_headers(&mut r), Decision::Continue),
            "OPTIONS without ACRM is an actual request; must not short-circuit"
        );
        // origin was valid so decorate_origin is stashed
        assert_eq!(
            f.decorate_origin.as_deref(),
            Some("http://allowed.example.com")
        );

        // encode: should add allow-origin header
        let mut resp = empty_resp();
        f.encode_headers(&mut resp);
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| k == "access-control-allow-origin"
                    && v == "http://allowed.example.com"),
            "encode must decorate allowed OPTIONS-without-ACRM"
        );
        assert_eq!(origin_valid_value(&reg), 1);
    }

    // ---- test 4: actual request allowed → encode decorates ----

    #[test]
    fn actual_request_allowed_decorates_on_encode() {
        let reg = registry();
        let p = policy();
        let mut f = filter_with(&reg, &p);

        let mut r = req("GET", vec![("origin", "http://allowed.example.com")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));

        let mut resp = empty_resp();
        f.encode_headers(&mut resp);

        let h = |name: &str| -> Option<&str> {
            resp.headers
                .iter()
                .find(|(k, _)| k.as_str() == name)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(
            h("access-control-allow-origin"),
            Some("http://allowed.example.com"),
            "allow-origin must be echoed"
        );
        assert_eq!(
            h("access-control-allow-credentials"),
            Some("true"),
            "allow-credentials must be present when configured"
        );
        assert_eq!(
            h("access-control-expose-headers"),
            Some("x-response-time"),
            "expose-headers must be present when configured"
        );
        // preflight-only headers must NOT appear on actual-request decoration
        assert!(
            h("access-control-allow-methods").is_none(),
            "allow-methods must NOT appear on actual-request encode"
        );
        assert!(
            h("access-control-allow-headers").is_none(),
            "allow-headers must NOT appear on actual-request encode"
        );
        assert!(
            h("access-control-max-age").is_none(),
            "max-age must NOT appear on actual-request encode"
        );
    }

    // ---- test 5: disallowed origin → no decoration ----

    #[test]
    fn disallowed_origin_no_decoration() {
        let reg = registry();
        let p = policy();
        let mut f = filter_with(&reg, &p);

        let mut r = req("GET", vec![("origin", "http://evil.example.com")]);
        f.decode_headers(&mut r);

        let mut resp = empty_resp();
        f.encode_headers(&mut resp);

        assert!(
            resp.headers.is_empty(),
            "no CORS headers must be added for disallowed origin"
        );
    }

    // ---- test 6: no origin → no action, no stats ----

    #[test]
    fn no_origin_no_action_no_stats() {
        let reg = registry();
        let p = policy();
        let mut f = filter_with(&reg, &p);

        let mut r = req("GET", vec![]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));

        let mut resp = empty_resp();
        f.encode_headers(&mut resp);

        assert!(
            resp.headers.is_empty(),
            "no headers added for no-origin request"
        );
        assert_eq!(
            origin_valid_value(&reg),
            0,
            "origin_valid must not tick for no-origin"
        );
        assert_eq!(
            origin_invalid_value(&reg),
            0,
            "origin_invalid must not tick for no-origin"
        );
    }

    // ---- test 7: no active policy → inert even for perfect preflight ----

    #[test]
    fn no_active_policy_is_inert() {
        let reg = registry();
        let mut f = CorsFilter::build_from_config(
            &envoy_config::CorsConfig::default(),
            &reg,
            "ingress_http",
        )
        .expect("build");
        // active_policy is None by default

        let mut r = req(
            "OPTIONS",
            vec![
                ("origin", "http://allowed.example.com"),
                ("access-control-request-method", "GET"),
            ],
        );
        assert!(
            matches!(f.decode_headers(&mut r), Decision::Continue),
            "inert filter (no active_policy) must Continue even on perfect preflight"
        );
        assert_eq!(origin_valid_value(&reg), 0);
        assert_eq!(origin_invalid_value(&reg), 0);
    }

    // ---- test 8: stats tick correctly across mixed requests ----

    #[test]
    fn stats_tick_once_per_present_origin() {
        let reg = registry();
        let p = policy();
        let mut f = filter_with(&reg, &p);

        // allowed GET → valid +1
        let mut r = req("GET", vec![("origin", "http://allowed.example.com")]);
        f.decode_headers(&mut r);

        // evil GET → invalid +1
        let mut r2 = req("GET", vec![("origin", "http://evil.example.com")]);
        f.decode_headers(&mut r2);

        // no origin → neither
        let mut r3 = req("GET", vec![]);
        f.decode_headers(&mut r3);

        assert_eq!(origin_valid_value(&reg), 1, "one valid tick");
        assert_eq!(origin_invalid_value(&reg), 1, "one invalid tick");
    }

    // ---- test 9: apply_route_config sets active_policy from Route ----

    #[test]
    fn apply_route_config_sets_policy_from_route() {
        let reg = registry();
        let mut f = CorsFilter::build_from_config(
            &envoy_config::CorsConfig::default(),
            &reg,
            "ingress_http",
        )
        .expect("build");

        let cors_policy = policy();
        let pfc = envoy_config::PerFilterConfig::Cors(cors_policy.clone());
        let mut pfc_map = BTreeMap::new();
        pfc_map.insert(CORS_FILTER_NAME.to_string(), pfc);
        let route = envoy_config::Route {
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
        };

        assert!(f.active_policy.is_none(), "initially no policy");
        f.apply_route_config(Some(&route));
        assert!(f.active_policy.is_some(), "policy set from route");

        // confirm the filter now acts on the policy
        let mut r = req(
            "OPTIONS",
            vec![
                ("origin", "http://allowed.example.com"),
                ("access-control-request-method", "GET"),
            ],
        );
        assert!(
            matches!(f.decode_headers(&mut r), Decision::StopAndSend(_)),
            "should short-circuit after apply_route_config"
        );
    }

    // ---- test 10: apply_route_config with None route clears policy ----

    #[test]
    fn apply_route_config_none_clears_policy() {
        let reg = registry();
        let p = policy();
        let mut f = filter_with(&reg, &p);
        assert!(f.active_policy.is_some());
        f.apply_route_config(None);
        assert!(
            f.active_policy.is_none(),
            "None route must clear the policy"
        );
    }

    // ---- test 11: apply_route_config with route that has no cors key → None ----

    #[test]
    fn apply_route_config_route_without_cors_key_is_none() {
        let reg = registry();
        let mut f = CorsFilter::build_from_config(
            &envoy_config::CorsConfig::default(),
            &reg,
            "ingress_http",
        )
        .expect("build");

        // Route with no typed_per_filter_config entries at all.
        let route = envoy_config::Route {
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
            typed_per_filter_config: BTreeMap::new(),
        };

        f.apply_route_config(Some(&route));
        assert!(
            f.active_policy.is_none(),
            "route without envoy.filters.http.cors key must leave active_policy None"
        );
    }

    // ---- test 12: minimal policy (no creds, no expose) → encode emits only allow-origin ----

    #[test]
    fn minimal_policy_encode_emits_only_allow_origin() {
        let reg = registry();

        // Construct a minimal CorsPolicy: only allow_origin_string_match set,
        // everything else None/false.
        let minimal = envoy_config::CorsPolicy {
            allow_origin_string_match: vec![StringMatcher {
                mode: StringMatcherMode::Exact("http://allowed.example.com".to_string()),
                ignore_case: false,
            }],
            allow_methods: None,
            allow_headers: None,
            expose_headers: None,
            max_age: None,
            allow_credentials: None,
        };
        let mut f = filter_with(&reg, &minimal);

        let mut r = req("GET", vec![("origin", "http://allowed.example.com")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));

        let mut resp = empty_resp();
        f.encode_headers(&mut resp);

        assert_eq!(
            resp.headers.len(),
            1,
            "exactly one header must be added for minimal policy"
        );
        assert_eq!(
            resp.headers[0],
            (
                "access-control-allow-origin".to_string(),
                "http://allowed.example.com".to_string()
            ),
            "the sole header must be access-control-allow-origin"
        );
        assert!(
            resp.headers
                .iter()
                .all(|(k, _)| k != "access-control-allow-credentials"),
            "allow-credentials must be absent for minimal policy"
        );
        assert!(
            resp.headers
                .iter()
                .all(|(k, _)| k != "access-control-expose-headers"),
            "expose-headers must be absent for minimal policy"
        );
    }
}
