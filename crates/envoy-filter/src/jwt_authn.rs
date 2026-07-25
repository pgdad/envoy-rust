//! The `envoy.filters.http.jwt_authn` runtime filter (phase 22, minimum-viable).
//!
//! Decode-side authentication gate: selects the first `rules[]` entry whose
//! `RouteMatch` matches the request, extracts the JWT from `Authorization:
//! Bearer`, verifies RS256 against the rule's provider JWKS, and validates
//! `iss`/`aud`/`exp`/`nbf` (`envoy-jwt`). On success: `Decision::Continue`,
//! `allowed.inc()`, and (when the provider's `forward` is false, the default)
//! the `Authorization` header is stripped (§6.2 L6). On failure: `denied.inc()`
//! and a `Decision::StopAndSend` 401/403 with the Envoy-faithful body + a
//! `www-authenticate` header. A request matching NO rule is allowed (§6.2 L4).
//! The standard response headers are decorated by the existing HCM filter-synth
//! helpers (H1 `decorate_filter_synth_response`; H2
//! `decorate_filter_synth_response_h2`) — unchanged.

use std::sync::Arc;

use bytes::Bytes;
use envoy_jwt::{JwkSet, JwtError};
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse, header_ci};

#[derive(Debug, Clone)]
struct CompiledProvider {
    issuer: String,
    audiences: Vec<String>,
    jwks: Arc<JwkSet>,
    forward: bool,
}

#[derive(Debug, Clone)]
struct CompiledRule {
    r#match: envoy_config::RouteMatch,
    provider: Arc<CompiledProvider>,
}

#[derive(Debug, Clone)]
pub struct JwtAuthnFilter {
    rules: Arc<Vec<CompiledRule>>,
    allowed: Arc<Counter>,
    denied: Arc<Counter>,
}

