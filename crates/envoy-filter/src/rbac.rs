//! `envoy.filters.http.rbac` runtime filter (phase 10).
//!
//! Hand-rolled per D-3.2's "Every individual filter ... Must be written from
//! scratch" doctrine + the 07.2 `header_mutation.rs` + 09 `local_rate_limit.rs`
//! precedent. Permission/Principal tree-walk evaluator + RbacFilter runtime.
//! The evaluator (this task) is pure-compute recursive descent; the filter
//! struct + decode/encode glue lands in Task 3.

use envoy_config::HeaderMatcher;

use crate::types::FilterRequest;

/// Build-time-lowered runtime representation of an Envoy RBAC `Permission`.
///
/// Mirrors the upstream `envoy.config.rbac.v3.Permission` discriminated union
/// at the runtime layer, flattened per PLAN lock-in #6: the wire-format
/// `PermissionSet { rules: Vec<Permission> }` wrapper is collapsed into a
/// direct `Vec<RuntimePermission>` payload on the `AndRules` / `OrRules`
/// variants. The `Box` indirection appears only on `NotRule` (single-child
/// negation); `AndRules` / `OrRules` already hold their children behind the
/// `Vec`'s allocation so no per-variant `Box` is needed.
///
/// `#[allow(dead_code)]` covers the production-profile build for the
/// Tasks 2-3 interim: this enum has no non-test consumer until Task 3 lands
/// `RbacFilter::build_from_config`. Same precedent as
/// `LocalRateLimitFilter::stat_prefix` (`local_rate_limit.rs:49`).
#[allow(dead_code)]
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
///
/// `#[allow(dead_code)]` covers the `Any` / `AndIds` / `NotId` variants which
/// aren't yet constructed at Task 2 (the principal-side tests only exercise
/// `OrIds` + `Header` per the symmetric-evaluator coverage discipline — the
/// permission-side tests exercise all 5 shapes). Task 3's
/// `RbacFilter::build_from_config` will construct all variants; the allow
/// goes away when this lowering exists. Same precedent as
/// `LocalRateLimitFilter::stat_prefix` (`local_rate_limit.rs:49`).
#[allow(dead_code)]
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
///
/// `#[allow(dead_code)]` for Tasks 2-3 interim — Task 3 wires this fn into
/// `RbacFilter::decode_headers`.
#[allow(dead_code)]
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
///
/// `#[allow(dead_code)]` for Tasks 2-3 interim — Task 3 wires this fn into
/// `RbacFilter::decode_headers`.
#[allow(dead_code)]
pub(crate) fn eval_principal(p: &RuntimePrincipal, req: &FilterRequest) -> bool {
    match p {
        RuntimePrincipal::Any(b) => *b,
        RuntimePrincipal::Header(m) => m.matches(&req.headers),
        RuntimePrincipal::AndIds(set) => set.iter().all(|p| eval_principal(p, req)),
        RuntimePrincipal::OrIds(set) => set.iter().any(|p| eval_principal(p, req)),
        RuntimePrincipal::NotId(inner) => !eval_principal(inner, req),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FilterRequest;
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
}
