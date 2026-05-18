//! `envoy.filters.http.rbac` runtime filter (phase 10).
//!
//! Hand-rolled per D-3.2's "Every individual filter ... Must be written from
//! scratch" doctrine + the 07.2 `header_mutation.rs` + 09 `local_rate_limit.rs`
//! precedent. Permission/Principal tree-walk evaluator + RbacFilter runtime.

use std::sync::Arc;

use bytes::Bytes;
use envoy_config::HeaderMatcher;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// Build-time-lowered runtime representation of an Envoy RBAC `Permission`.
///
/// Mirrors the upstream `envoy.config.rbac.v3.Permission` discriminated union
/// at the runtime layer, flattened per PLAN lock-in #6: the wire-format
/// `PermissionSet { rules: Vec<Permission> }` wrapper is collapsed into a
/// direct `Vec<RuntimePermission>` payload on the `AndRules` / `OrRules`
/// variants. The `Box` indirection appears only on `NotRule` (single-child
/// negation); `AndRules` / `OrRules` already hold their children behind the
/// `Vec`'s allocation so no per-variant `Box` is needed.
#[derive(Debug)]
pub(crate) enum RuntimePermission {
    /// Constant truth value. Wire-form `{ any: true }` / `{ any: false }`.
    Any(bool),
    /// Per-header predicate; delegates to `HeaderMatcher::matches`.
    Header(HeaderMatcher),
    /// Conjunction: matches iff every child rule matches. Short-circuits on
    /// first `false` via `Iterator::all`.
    AndRules(Vec<RuntimePermission>),
    /// Disjunction: matches iff any child rule matches. Short-circuits on
    /// first `true` via `Iterator::any`.
    OrRules(Vec<RuntimePermission>),
    /// Negation of a single inner rule.
    NotRule(Box<RuntimePermission>),
}

/// Build-time-lowered runtime representation of an Envoy RBAC `Principal`.
///
/// Structurally symmetric to `RuntimePermission` per PLAN lock-in #7. The
/// wire-format `PrincipalSet { ids: Vec<Principal> }` wrapper is flattened
/// into a direct `Vec<RuntimePrincipal>` on `AndIds` / `OrIds`; `Box` appears
/// only on `NotId`.
#[derive(Debug)]
pub(crate) enum RuntimePrincipal {
    /// Constant truth value. Wire-form `{ any: true }` / `{ any: false }`.
    Any(bool),
    /// Per-header predicate; delegates to `HeaderMatcher::matches`.
    Header(HeaderMatcher),
    /// Conjunction: matches iff every child id matches.
    AndIds(Vec<RuntimePrincipal>),
    /// Disjunction: matches iff any child id matches.
    OrIds(Vec<RuntimePrincipal>),
    /// Negation of a single inner id.
    NotId(Box<RuntimePrincipal>),
}

/// Recursive tree-walk evaluator for `RuntimePermission`. Synchronous,
/// pure-compute, no I/O. Returns `true` iff the permission tree matches the
/// request. Short-circuits via `Iterator::all` (AndRules) and `Iterator::any`
/// (OrRules); `NotRule` negates its inner result. Per PLAN lock-ins #8 + #9.
pub(crate) fn eval_permission(p: &RuntimePermission, req: &FilterRequest) -> bool {
    match p {
        RuntimePermission::Any(b) => *b,
        RuntimePermission::Header(m) => m.matches(&req.headers),
        RuntimePermission::AndRules(set) => set.iter().all(|p| eval_permission(p, req)),
        RuntimePermission::OrRules(set) => set.iter().any(|p| eval_permission(p, req)),
        RuntimePermission::NotRule(inner) => !eval_permission(inner, req),
    }
}

/// Recursive tree-walk evaluator for `RuntimePrincipal`. Structurally
/// symmetric to `eval_permission` per PLAN lock-in #7. Short-circuits via
/// `Iterator::all` (AndIds) and `Iterator::any` (OrIds); `NotId` negates.
pub(crate) fn eval_principal(p: &RuntimePrincipal, req: &FilterRequest) -> bool {
    match p {
        RuntimePrincipal::Any(b) => *b,
        RuntimePrincipal::Header(m) => m.matches(&req.headers),
        RuntimePrincipal::AndIds(set) => set.iter().all(|p| eval_principal(p, req)),
        RuntimePrincipal::OrIds(set) => set.iter().any(|p| eval_principal(p, req)),
        RuntimePrincipal::NotId(inner) => !eval_principal(inner, req),
    }
}