impl JwtAuthnFilter {
    pub(crate) fn build_from_config(
        cfg: &envoy_config::JwtAuthnConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        let mut compiled: std::collections::BTreeMap<String, Arc<CompiledProvider>> =
            std::collections::BTreeMap::new();
        for (name, p) in &cfg.providers {
            let jwks_json = p.local_jwks.inline_string.as_deref().ok_or_else(|| {
                FilterError::InvalidConfig {
                    message: format!("jwt_authn provider {name}: local_jwks not inline"),
                }
            })?;
            let jwks = JwkSet::parse(jwks_json).map_err(|e| FilterError::InvalidConfig {
                message: format!("jwt_authn provider {name}: invalid JWKS: {e}"),
            })?;
            compiled.insert(
                name.clone(),
                Arc::new(CompiledProvider {
                    issuer: p.issuer.clone(),
                    audiences: p.audiences.clone(),
                    jwks: Arc::new(jwks),
                    forward: p.forward,
                }),
            );
        }
        let mut rules = Vec::with_capacity(cfg.rules.len());
        for r in &cfg.rules {
            let provider = compiled
                .get(&r.requires.provider_name)
                .ok_or_else(|| FilterError::InvalidConfig {
                    message: format!(
                        "jwt_authn rule references unknown provider {}",
                        r.requires.provider_name
                    ),
                })?
                .clone();
            rules.push(CompiledRule {
                r#match: r.r#match.clone(),
                provider,
            });
        }
        let reg = |suffix: &str| {
            crate::error::register_counter(
                registry,
                &format!("http.{hcm_stat_prefix}.jwt_authn.{suffix}"),
            )
        };
        Ok(Self {
            rules: Arc::new(rules),
            allowed: reg("allowed")?,
            denied: reg("denied")?,
        })
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        let Some(rule) = self
            .rules
            .iter()
            .find(|r| route_match_matches(&r.r#match, &req.path, &req.headers))
        else {
            self.allowed.inc();
            return Decision::Continue;
        };
        let provider = rule.provider.clone();

        let token = bearer_token(&req.headers);
        let realm = realm(&req.path, &req.headers);

        let Some(token) = token else {
            self.denied.inc();
            return missing_reply(&realm);
        };

        let now = now_unix();
        match envoy_jwt::verify_rs256(
            token,
            &provider.jwks,
            &provider.issuer,
            &provider.audiences,
            now,
        ) {
            Ok(_) => {
                self.allowed.inc();
                if !provider.forward {
                    strip_authorization(&mut req.headers);
                }
                Decision::Continue
            }
            Err(e) => {
                self.denied.inc();
                error_reply(&e, &realm)
            }
        }
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Extracts the token after a literal `Bearer ` scheme prefix. Case-sensitive
/// per RFC 6750 §2.1 (the scheme identifier is `Bearer`, not case-folded); a
/// lowercase `bearer` is treated as a non-Bearer token (→ "Jwt is missing").
fn bearer_token(headers: &[(String, String)]) -> Option<&str> {
    header_ci(headers, "authorization").and_then(|v| v.strip_prefix("Bearer "))
}

fn strip_authorization(headers: &mut Vec<(String, String)>) {
    headers.retain(|(k, _)| !k.eq_ignore_ascii_case("authorization"));
}

fn realm(path: &str, headers: &[(String, String)]) -> String {
    let host = header_ci(headers, "host").unwrap_or("");
    format!("http://{host}{path}")
}

/// First-matching-rule path+header evaluation, mirroring the HCM
/// `route_matches` (prefix XOR path, AND-combined header matchers).
fn route_match_matches(
    m: &envoy_config::RouteMatch,
    path: &str,
    headers: &[(String, String)],
) -> bool {
    let path_ok = match (&m.prefix, &m.path) {
        (Some(p), None) => path.starts_with(p),
        (None, Some(p)) => path == p,
        // (None, None) or (Some, Some) are rejected by envoy-config validation
        // (UnsupportedRouteMatcher); treat as no-match (dead rule) defensively.
        _ => false,
    };
    path_ok && m.headers.iter().all(|hm| hm.matches(headers))
}

fn www_authenticate(realm: &str, with_error: bool) -> (String, String) {
    let v = if with_error {
        format!(r#"Bearer realm="{realm}", error="invalid_token""#)
    } else {
        format!(r#"Bearer realm="{realm}""#)
    };
    ("www-authenticate".to_string(), v)
}

fn missing_reply(realm: &str) -> Decision {
    Decision::StopAndSend(FilterResponse {
        status: 401,
        reason: Some("Unauthorized"),
        headers: vec![www_authenticate(realm, false)],
        body: Bytes::from_static(b"Jwt is missing"),
    })
}

/// Map a `JwtError` to its Envoy-faithful (status, body) + `www-authenticate`
/// (all non-missing classes carry `error="invalid_token"`). Bytes verified at
/// §6.2 L2.
fn error_reply(e: &JwtError, realm: &str) -> Decision {
    let (status, body): (u16, &'static [u8]) = match e {
        JwtError::NotInForm => (
            401,
            b"Jwt is not in the form of Header.Payload.Signature with two dots and 3 sections",
        ),
        JwtError::BadHeaderJson => (401, b"Jwt header is an invalid JSON"),
        JwtError::BadPayloadJson => (401, b"Jwt payload is an invalid JSON"),
        JwtError::NoMatchingKey => (401, b"Jwks doesn't have key to match kid or alg from Jwt"),
        JwtError::VerificationFails => (401, b"Jwt verification fails"),
        JwtError::IssuerMismatch => (401, b"Jwt issuer is not configured"),
        JwtError::Expired => (401, b"Jwt is expired"),
        JwtError::NotYetValid => (401, b"Jwt not yet valid"),
        JwtError::AudienceNotAllowed => (403, b"Audiences in Jwt are not allowed"),
        // InvalidJwks is config-load-time only; verify_rs256 never returns it on
        // the data path. Covered to stay exhaustive across the crate boundary.
        JwtError::InvalidJwks => (401, b"Jwt verification fails"),
    };
    let reason = if status == 403 {
        "Forbidden"
    } else {
        "Unauthorized"
    };
    Decision::StopAndSend(FilterResponse {
        status,
        reason: Some(reason),
        headers: vec![www_authenticate(realm, true)],
        body: Bytes::from_static(body),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_jwt::test_support::{self, RsaKeyPair, keypair};
    use envoy_stats::StatsRegistry;

    // Real-RS256 signing helpers live in envoy_jwt::test_support (behind its
    // `test-util` feature); this shim just fixes alg to RS256.
    fn make_token(kp: &RsaKeyPair, payload: &str) -> String {
        test_support::make_token(kp, "RS256", payload)
    }

    const ISS: &str = "testing@secure.istio.io";

    // ---- config + request builders ----

    fn build_cfg(
        jwks: &str,
        issuer: &str,
        audiences: Vec<String>,
        forward: bool,
        rule_prefix: &str,
    ) -> envoy_config::JwtAuthnConfig {
        let mut providers = std::collections::BTreeMap::new();
        providers.insert(
            "prov".to_string(),
            envoy_config::JwtProvider {
                issuer: issuer.to_string(),
                audiences,
                local_jwks: envoy_config::DataSource {
                    filename: None,
                    inline_string: Some(jwks.to_string()),
                },
                forward,
            },
        );
        envoy_config::JwtAuthnConfig {
            providers,
            rules: vec![envoy_config::RequirementRule {
                r#match: envoy_config::RouteMatch {
                    prefix: Some(rule_prefix.to_string()),
                    path: None,
                    headers: vec![],
                },
                requires: envoy_config::JwtRequirement {
                    provider_name: "prov".to_string(),
                },
            }],
        }
    }

    fn req(headers: Vec<(String, String)>, path: &str) -> FilterRequest {
        FilterRequest {
            headers,
            ..FilterRequest::test("GET", path, &[])
        }
    }

    fn registry() -> Arc<StatsRegistry> {
        Arc::new(StatsRegistry::new())
    }

    fn allowed_value(registry: &Arc<StatsRegistry>) -> u64 {
        registry
            .register_counter("http.ingress_http.jwt_authn.allowed")
            .unwrap()
            .value()
    }

    fn denied_value(registry: &Arc<StatsRegistry>) -> u64 {
        registry
            .register_counter("http.ingress_http.jwt_authn.denied")
            .unwrap()
            .value()
    }

    fn auth(token: &str) -> (String, String) {
        ("authorization".to_string(), format!("Bearer {token}"))
    }

    fn host() -> (String, String) {
        ("host".to_string(), "envoy.test".to_string())
    }

    // ---- 1. valid token, matched rule ----

    #[test]
    fn valid_token_matched_rule_continues_strips_auth() {
        let (kp, jwks) = keypair();
        let reg = registry();
        let cfg = build_cfg(&jwks, ISS, vec!["a".to_string()], false, "/secure");
        let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();
        let tok = make_token(
            &kp,
            r#"{"iss":"testing@secure.istio.io","aud":["a"],"exp":4102444800}"#,
        );
        let mut r = req(vec![host(), auth(&tok)], "/secure/x");
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(allowed_value(&reg), 1);
        assert_eq!(denied_value(&reg), 0);
        assert!(
            !r.headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("authorization")),
            "authorization must be stripped when forward=false"
        );
    }

    // ---- 2. missing token ----

    #[test]
    fn missing_token_denied_401_no_error_in_www_authenticate() {
        let (_kp, jwks) = keypair();
        let reg = registry();
        let cfg = build_cfg(&jwks, ISS, vec![], false, "/");
        let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();
        let mut r = req(vec![host()], "/");
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 401);
                assert_eq!(resp.reason, Some("Unauthorized"));
                assert_eq!(resp.body.as_ref(), b"Jwt is missing");
                let wa = resp
                    .headers
                    .iter()
                    .find(|(k, _)| k == "www-authenticate")
                    .expect("www-authenticate present");
                assert_eq!(wa.1, r#"Bearer realm="http://envoy.test/""#);
                assert!(!wa.1.contains("error="), "no error= on missing");
            }
            Decision::Continue => panic!("expected StopAndSend"),
        }
        assert_eq!(denied_value(&reg), 1);
        assert_eq!(allowed_value(&reg), 0);
    }

    // ---- 3. tampered signature ----

    #[test]
    fn tampered_signature_401_verification_fails() {
        let (kp, jwks) = keypair();
        let reg = registry();
        let cfg = build_cfg(&jwks, ISS, vec![], false, "/");
        let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();
        let mut tok = make_token(&kp, r#"{"iss":"testing@secure.istio.io","exp":4102444800}"#);
        // Corrupt the signature's FIRST base64url char (its value owns the top
        // 6 bits of signature byte 0, so a guaranteed-different replacement
        // always alters the decoded signature). NOT the last char: a 256-byte
        // RSA signature's final base64url char carries only 2 meaningful bits,
        // so replacing it can be a no-op under non-canonical-tolerant base64url
        // decoding (~1/4 of random keys) — which made the previous
        // `pop()`+`push('A'/'B')` tamper flaky.
        let sig_start = tok.rfind('.').unwrap() + 1;
        let repl = if tok.as_bytes()[sig_start] == b'A' {
            'B'
        } else {
            'A'
        };
        tok.replace_range(sig_start..sig_start + 1, &repl.to_string());
        let mut r = req(vec![host(), auth(&tok)], "/");
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 401);
                assert_eq!(resp.body.as_ref(), b"Jwt verification fails");
                let wa = resp
                    .headers
                    .iter()
                    .find(|(k, _)| k == "www-authenticate")
                    .unwrap();
                assert!(wa.1.contains(r#", error="invalid_token""#));
            }
            Decision::Continue => panic!("expected StopAndSend"),
        }
        assert_eq!(denied_value(&reg), 1);
    }

    // ---- 4. expired ----

    #[test]
    fn expired_token_401() {
        let (kp, jwks) = keypair();
        let reg = registry();
        let cfg = build_cfg(&jwks, ISS, vec![], false, "/");
        let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();
        // exp in the past (year 2017) vs the real wall clock.
        let tok = make_token(&kp, r#"{"iss":"testing@secure.istio.io","exp":1500000000}"#);
        let mut r = req(vec![host(), auth(&tok)], "/");
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 401);
                assert_eq!(resp.body.as_ref(), b"Jwt is expired");
                let wa = resp
                    .headers
                    .iter()
                    .find(|(k, _)| k == "www-authenticate")
                    .expect("www-authenticate present");
                assert!(
                    wa.1.contains(r#", error="invalid_token""#),
                    "expired must carry error=invalid_token"
                );
            }
            Decision::Continue => panic!("expected StopAndSend"),
        }
        assert_eq!(denied_value(&reg), 1);
    }

    // ---- 5. wrong audience ----

    #[test]
    fn wrong_audience_403_forbidden() {
        let (kp, jwks) = keypair();
        let reg = registry();
        let cfg = build_cfg(&jwks, ISS, vec!["allowed-aud".to_string()], false, "/");
        let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();
        let tok = make_token(
            &kp,
            r#"{"iss":"testing@secure.istio.io","aud":"other-aud","exp":4102444800}"#,
        );
        let mut r = req(vec![host(), auth(&tok)], "/");
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 403);
                assert_eq!(resp.reason, Some("Forbidden"));
                assert_eq!(resp.body.as_ref(), b"Audiences in Jwt are not allowed");
                let wa = resp
                    .headers
                    .iter()
                    .find(|(k, _)| k == "www-authenticate")
                    .unwrap();
                assert!(wa.1.contains(r#", error="invalid_token""#));
            }
            Decision::Continue => panic!("expected StopAndSend"),
        }
        assert_eq!(denied_value(&reg), 1);
    }

    // ---- 6. no-matching-rule ----

    #[test]
    fn no_matching_rule_continues_untouched() {
        let (kp, jwks) = keypair();
        let reg = registry();
        let cfg = build_cfg(&jwks, ISS, vec![], false, "/secure");
        let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();
        // valid-ish token present, but the path is NOT covered by the rule.
        let tok = make_token(&kp, r#"{"iss":"testing@secure.istio.io","exp":4102444800}"#);
        let mut r = req(vec![host(), auth(&tok)], "/public");
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(allowed_value(&reg), 1);
        assert_eq!(denied_value(&reg), 0);
        // Authorization header must remain untouched on the no-rule path.
        assert!(
            r.headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("authorization")),
            "authorization untouched when no rule matches"
        );
    }

    // ---- 7. forward=true keeps Authorization ----

    #[test]
    fn forward_true_keeps_authorization() {
        let (kp, jwks) = keypair();
        let reg = registry();
        let cfg = build_cfg(&jwks, ISS, vec![], true, "/secure");
        let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();
        let tok = make_token(&kp, r#"{"iss":"testing@secure.istio.io","exp":4102444800}"#);
        let mut r = req(vec![host(), auth(&tok)], "/secure/x");
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(allowed_value(&reg), 1);
        assert!(
            r.headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("authorization")),
            "authorization preserved when forward=true"
        );
    }

    // ---- 8. non-Bearer Authorization ----

    #[test]
    fn non_bearer_authorization_treated_as_missing() {
        let (_kp, jwks) = keypair();
        let reg = registry();
        let cfg = build_cfg(&jwks, ISS, vec![], false, "/");
        let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();
        let mut r = req(
            vec![
                host(),
                ("authorization".to_string(), "Basic xxx".to_string()),
            ],
            "/",
        );
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 401);
                assert_eq!(resp.body.as_ref(), b"Jwt is missing");
                // non-Bearer is treated as missing — no error= in www-authenticate.
                let wa = resp
                    .headers
                    .iter()
                    .find(|(k, _)| k == "www-authenticate")
                    .expect("www-authenticate present");
                assert_eq!(wa.1, r#"Bearer realm="http://envoy.test/""#);
            }
            Decision::Continue => panic!("expected StopAndSend"),
        }
        assert_eq!(denied_value(&reg), 1);
    }

