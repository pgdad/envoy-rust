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
    /// Phase 35: dynamic-metadata condition. Holds the config matcher directly
    /// (the `Header(HeaderMatcher)` precedent). Reads a single-segment metadata
    /// path; absent namespace/key → no match.
    Metadata(envoy_config::MetadataMatcher),
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
    /// Phase 35: dynamic-metadata condition. Holds the config matcher directly
    /// (the `Header(HeaderMatcher)` precedent). Reads a single-segment metadata
    /// path; absent namespace/key → no match.
    Metadata(envoy_config::MetadataMatcher),
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
        RuntimePermission::Metadata(m) => eval_metadata(m, req),
    }
}

/// Phase 35/36: resolve the single-segment metadata path and apply the ValueMatcher.
/// §A1: routed through `matches_resolved` so `present_match` observes KEY PRESENCE
/// (`match = present && want`); `string_match` keeps present-AND-value-matches.
/// The validator guarantees `path.len() == 1`, so `path[0]` is safe.
fn eval_metadata(m: &envoy_config::MetadataMatcher, req: &FilterRequest) -> bool {
    let resolved = req
        .dynamic_metadata
        .get(&m.filter)
        .and_then(|ns| ns.get(&m.path[0].key))
        .map(String::as_str);
    m.value.matches_resolved(resolved)
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
        RuntimePrincipal::Metadata(m) => eval_metadata(m, req),
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
    /// Also returns `FilterError::InvalidConfig` if any `safe_regex` pattern
    /// in an RBAC `header`/`metadata` matcher is malformed — compiled here at
    /// lowering time so a bad pattern is boot-fatal rather than a
    /// first-request panic (closes carry-forward M35-1).
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
            .map(|(name, policy)| -> Result<RuntimePolicy, FilterError> {
                Ok(RuntimePolicy {
                    name: name.clone(),
                    permissions: policy
                        .permissions
                        .iter()
                        .map(lower_permission)
                        .collect::<Result<_, _>>()?,
                    principals: policy
                        .principals
                        .iter()
                        .map(lower_principal)
                        .collect::<Result<_, _>>()?,
                })
            })
            .collect::<Result<_, _>>()?;
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
/// payload per PLAN lock-in #6. Now fallible: `Header` and `Metadata` arms
/// compile any `SafeRegex` on the owned clone (phase 36 M35-1 fix, §A4).
fn lower_permission(p: &envoy_config::Permission) -> Result<RuntimePermission, FilterError> {
    Ok(match p {
        envoy_config::Permission::Any(b) => RuntimePermission::Any(*b),
        envoy_config::Permission::Header(m) => {
            let mut m = m.clone();
            m.compile_safe_regexes()
                .map_err(|e| FilterError::InvalidConfig {
                    message: e.to_string(),
                })?;
            RuntimePermission::Header(m)
        }
        envoy_config::Permission::AndRules(set) => RuntimePermission::AndRules(
            set.rules
                .iter()
                .map(lower_permission)
                .collect::<Result<_, _>>()?,
        ),
        envoy_config::Permission::OrRules(set) => RuntimePermission::OrRules(
            set.rules
                .iter()
                .map(lower_permission)
                .collect::<Result<_, _>>()?,
        ),
        envoy_config::Permission::NotRule(inner) => {
            RuntimePermission::NotRule(Box::new(lower_permission(inner)?))
        }
        envoy_config::Permission::Metadata(m) => {
            let mut m = m.clone();
            m.value
                .compile_safe_regexes()
                .map_err(|e| FilterError::InvalidConfig {
                    message: e.to_string(),
                })?;
            RuntimePermission::Metadata(m)
        }
    })
}

/// Recursive lowering of wire-form `envoy_config::Principal` → runtime
/// `RuntimePrincipal`. Symmetric to `lower_permission` per PLAN lock-in #7;
/// `PrincipalSet { ids }` wrapper flattened on `AndIds`/`OrIds`. Now fallible:
/// `Header` and `Metadata` arms compile any `SafeRegex` on the owned clone
/// (phase 36 M35-1 fix, §A4).
fn lower_principal(p: &envoy_config::Principal) -> Result<RuntimePrincipal, FilterError> {
    Ok(match p {
        envoy_config::Principal::Any(b) => RuntimePrincipal::Any(*b),
        envoy_config::Principal::Header(m) => {
            let mut m = m.clone();
            m.compile_safe_regexes()
                .map_err(|e| FilterError::InvalidConfig {
                    message: e.to_string(),
                })?;
            RuntimePrincipal::Header(m)
        }
        envoy_config::Principal::AndIds(set) => RuntimePrincipal::AndIds(
            set.ids
                .iter()
                .map(lower_principal)
                .collect::<Result<_, _>>()?,
        ),
        envoy_config::Principal::OrIds(set) => RuntimePrincipal::OrIds(
            set.ids
                .iter()
                .map(lower_principal)
                .collect::<Result<_, _>>()?,
        ),
        envoy_config::Principal::NotId(inner) => {
            RuntimePrincipal::NotId(Box::new(lower_principal(inner)?))
        }
        envoy_config::Principal::Metadata(m) => {
            let mut m = m.clone();
            m.value
                .compile_safe_regexes()
                .map_err(|e| FilterError::InvalidConfig {
                    message: e.to_string(),
                })?;
            RuntimePrincipal::Metadata(m)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{
        HeaderMatcher, HeaderMatcherMode, MetadataMatcher, MetadataPathSegment, StringMatcher,
        StringMatcherMode, ValueMatcher,
    };

    fn req_with(headers: Vec<(&'static str, &'static str)>) -> FilterRequest {
        FilterRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: headers
                .into_iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body: None,
            dynamic_metadata: std::collections::BTreeMap::new(),
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

    fn metadata_matcher(filter: &str, key: &str, exact: &str) -> MetadataMatcher {
        MetadataMatcher {
            filter: filter.to_string(),
            path: vec![MetadataPathSegment {
                key: key.to_string(),
            }],
            value: ValueMatcher::StringMatch(StringMatcher {
                mode: StringMatcherMode::Exact(exact.to_string()),
                ignore_case: false,
            }),
        }
    }

    // a req carrying dynamic_metadata[ns][key] = val
    fn req_with_md(ns: &str, key: &str, val: &str) -> FilterRequest {
        let mut req = req_with(vec![]);
        let mut inner = std::collections::BTreeMap::new();
        inner.insert(key.to_string(), val.to_string());
        req.dynamic_metadata.insert(ns.to_string(), inner);
        req
    }

    #[test]
    fn metadata_permission_matches_present_value() {
        let req = req_with_md("envoy.filters.http.header_to_metadata", "tier", "prod");
        let perm = RuntimePermission::Metadata(metadata_matcher(
            "envoy.filters.http.header_to_metadata",
            "tier",
            "prod",
        ));
        assert!(eval_permission(&perm, &req));
    }

    #[test]
    fn metadata_permission_no_match_on_value_mismatch() {
        // tier=dev present but matcher wants exact "prod" → false
        let req = req_with_md("envoy.filters.http.header_to_metadata", "tier", "dev");
        let perm = RuntimePermission::Metadata(metadata_matcher(
            "envoy.filters.http.header_to_metadata",
            "tier",
            "prod",
        ));
        assert!(!eval_permission(&perm, &req));
    }

    #[test]
    fn metadata_permission_no_match_on_absent_namespace() {
        // req has a DIFFERENT namespace → false
        let req = req_with_md("some.other.ns", "tier", "prod");
        let perm = RuntimePermission::Metadata(metadata_matcher(
            "envoy.filters.http.header_to_metadata",
            "tier",
            "prod",
        ));
        assert!(!eval_permission(&perm, &req));
    }

    #[test]
    fn metadata_permission_no_match_on_absent_key() {
        // namespace present, but a different key → false
        let req = req_with_md("envoy.filters.http.header_to_metadata", "other_key", "prod");
        let perm = RuntimePermission::Metadata(metadata_matcher(
            "envoy.filters.http.header_to_metadata",
            "tier",
            "prod",
        ));
        assert!(!eval_permission(&perm, &req));
    }

    #[test]
    fn metadata_principal_mirrors_permission() {
        // RuntimePrincipal::Metadata, same present-value match
        let req = req_with_md("envoy.filters.http.header_to_metadata", "tier", "prod");
        let prin = RuntimePrincipal::Metadata(metadata_matcher(
            "envoy.filters.http.header_to_metadata",
            "tier",
            "prod",
        ));
        assert!(eval_principal(&prin, &req));
    }

    fn present_matcher(filter: &str, key: &str, want: bool) -> MetadataMatcher {
        MetadataMatcher {
            filter: filter.into(),
            path: vec![MetadataPathSegment { key: key.into() }],
            value: ValueMatcher::PresentMatch(want),
        }
    }

    #[test]
    fn metadata_present_match_true_matches_present_key() {
        let ns = "envoy.filters.http.header_to_metadata";
        let req = req_with_md(ns, "tier", "staging"); // any value
        assert!(eval_permission(
            &RuntimePermission::Metadata(present_matcher(ns, "tier", true)),
            &req
        ));
    }

    #[test]
    fn metadata_present_match_true_no_match_when_absent() {
        let ns = "envoy.filters.http.header_to_metadata";
        let req = req_with_md(ns, "other", "x"); // key tier absent
        assert!(!eval_permission(
            &RuntimePermission::Metadata(present_matcher(ns, "tier", true)),
            &req
        ));
    }

    #[test]
    fn metadata_present_match_false_never_matches() {
        // §A1: present_match:false → present && false → never matches, even when present.
        let ns = "envoy.filters.http.header_to_metadata";
        let present = req_with_md(ns, "tier", "staging");
        let absent = req_with(vec![]);
        assert!(!eval_permission(
            &RuntimePermission::Metadata(present_matcher(ns, "tier", false)),
            &present
        ));
        assert!(!eval_permission(
            &RuntimePermission::Metadata(present_matcher(ns, "tier", false)),
            &absent
        ));
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

    // --- Task 4 (T4): safe_regex compile-at-lower regression guard (M35-1) ---

    fn safe_regex_string_matcher(pattern: &str) -> StringMatcher {
        StringMatcher {
            mode: StringMatcherMode::SafeRegex(envoy_config::SafeRegex {
                regex: pattern.into(),
                compiled: None,
            }),
            ignore_case: false,
        }
    }

    #[test]
    fn metadata_safe_regex_value_matches_without_panic() {
        // §A3: ANCHORED ^(prod|staging)$ → staging matches, dev misses. No panic (M35-1 closed).
        let ns = "envoy.filters.http.header_to_metadata";
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let mut policies = std::collections::BTreeMap::new();
        policies.insert(
            "p".into(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Metadata(MetadataMatcher {
                    filter: ns.into(),
                    path: vec![MetadataPathSegment { key: "tier".into() }],
                    value: ValueMatcher::StringMatch(safe_regex_string_matcher("^(prod|staging)$")),
                })],
                principals: vec![envoy_config::Principal::Any(true)],
            },
        );
        let cfg = envoy_config::RbacConfig {
            rules: envoy_config::Rules {
                action: envoy_config::Action::Allow,
                policies,
            },
        };
        let mut f = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").expect("builds");
        let mut ok = req_with_md(ns, "tier", "staging");
        assert!(matches!(
            f.decode_headers(&mut ok),
            crate::pipeline::Decision::Continue
        ));
        let mut miss = req_with_md(ns, "tier", "dev");
        match f.decode_headers(&mut miss) {
            crate::pipeline::Decision::StopAndSend(r) => assert_eq!(r.status, 403),
            other => panic!("expected 403, got {other:?}"),
        }
    }

    #[test]
    fn header_safe_regex_matches_without_panic() {
        // PANIC-REGRESSION GUARD (M35-1)
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let mut policies = std::collections::BTreeMap::new();
        policies.insert(
            "p".into(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Header(HeaderMatcher {
                    name: "x-tier".into(),
                    mode: HeaderMatcherMode::SafeRegexMatch(envoy_config::SafeRegex {
                        regex: "^(prod|staging)$".into(),
                        compiled: None,
                    }),
                    invert_match: false,
                })],
                principals: vec![envoy_config::Principal::Any(true)],
            },
        );
        let cfg = envoy_config::RbacConfig {
            rules: envoy_config::Rules {
                action: envoy_config::Action::Allow,
                policies,
            },
        };
        let mut f = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").expect("builds");
        let mut ok = req_with(vec![("x-tier", "staging")]);
        assert!(matches!(
            f.decode_headers(&mut ok),
            crate::pipeline::Decision::Continue
        ));
        let mut miss = req_with(vec![("x-tier", "dev")]);
        assert!(matches!(
            f.decode_headers(&mut miss),
            crate::pipeline::Decision::StopAndSend(_)
        ));
    }

    #[test]
    fn malformed_rbac_safe_regex_is_boot_fatal_not_panic() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let mut policies = std::collections::BTreeMap::new();
        policies.insert(
            "p".into(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Header(HeaderMatcher {
                    name: "x".into(),
                    mode: HeaderMatcherMode::SafeRegexMatch(envoy_config::SafeRegex {
                        regex: "(".into(),
                        compiled: None,
                    }),
                    invert_match: false,
                })],
                principals: vec![envoy_config::Principal::Any(true)],
            },
        );
        let cfg = envoy_config::RbacConfig {
            rules: envoy_config::Rules {
                action: envoy_config::Action::Allow,
                policies,
            },
        };
        assert!(RbacFilter::build_from_config(&cfg, &registry, "ingress_http").is_err());
    }

    // --- Task 4: in-process producer→consumer backstop ----------------------
    //
    // These prove, IN-PROCESS through the real (non-test-util) pipeline, the
    // load-bearing mechanism that the cross-proxy fixture (Task 5) cannot show
    // deterministically: a real `header_to_metadata` PRODUCER writes
    // `dynamic_metadata` that the real `rbac` CONSUMER reads in the SAME decode
    // pass. Built via the non-gated `build_from_config` paths so they run under
    // plain `cargo test --workspace` (NOT behind `test-util`).

    // (A) mid-chain: [header_to_metadata, rbac] driven by the real FilterPipeline.
    fn h2m_then_rbac_pipeline(action: envoy_config::Action) -> crate::pipeline::FilterPipeline {
        use envoy_stats::StatsRegistry;
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let registry = Arc::new(StatsRegistry::new());
        // producer: x-tier -> envoy.filters.http.header_to_metadata:tier
        let hf_h2m = envoy_config::HttpFilter {
            name: "envoy.filters.http.header_to_metadata".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::HeaderToMetadata(
                envoy_config::HeaderToMetadataConfig {
                    request_rules: vec![envoy_config::HeaderToMetadataRule {
                        header: "x-tier".to_string(),
                        on_header_present: Some(envoy_config::HeaderToMetadataKeyValue {
                            metadata_namespace: "envoy.filters.http.header_to_metadata".to_string(),
                            key: "tier".to_string(),
                            value: None,
                            r#type: envoy_config::HeaderToMetadataType::String,
                        }),
                        on_header_missing: None,
                    }],
                },
            ),
        };
        // consumer: ALLOW/DENY policy whose Permission is metadata tier==prod.
        let mut policies = BTreeMap::new();
        policies.insert(
            "tier_prod".to_string(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Metadata(metadata_matcher(
                    "envoy.filters.http.header_to_metadata",
                    "tier",
                    "prod",
                ))],
                principals: vec![envoy_config::Principal::Any(true)],
            },
        );
        let hf_rbac = envoy_config::HttpFilter {
            name: "envoy.filters.http.rbac".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Rbac(envoy_config::RbacConfig {
                rules: envoy_config::Rules { action, policies },
            }),
        };
        crate::pipeline::FilterPipeline::build_from_config(
            &[hf_h2m, hf_rbac],
            &registry,
            "ingress_http",
        )
        .expect("pipeline builds")
    }

    #[test]
    fn mid_chain_producer_then_consumer_allows_prod() {
        // The consumer reads the producer's mid-pass write: x-tier:prod →
        // metadata tier==prod → ALLOW policy matches → Continue.
        let mut pipeline = h2m_then_rbac_pipeline(envoy_config::Action::Allow);
        let mut req = req_with(vec![("x-tier", "prod")]);
        assert!(matches!(
            pipeline.decode_headers(&mut req),
            crate::pipeline::Decision::Continue
        ));
    }

    #[test]
    fn mid_chain_producer_then_consumer_denies_dev() {
        // x-tier:dev → metadata tier==dev → ALLOW policy (wants prod) no match → 403.
        let mut pipeline = h2m_then_rbac_pipeline(envoy_config::Action::Allow);
        let mut req = req_with(vec![("x-tier", "dev")]);
        match pipeline.decode_headers(&mut req) {
            crate::pipeline::Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 403);
                assert_eq!(&resp.body[..], b"RBAC: access denied");
            }
            other => panic!("expected StopAndSend(403), got {other:?}"),
        }
    }

    #[test]
    fn mid_chain_absent_header_denies() {
        // No x-tier → header_to_metadata writes nothing → key unset → no match → 403.
        let mut pipeline = h2m_then_rbac_pipeline(envoy_config::Action::Allow);
        let mut req = req_with(vec![]);
        match pipeline.decode_headers(&mut req) {
            crate::pipeline::Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 403);
                assert_eq!(&resp.body[..], b"RBAC: access denied");
            }
            other => panic!("expected StopAndSend(403), got {other:?}"),
        }
    }

    // (B) composition / Principal / DENY-inversion — standalone RbacFilter with
    // metadata injected directly (no producer needed).

    #[test]
    fn metadata_composes_in_and_rules() {
        use envoy_stats::StatsRegistry;
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let ns = "envoy.filters.http.header_to_metadata";
        let registry = Arc::new(StatsRegistry::new());
        let mut policies = BTreeMap::new();
        policies.insert(
            "p".to_string(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::AndRules(
                    envoy_config::PermissionSet {
                        rules: vec![
                            envoy_config::Permission::Metadata(metadata_matcher(
                                ns, "tier", "prod",
                            )),
                            envoy_config::Permission::Any(true),
                        ],
                    },
                )],
                principals: vec![envoy_config::Principal::Any(true)],
            },
        );
        let cfg = envoy_config::RbacConfig {
            rules: envoy_config::Rules {
                action: envoy_config::Action::Allow,
                policies,
            },
        };
        let mut filter = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").unwrap();

        // tier==prod → and_rules all-true → ALLOW → Continue.
        let mut req_prod = req_with_md(ns, "tier", "prod");
        assert!(matches!(
            filter.decode_headers(&mut req_prod),
            crate::pipeline::Decision::Continue
        ));

        // tier==dev → metadata child fails → and_rules fails → no match → 403.
        let mut req_dev = req_with_md(ns, "tier", "dev");
        match filter.decode_headers(&mut req_dev) {
            crate::pipeline::Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 403);
                assert_eq!(&resp.body[..], b"RBAC: access denied");
            }
            other => panic!("expected StopAndSend(403), got {other:?}"),
        }
    }

    #[test]
    fn metadata_principal_and_deny_inversion() {
        use envoy_stats::StatsRegistry;
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let ns = "envoy.filters.http.header_to_metadata";
        let registry = Arc::new(StatsRegistry::new());
        let mut policies = BTreeMap::new();
        policies.insert(
            "deny_prod".to_string(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Any(true)],
                principals: vec![envoy_config::Principal::Metadata(metadata_matcher(
                    ns, "tier", "prod",
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

        // tier==prod → Principal::Metadata matches → DENY action match → 403.
        let mut req_prod = req_with_md(ns, "tier", "prod");
        match filter.decode_headers(&mut req_prod) {
            crate::pipeline::Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 403);
                assert_eq!(&resp.body[..], b"RBAC: access denied");
            }
            other => panic!("expected StopAndSend(403), got {other:?}"),
        }

        // tier==dev → Principal::Metadata no match → DENY action no_match → Continue.
        let mut req_dev = req_with_md(ns, "tier", "dev");
        assert!(matches!(
            filter.decode_headers(&mut req_dev),
            crate::pipeline::Decision::Continue
        ));
    }
}