/// The `envoy.filters.http.rbac` runtime filter (phase 10).
///
/// Decode-only filter per SPEC §5.4: evaluates every inbound request against
/// the lowered `RuntimePolicy` list and either allows it through
/// (`Decision::Continue`) or short-circuits with a 403 response
/// (`Decision::StopAndSend`). Encode-side (`encode_headers`) is a no-op.
/// Two stat counters (`allowed` + `denied`) are wired at construction time
/// and incremented synchronously at the decision site in `decode_headers`.
#[derive(Debug, Clone)]
pub struct RbacFilter {
    action: RuntimeAction,
    policies: Arc<Vec<RuntimePolicy>>,
    allowed_counter: Arc<Counter>,
    denied_counter: Arc<Counter>,
}

/// Wire-form action: determines whether a policy _match_ means ALLOW or DENY.
#[derive(Debug, Clone, Copy)]
enum RuntimeAction {
    Allow,
    Deny,
}

/// Build-time-lowered runtime policy: a named (permission × principal) pair.
#[derive(Debug)]
struct RuntimePolicy {
    #[allow(dead_code)] // retained for future tracing::debug! diagnostics
    name: String,
    permissions: Vec<RuntimePermission>,
    principals: Vec<RuntimePrincipal>,
}

impl RbacFilter {
    /// Lower an `envoy_config::RbacConfig` into the runtime filter and register
    /// the 2 stat counters against the `StatsRegistry` under
    /// `http.{hcm_stat_prefix}.rbac.{allowed,denied}`. Returns
    /// `FilterError::InvalidConfig` if the registry rejects a counter name
    /// (defense-in-depth; the envoy-config validator is the primary gate).
    pub(crate) fn build_from_config(
        cfg: &envoy_config::RbacConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        let action = match cfg.rules.action {
            envoy_config::Action::Allow => RuntimeAction::Allow,
            envoy_config::Action::Deny => RuntimeAction::Deny,
        };
        let policies: Vec<RuntimePolicy> = cfg
            .rules
            .policies
            .iter()
            .map(|(name, policy)| RuntimePolicy {
                name: name.clone(),
                permissions: policy.permissions.iter().map(lower_permission).collect(),
                principals: policy.principals.iter().map(lower_principal).collect(),
            })
            .collect();
        // Reuse FilterError::InvalidConfig per local_rate_limit.rs precedent.
        let allowed_counter = registry
            .register_counter(&format!("http.{hcm_stat_prefix}.rbac.allowed"))
            .map_err(|e| FilterError::InvalidConfig {
                message: format!("StatsRegistry: {e}"),
            })?;
        let denied_counter = registry
            .register_counter(&format!("http.{hcm_stat_prefix}.rbac.denied"))
            .map_err(|e| FilterError::InvalidConfig {
                message: format!("StatsRegistry: {e}"),
            })?;
        Ok(Self {
            action,
            policies: Arc::new(policies),
            allowed_counter,
            denied_counter,
        })
    }

    /// Evaluate the RBAC policy against the incoming request headers.
    ///
    /// Per SPEC §5.6 decision matrix: iterates policies in `BTreeMap`
    /// alphabetical order; short-circuits on the first matching policy
    /// (both permission AND principal must match). The `(action, match)`
    /// combination determines the outcome:
    /// - `(Allow, true)` or `(Deny, false)` → `Decision::Continue` + increment `allowed`.
    /// - `(Allow, false)` or `(Deny, true)` → `Decision::StopAndSend(403)` + increment `denied`.
    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        let any_policy_matches = self.policies.iter().any(|p| {
            let perm_match = p.permissions.iter().any(|x| eval_permission(x, req));
            let prin_match = p.principals.iter().any(|x| eval_principal(x, req));
            perm_match && prin_match
        });
        let allow = matches!(
            (self.action, any_policy_matches),
            (RuntimeAction::Allow, true) | (RuntimeAction::Deny, false)
        );
        if allow {
            self.allowed_counter.inc();
            Decision::Continue
        } else {
            self.denied_counter.inc();
            Decision::StopAndSend(FilterResponse {
                status: 403,
                // reason is Option<&'static str> per crate::types::FilterResponse.
                reason: Some("Forbidden"),
                headers: vec![],
                // ADR-0034: 19 bytes, no trailing newline, per upstream Envoy v1.33 empirical verification.
                body: Bytes::from_static(b"RBAC: access denied"),
            })
        }
    }

    /// Encode-side no-op per SPEC §5.4 — RBAC operates on requests only.
    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