    // ---- encode_headers is a no-op ----

    #[test]
    fn encode_headers_is_noop() {
        let (_kp, jwks) = keypair();
        let reg = registry();
        let cfg = build_cfg(&jwks, ISS, vec![], false, "/");
        let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();
        let mut resp = FilterResponse::test_200();
        assert!(matches!(f.encode_headers(&mut resp), Decision::Continue));
    }

    // ---- build rejects unknown provider reference ----

    #[test]
    fn build_rejects_unknown_provider() {
        let (_kp, jwks) = keypair();
        let reg = registry();
        let mut cfg = build_cfg(&jwks, ISS, vec![], false, "/");
        cfg.rules[0].requires.provider_name = "nope".to_string();
        let err = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap_err();
        assert!(matches!(err, FilterError::InvalidConfig { .. }));
    }

    // ---- header matcher on a rule ----

    /// Exercises the `m.headers.iter().all(|hm| hm.matches(headers))` branch
    /// in `route_match_matches`. The rule requires `x-require: yes` (ExactMatch).
    /// • Request WITH the header + valid token  → Continue (allowed).
    /// • Request WITHOUT the header (no token)  → rule does NOT match → no-rule
    ///   allow path → Continue + allowed incremented, never denied.
    #[test]
    fn header_matcher_gates_rule_match() {
        use envoy_config::{HeaderMatcher, HeaderMatcherMode};

        let (kp, jwks) = keypair();
        let reg = registry();

        // Build a config whose single rule has a header matcher in addition to
        // the prefix match on "/".
        let mut providers = std::collections::BTreeMap::new();
        providers.insert(
            "prov".to_string(),
            envoy_config::JwtProvider {
                issuer: ISS.to_string(),
                audiences: vec![],
                local_jwks: envoy_config::DataSource {
                    filename: None,
                    inline_string: Some(jwks.clone()),
                },
                forward: false,
            },
        );
        let cfg = envoy_config::JwtAuthnConfig {
            providers,
            rules: vec![envoy_config::RequirementRule {
                r#match: envoy_config::RouteMatch {
                    prefix: Some("/".to_string()),
                    path: None,
                    headers: vec![HeaderMatcher {
                        name: "x-require".to_string(),
                        mode: HeaderMatcherMode::ExactMatch("yes".to_string()),
                        invert_match: false,
                    }],
                },
                requires: envoy_config::JwtRequirement {
                    provider_name: "prov".to_string(),
                },
            }],
        };

        let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();

        // --- sub-case A: header present + valid token → matched rule → Continue ---
        let tok = make_token(&kp, r#"{"iss":"testing@secure.istio.io","exp":4102444800}"#);
        let mut r = req(
            vec![
                host(),
                ("x-require".to_string(), "yes".to_string()),
                auth(&tok),
            ],
            "/api",
        );
        assert!(
            matches!(f.decode_headers(&mut r), Decision::Continue),
            "matched rule with valid token should Continue"
        );
        assert_eq!(allowed_value(&reg), 1);
        assert_eq!(denied_value(&reg), 0);

        // --- sub-case B: header absent (no token) → rule does NOT match → no-rule allow ---
        let mut r2 = req(vec![host()], "/api");
        assert!(
            matches!(f.decode_headers(&mut r2), Decision::Continue),
            "unmatched rule (no header) should Continue without JWT check"
        );
        assert_eq!(allowed_value(&reg), 2);
        assert_eq!(denied_value(&reg), 0);
    }

    /// Phase 75.1 (ADR-0159): the MODE-SCOPED absence rule propagates into
    /// JWT-authn requirement-rule matching — call site 4 of 5. Observable
    /// without minting a token: a rule whose header matcher does NOT match is
    /// skipped, so a TOKENLESS request is allowed; a rule that DOES match
    /// demands a token, so a tokenless request is denied.
    ///
    /// This mirrors the observable of the neighbouring
    /// `header_matcher_gates_rule_match`, but with a FRESH registry per
    /// invocation so the verdict is a clean `denied == 1` rather than a
    /// cumulative count.
    #[test]
    fn jwt_rule_header_matcher_absence_rule_is_mode_scoped() {
        use envoy_config::{HeaderMatcher, HeaderMatcherMode};

        let (_kp, jwks) = keypair();

        // Returns true iff the RULE MATCHED (a tokenless request got denied).
        let rule_matched =
            |mode: HeaderMatcherMode, invert: bool, headers: Vec<(String, String)>| {
                let reg = registry();
                let mut providers = std::collections::BTreeMap::new();
                providers.insert(
                    "prov".to_string(),
                    envoy_config::JwtProvider {
                        issuer: ISS.to_string(),
                        audiences: vec![],
                        local_jwks: envoy_config::DataSource {
                            filename: None,
                            inline_string: Some(jwks.clone()),
                        },
                        forward: false,
                    },
                );
                let cfg = envoy_config::JwtAuthnConfig {
                    providers,
                    rules: vec![envoy_config::RequirementRule {
                        r#match: envoy_config::RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![HeaderMatcher {
                                name: "x-a".to_string(),
                                mode,
                                invert_match: invert,
                            }],
                        },
                        requires: envoy_config::JwtRequirement {
                            provider_name: "prov".to_string(),
                        },
                    }],
                };
                let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();
                let mut hs = vec![host()];
                hs.extend(headers);
                let mut r = req(hs, "/api");
                let _ = f.decode_headers(&mut r);
                denied_value(&reg) == 1
            };

        let present = vec![("x-a".to_string(), "zzz".to_string())];

        // D1: value matcher + invert + ABSENT → the rule no longer matches.
        assert!(
            rule_matched(
                HeaderMatcherMode::ExactMatch("v".into()),
                true,
                present.clone()
            ),
            "value+invert, present non-matching → rule matches → tokenless denied"
        );
        assert!(
            !rule_matched(HeaderMatcherMode::ExactMatch("v".into()), true, vec![]),
            "value+invert, ABSENT → rule must NOT match (D1 / CF-72-1 closed)"
        );

        // D2: plain `present_match: false` requires ABSENCE.
        assert!(
            !rule_matched(
                HeaderMatcherMode::PresentMatch(false),
                false,
                present.clone()
            ),
            "present_match:false, PRESENT → rule must NOT match (D2)"
        );
        assert!(
            rule_matched(HeaderMatcherMode::PresentMatch(false), false, vec![]),
            "present_match:false, ABSENT → rule matches"
        );

        // P1 THE GUARD.
        assert!(
            rule_matched(HeaderMatcherMode::PresentMatch(true), true, vec![]),
            "present_match:true+invert, ABSENT → rule STILL matches (P1 parity)"
        );
        assert!(
            !rule_matched(HeaderMatcherMode::PresentMatch(true), true, present),
            "present_match:true+invert, PRESENT → rule does not match"
        );
    }
}