/// Recursive lowering of wire-form `envoy_config::Permission` → runtime
/// `RuntimePermission`. Flattens the `PermissionSet { rules }` wrapper on
/// `AndRules`/`OrRules` into the runtime enum's direct `Vec<RuntimePermission>`
/// payload per PLAN lock-in #6.
fn lower_permission(p: &envoy_config::Permission) -> RuntimePermission {
    match p {
        envoy_config::Permission::Any(b) => RuntimePermission::Any(*b),
        envoy_config::Permission::Header(m) => RuntimePermission::Header(m.clone()),
        envoy_config::Permission::AndRules(set) => {
            RuntimePermission::AndRules(set.rules.iter().map(lower_permission).collect())
        }
        envoy_config::Permission::OrRules(set) => {
            RuntimePermission::OrRules(set.rules.iter().map(lower_permission).collect())
        }
        envoy_config::Permission::NotRule(inner) => {
            RuntimePermission::NotRule(Box::new(lower_permission(inner)))
        }
    }
}

/// Recursive lowering of wire-form `envoy_config::Principal` → runtime
/// `RuntimePrincipal`. Symmetric to `lower_permission` per PLAN lock-in #7;
/// `PrincipalSet { ids }` wrapper flattened on `AndIds`/`OrIds`.
fn lower_principal(p: &envoy_config::Principal) -> RuntimePrincipal {
    match p {
        envoy_config::Principal::Any(b) => RuntimePrincipal::Any(*b),
        envoy_config::Principal::Header(m) => RuntimePrincipal::Header(m.clone()),
        envoy_config::Principal::AndIds(set) => {
            RuntimePrincipal::AndIds(set.ids.iter().map(lower_principal).collect())
        }
        envoy_config::Principal::OrIds(set) => {
            RuntimePrincipal::OrIds(set.ids.iter().map(lower_principal).collect())
        }
        envoy_config::Principal::NotId(inner) => {
            RuntimePrincipal::NotId(Box::new(lower_principal(inner)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{HeaderMatcher, HeaderMatcherMode, StringMatcher, StringMatcherMode};

    fn req_with(headers: Vec<(&'static str, &'static str)>) -> FilterRequest {
        FilterRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: headers
                .into_iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body: None,
        }
    }

    fn header_matcher_exact(name: &str, exact: &str) -> HeaderMatcher {
        HeaderMatcher {
            name: name.to_string(),
            mode: HeaderMatcherMode::StringMatch(StringMatcher {
                mode: StringMatcherMode::Exact(exact.to_string()),
                ignore_case: false,
            }),
            invert_match: false,
        }
    }

    #[test]
    fn any_true_permission_matches() {
        let req = req_with(vec![]);
        assert!(eval_permission(&RuntimePermission::Any(true), &req));
    }

    #[test]
    fn any_false_permission_does_not_match() {
        let req = req_with(vec![]);
        assert!(!eval_permission(&RuntimePermission::Any(false), &req));
    }

    #[test]
    fn header_permission_matches_when_value_equals() {
        let req = req_with(vec![("x-rbac-pass", "yes")]);
        let perm = RuntimePermission::Header(header_matcher_exact("x-rbac-pass", "yes"));
        assert!(eval_permission(&perm, &req));
    }

    #[test]
    fn header_permission_does_not_match_when_value_differs() {
        let req = req_with(vec![("x-rbac-pass", "no")]);
        let perm = RuntimePermission::Header(header_matcher_exact("x-rbac-pass", "yes"));
        assert!(!eval_permission(&perm, &req));
    }

    #[test]
    fn header_permission_does_not_match_when_header_absent() {
        let req = req_with(vec![("x-other", "yes")]);
        let perm = RuntimePermission::Header(header_matcher_exact("x-rbac-pass", "yes"));
        assert!(!eval_permission(&perm, &req));
    }

    #[test]
    fn and_rules_short_circuits_on_first_false() {
        let req = req_with(vec![]);
        let perm = RuntimePermission::AndRules(vec![
            RuntimePermission::Any(true),
            RuntimePermission::Any(false),
            RuntimePermission::Any(true),
        ]);
        assert!(!eval_permission(&perm, &req));
    }

    #[test]
    fn and_rules_all_true_matches() {
        let req = req_with(vec![]);
        let perm = RuntimePermission::AndRules(vec![
            RuntimePermission::Any(true),
            RuntimePermission::Any(true),
        ]);
        assert!(eval_permission(&perm, &req));
    }

    #[test]
    fn or_rules_short_circuits_on_first_true() {
        let req = req_with(vec![]);
        let perm = RuntimePermission::OrRules(vec![
            RuntimePermission::Any(false),
            RuntimePermission::Any(true),
            RuntimePermission::Any(false),
        ]);
        assert!(eval_permission(&perm, &req));
    }

    #[test]
    fn or_rules_all_false_does_not_match() {
        let req = req_with(vec![]);
        let perm = RuntimePermission::OrRules(vec![
            RuntimePermission::Any(false),
            RuntimePermission::Any(false),
        ]);
        assert!(!eval_permission(&perm, &req));
    }

    #[test]
    fn not_rule_negates_inner() {
        let req = req_with(vec![]);
        let perm_t = RuntimePermission::NotRule(Box::new(RuntimePermission::Any(false)));
        let perm_f = RuntimePermission::NotRule(Box::new(RuntimePermission::Any(true)));
        assert!(eval_permission(&perm_t, &req));
        assert!(!eval_permission(&perm_f, &req));
    }

    #[test]
    fn nested_and_or_not_evaluates_correctly() {
        let req = req_with(vec![("x-a", "1"), ("x-b", "2")]);
        // (header x-a == "1") AND NOT(header x-b == "3")
        let perm = RuntimePermission::AndRules(vec![
            RuntimePermission::Header(header_matcher_exact("x-a", "1")),
            RuntimePermission::NotRule(Box::new(RuntimePermission::Header(header_matcher_exact(
                "x-b", "3",
            )))),
        ]);
        assert!(eval_permission(&perm, &req));
    }

    #[test]
    fn principal_evaluator_mirrors_permission_evaluator() {
        let req = req_with(vec![("x-user", "alice")]);
        let prin = RuntimePrincipal::OrIds(vec![
            RuntimePrincipal::Header(header_matcher_exact("x-user", "bob")),
            RuntimePrincipal::Header(header_matcher_exact("x-user", "alice")),
        ]);
        assert!(eval_principal(&prin, &req));
    }

    // --- Task 3: RbacFilter runtime tests -----------------------------------

    #[test]
    fn build_from_config_allow_with_header_principal_creates_filter() {
        use envoy_stats::StatsRegistry;
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let registry = Arc::new(StatsRegistry::new());
        let mut policies = BTreeMap::new();
        policies.insert(
            "pass".to_string(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Any(true)],
                principals: vec![envoy_config::Principal::Header(header_matcher_exact(
                    "x-rbac-pass",
                    "yes",
                ))],
            },
        );
        let cfg = envoy_config::RbacConfig {
            rules: envoy_config::Rules {
                action: envoy_config::Action::Allow,
                policies,
            },
        };
        let filter =
            RbacFilter::build_from_config(&cfg, &registry, "ingress_http").expect("build succeeds");
        let _ = filter; // ensure construction succeeds
    }

    #[test]
    fn decode_headers_allow_action_no_header_returns_deny() {
        use envoy_stats::StatsRegistry;
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let registry = Arc::new(StatsRegistry::new());
        let mut policies = BTreeMap::new();
        policies.insert(
            "p".to_string(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Any(true)],
                principals: vec![envoy_config::Principal::Header(header_matcher_exact(
                    "x-rbac-pass",
                    "yes",
                ))],
            },
        );
        let cfg = envoy_config::RbacConfig {
            rules: envoy_config::Rules {
                action: envoy_config::Action::Allow,
                policies,
            },
        };
        let mut filter = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").unwrap();
        let mut req = req_with(vec![]);

        match filter.decode_headers(&mut req) {
            crate::pipeline::Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 403);
                assert_eq!(resp.reason, Some("Forbidden"));
                assert!(resp.headers.is_empty());
                assert_eq!(&resp.body[..], b"RBAC: access denied");
            }
            other => panic!("expected StopAndSend(403), got {other:?}"),
        }
    }

    #[test]
    fn decode_headers_allow_action_with_header_returns_continue() {
        use envoy_stats::StatsRegistry;
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let registry = Arc::new(StatsRegistry::new());
        let mut policies = BTreeMap::new();
        policies.insert(
            "p".to_string(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Any(true)],
                principals: vec![envoy_config::Principal::Header(header_matcher_exact(
                    "x-rbac-pass",
                    "yes",
                ))],
            },
        );
        let cfg = envoy_config::RbacConfig {
            rules: envoy_config::Rules {
                action: envoy_config::Action::Allow,
                policies,
            },
        };
        let mut filter = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").unwrap();
        let mut req = req_with(vec![("x-rbac-pass", "yes")]);

        assert!(matches!(
            filter.decode_headers(&mut req),
            crate::pipeline::Decision::Continue
        ));
    }

    #[test]
    fn decode_headers_deny_action_inverts_semantics() {
        use envoy_stats::StatsRegistry;
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let registry = Arc::new(StatsRegistry::new());
        let mut policies = BTreeMap::new();
        policies.insert(
            "block_evil".to_string(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Any(true)],
                principals: vec![envoy_config::Principal::Header(header_matcher_exact(
                    "x-evil", "true",
                ))],
            },
        );
        let cfg = envoy_config::RbacConfig {
            rules: envoy_config::Rules {
                action: envoy_config::Action::Deny,
                policies,
            },
        };
        let mut filter = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").unwrap();

        // No x-evil header → no policy match → Deny action no_match → ALLOW.
        let mut req_benign = req_with(vec![]);
        assert!(matches!(
            filter.decode_headers(&mut req_benign),
            crate::pipeline::Decision::Continue
        ));

        // With x-evil: true → policy match → Deny action match → DENY.
        let mut req_evil = req_with(vec![("x-evil", "true")]);
        match filter.decode_headers(&mut req_evil) {
            crate::pipeline::Decision::StopAndSend(resp) => assert_eq!(resp.status, 403),
            other => panic!("expected StopAndSend(403), got {other:?}"),
        }
    }

    #[test]
    fn decode_headers_counters_increment_correctly() {
        use envoy_stats::StatsRegistry;
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let registry = Arc::new(StatsRegistry::new());
        let mut policies = BTreeMap::new();
        policies.insert(
            "p".to_string(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Any(true)],
                principals: vec![envoy_config::Principal::Header(header_matcher_exact(
                    "x-rbac-pass",
                    "yes",
                ))],
            },
        );
        let cfg = envoy_config::RbacConfig {
            rules: envoy_config::Rules {
                action: envoy_config::Action::Allow,
                policies,
            },
        };
        let mut filter = RbacFilter::build_from_config(&cfg, &registry, "test_prefix").unwrap();

        // 2 allowed + 1 denied
        let mut req_ok = req_with(vec![("x-rbac-pass", "yes")]);
        let _ = filter.decode_headers(&mut req_ok);
        let _ = filter.decode_headers(&mut req_ok);
        let mut req_deny = req_with(vec![]);
        let _ = filter.decode_headers(&mut req_deny);

        // CORRECTION 1: register_counter (not counter); idempotent — returns existing handle.
        let allowed = registry
            .register_counter("http.test_prefix.rbac.allowed")
            .expect("allowed counter registered");
        let denied = registry
            .register_counter("http.test_prefix.rbac.denied")
            .expect("denied counter registered");
        assert_eq!(allowed.value(), 2);
        assert_eq!(denied.value(), 1);
    }

    #[test]
    fn encode_headers_is_noop() {
        use envoy_stats::StatsRegistry;
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let registry = Arc::new(StatsRegistry::new());
        let mut policies = BTreeMap::new();
        policies.insert(
            "p".to_string(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Any(true)],
                principals: vec![envoy_config::Principal::Any(true)],
            },
        );
        let cfg = envoy_config::RbacConfig {
            rules: envoy_config::Rules {
                action: envoy_config::Action::Allow,
                policies,
            },
        };
        let mut filter = RbacFilter::build_from_config(&cfg, &registry, "p").unwrap();
        let mut resp = crate::types::FilterResponse {
            status: 200,
            reason: None,
            headers: vec![],
            body: bytes::Bytes::new(),
        };
        assert!(matches!(
            filter.encode_headers(&mut resp),
            crate::pipeline::Decision::Continue
        ));
    }
}
