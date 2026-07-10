//! `envoy.filters.http.rbac` runtime filter (phase 10).
//!
//! Hand-rolled per D-3.2's "Every individual filter ... Must be written from
//! scratch" doctrine + the 07.2 `header_mutation.rs` + 09 `local_rate_limit.rs`
//! precedent. Permission/Principal tree-walk evaluator + RbacFilter runtime.

use std::sync::Arc;

use envoy_config::HeaderMatcher;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// Build-time-lowered runtime representation of an Envoy RBAC `Permission`
/// or `Principal`.
///
/// The two wire-format discriminated unions (`envoy.config.rbac.v3.Permission`
/// / `.Principal`) are structurally symmetric at the runtime layer (PLAN
/// lock-ins #6 + #7), so a single runtime enum serves both; only the
/// wire-lowering entry points (`lower_permission` / `lower_principal`) stay
/// distinct. Flattening per PLAN lock-in #6: the wire-format
/// `PermissionSet { rules }` / `PrincipalSet { ids }` wrappers are collapsed
/// into a direct `Vec<RuntimeMatcher>` payload on the `And` / `Or` variants.
/// The `Box` indirection appears only on `Not` (single-child negation);
/// `And` / `Or` already hold their children behind the `Vec`'s allocation so
/// no per-variant `Box` is needed.
#[derive(Debug)]
pub(crate) enum RuntimeMatcher {
    /// Constant truth value. Wire-form `{ any: true }` / `{ any: false }`.
    Any(bool),
    /// Per-header predicate; delegates to `HeaderMatcher::matches`.
    Header(HeaderMatcher),
    /// Conjunction (wire `and_rules` / `and_ids`): matches iff every child
    /// matches. Short-circuits on first `false` via `Iterator::all`.
    And(Vec<RuntimeMatcher>),
    /// Disjunction (wire `or_rules` / `or_ids`): matches iff any child
    /// matches. Short-circuits on first `true` via `Iterator::any`.
    Or(Vec<RuntimeMatcher>),
    /// Negation (wire `not_rule` / `not_id`) of a single inner matcher.
    Not(Box<RuntimeMatcher>),
    /// Phase 35: dynamic-metadata condition. Holds the config matcher directly
    /// (the `Header(HeaderMatcher)` precedent). Reads a single-segment metadata
    /// path; absent namespace/key → no match.
    Metadata(envoy_config::MetadataMatcher),
    /// Phase 37: `url_path` condition. Holds the inner `StringMatcher` directly
    /// (the `PathMatcher` wrapper is trivial). Matches the query-stripped req.path.
    UrlPath(envoy_config::StringMatcher),
}

/// Recursive tree-walk evaluator for `RuntimeMatcher` (serves both the
/// Permission and the Principal side). Synchronous, pure-compute, no I/O.
/// Returns `true` iff the matcher tree matches the request. Short-circuits
/// via `Iterator::all` (And) and `Iterator::any` (Or); `Not` negates its
/// inner result. Per PLAN lock-ins #8 + #9.
pub(crate) fn eval(m: &RuntimeMatcher, req: &FilterRequest) -> bool {
    match m {
        RuntimeMatcher::Any(b) => *b,
        RuntimeMatcher::Header(hm) => hm.matches(&req.headers),
        RuntimeMatcher::And(set) => set.iter().all(|m| eval(m, req)),
        RuntimeMatcher::Or(set) => set.iter().any(|m| eval(m, req)),
        RuntimeMatcher::Not(inner) => !eval(inner, req),
        RuntimeMatcher::Metadata(mm) => eval_metadata(mm, req),
        RuntimeMatcher::UrlPath(sm) => sm.matches(strip_query(&req.path)),
    }
}

/// Phase 37: extract the path Envoy matches `url_path` against — the request
/// target with everything from the first `?` removed (ADR-0090 §B: query-strip
/// ONLY; no percent-decode / dot-segment / slash / case normalization). Envoy's
/// `#fragment` is rejected at the H1 codec (400) before it reaches here (R1/M37-1).
fn strip_query(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
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
    permissions: Vec<RuntimeMatcher>,
    principals: Vec<RuntimeMatcher>,
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
        let allowed_counter = crate::error::register_counter(
            registry,
            &format!("http.{hcm_stat_prefix}.rbac.allowed"),
        )?;
        let denied_counter = crate::error::register_counter(
            registry,
            &format!("http.{hcm_stat_prefix}.rbac.denied"),
        )?;
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
            let perm_match = p.permissions.iter().any(|x| eval(x, req));
            let prin_match = p.principals.iter().any(|x| eval(x, req));
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
            // ADR-0034: 19 bytes, no trailing newline, per upstream Envoy v1.33
            // empirical verification.
            Decision::StopAndSend(FilterResponse::static_reply(
                403,
                Some("Forbidden"),
                b"RBAC: access denied",
            ))
        }
    }

    /// Encode-side no-op per SPEC §5.4 — RBAC operates on requests only.
    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

/// Shared `Header` arm body for `lower_permission` / `lower_principal`:
/// clone the wire-form matcher and compile any `SafeRegex` on the owned copy
/// so a malformed pattern is boot-fatal (phase 36 M35-1 fix, §A4).
fn compile_header(m: &HeaderMatcher) -> Result<HeaderMatcher, FilterError> {
    let mut m = m.clone();
    m.compile_safe_regexes()
        .map_err(|e| FilterError::InvalidConfig {
            message: e.to_string(),
        })?;
    Ok(m)
}

/// Shared `Metadata` arm body: compile the ValueMatcher's `SafeRegex` on the
/// owned clone (phase 36 M35-1 fix, §A4).
fn compile_metadata(
    m: &envoy_config::MetadataMatcher,
) -> Result<envoy_config::MetadataMatcher, FilterError> {
    let mut m = m.clone();
    m.value
        .compile_safe_regexes()
        .map_err(|e| FilterError::InvalidConfig {
            message: e.to_string(),
        })?;
    Ok(m)
}

/// Shared `UrlPath` arm body (phase 37): reuse the phase-36 fallible SafeRegex
/// compile so a malformed `safe_regex` url_path pattern is boot-fatal, not a
/// first-request panic.
fn compile_url_path(
    pm: &envoy_config::PathMatcher,
) -> Result<envoy_config::StringMatcher, FilterError> {
    let mut sm = pm.path.clone();
    sm.compile_safe_regex()
        .map_err(|e| FilterError::InvalidConfig {
            message: e.to_string(),
        })?;
    Ok(sm)
}

/// Recursive lowering of wire-form `envoy_config::Permission` → runtime
/// `RuntimeMatcher`. Flattens the `PermissionSet { rules }` wrapper on
/// `and_rules`/`or_rules` into the runtime enum's direct `Vec<RuntimeMatcher>`
/// payload per PLAN lock-in #6. Fallible: `Header` and `Metadata` arms
/// compile any `SafeRegex` on the owned clone (phase 36 M35-1 fix, §A4).
fn lower_permission(p: &envoy_config::Permission) -> Result<RuntimeMatcher, FilterError> {
    Ok(match p {
        envoy_config::Permission::Any(b) => RuntimeMatcher::Any(*b),
        envoy_config::Permission::Header(m) => RuntimeMatcher::Header(compile_header(m)?),
        envoy_config::Permission::AndRules(set) => RuntimeMatcher::And(
            set.rules
                .iter()
                .map(lower_permission)
                .collect::<Result<_, _>>()?,
        ),
        envoy_config::Permission::OrRules(set) => RuntimeMatcher::Or(
            set.rules
                .iter()
                .map(lower_permission)
                .collect::<Result<_, _>>()?,
        ),
        envoy_config::Permission::NotRule(inner) => {
            RuntimeMatcher::Not(Box::new(lower_permission(inner)?))
        }
        envoy_config::Permission::Metadata(m) => RuntimeMatcher::Metadata(compile_metadata(m)?),
        envoy_config::Permission::UrlPath(pm) => RuntimeMatcher::UrlPath(compile_url_path(pm)?),
        // 67.2 D3 (ADR-0133): the L4-only arms are UNSUPPORTED in the HTTP RBAC
        // filter — rejected fail-loud at construction (this `lower_*` runs inside a
        // `collect::<Result<_,_>>()?` at filter build, so the `Err` is startup
        // fatal). Upstream Envoy ACCEPTS them in an HTTP rbac filter (measured), so
        // this is a deliberate divergence (ADR-0049 decision-2 (b)), not parity.
        envoy_config::Permission::DestinationIp(_) | envoy_config::Permission::DestinationPort(_) => {
            return Err(FilterError::InvalidConfig {
                message: "envoy.filters.http.rbac: destination_ip / destination_port are \
                          L4-only matchers, unsupported in the HTTP RBAC filter (ADR-0133)"
                    .into(),
            });
        }
    })
}

/// Recursive lowering of wire-form `envoy_config::Principal` → runtime
/// `RuntimeMatcher`. Symmetric to `lower_permission` per PLAN lock-in #7;
/// `PrincipalSet { ids }` wrapper flattened on `and_ids`/`or_ids`. Fallible:
/// `Header` and `Metadata` arms compile any `SafeRegex` on the owned clone
/// (phase 36 M35-1 fix, §A4).
fn lower_principal(p: &envoy_config::Principal) -> Result<RuntimeMatcher, FilterError> {
    Ok(match p {
        envoy_config::Principal::Any(b) => RuntimeMatcher::Any(*b),
        envoy_config::Principal::Header(m) => RuntimeMatcher::Header(compile_header(m)?),
        envoy_config::Principal::AndIds(set) => RuntimeMatcher::And(
            set.ids
                .iter()
                .map(lower_principal)
                .collect::<Result<_, _>>()?,
        ),
        envoy_config::Principal::OrIds(set) => RuntimeMatcher::Or(
            set.ids
                .iter()
                .map(lower_principal)
                .collect::<Result<_, _>>()?,
        ),
        envoy_config::Principal::NotId(inner) => {
            RuntimeMatcher::Not(Box::new(lower_principal(inner)?))
        }
        envoy_config::Principal::Metadata(m) => RuntimeMatcher::Metadata(compile_metadata(m)?),
        // Phase 37: symmetric to `lower_permission`'s url_path arm.
        envoy_config::Principal::UrlPath(pm) => RuntimeMatcher::UrlPath(compile_url_path(pm)?),
        // 67.2 D3 (ADR-0133): the connection-level source-IP arms are UNSUPPORTED
        // in the HTTP RBAC filter — rejected fail-loud (see `lower_permission`).
        envoy_config::Principal::DirectRemoteIp(_)
        | envoy_config::Principal::RemoteIp(_)
        | envoy_config::Principal::SourceIp(_) => {
            return Err(FilterError::InvalidConfig {
                message: "envoy.filters.http.rbac: direct_remote_ip / remote_ip / source_ip are \
                          connection-level matchers, unsupported in the HTTP RBAC filter (ADR-0133)"
                    .into(),
            });
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::header_matcher_exact;
    use envoy_config::{
        HeaderMatcherMode, MetadataMatcher, MetadataPathSegment, StringMatcher, StringMatcherMode,
        ValueMatcher,
    };

    fn req_with(headers: Vec<(&'static str, &'static str)>) -> FilterRequest {
        FilterRequest::test("GET", "/", &headers)
    }

    // Phase 37: a request whose only varying axis is the request-target `path`.
    fn req_with_path(path: &str) -> FilterRequest {
        FilterRequest::test("GET", path, &[])
    }

    #[test]
    fn url_path_permission_exact_matches_and_strips_query() {
        let sm = StringMatcher {
            mode: StringMatcherMode::Exact("/allowed".into()),
            ignore_case: false,
        };
        let p = RuntimeMatcher::UrlPath(sm);
        assert!(eval(&p, &req_with_path("/allowed"))); // match
        assert!(eval(&p, &req_with_path("/allowed?x=1"))); // query stripped (ADR-0090 §B)
        assert!(eval(&p, &req_with_path("/allowed?"))); // empty query stripped
        assert!(!eval(&p, &req_with_path("/denied"))); // miss
        assert!(!eval(&p, &req_with_path("/allowed/"))); // trailing slash significant
    }

    #[test]
    fn url_path_principal_matches_query_stripped() {
        let sm = StringMatcher {
            mode: StringMatcherMode::Exact("/allowed".into()),
            ignore_case: false,
        };
        let p = RuntimeMatcher::UrlPath(sm);
        assert!(eval(&p, &req_with_path("/allowed?x=1")));
        assert!(!eval(&p, &req_with_path("/denied")));
    }

    // ---- Phase 37: url_path backstop (Task 4) ----
    // ADR-0090 §C: LOCK anchored `^…$` safe_regex (M36-1 — partial==full).

    #[test]
    fn url_path_all_string_modes() {
        use StringMatcherMode::*;
        for (mode, path, want) in [
            (Prefix("/api".into()), "/api/users", true),
            (Prefix("/api".into()), "/v2/users", false),
            (Suffix("/health".into()), "/svc/health", true),
            (Suffix("/health".into()), "/svc/ready", false),
            (Contains("admin".into()), "/x/admin/y", true),
            (Contains("admin".into()), "/x/user/y", false),
        ] {
            let p = RuntimeMatcher::UrlPath(StringMatcher {
                mode,
                ignore_case: false,
            });
            assert_eq!(eval(&p, &req_with_path(path)), want, "path={path}");
        }
    }

    #[test]
    fn url_path_composes_and_inverts_under_deny() {
        // DENY policy whose permission is `not_rule { url_path exact /allowed }`,
        // principal any:true. DENY + match(of not) inverts:
        //   /allowed → matched-by-inner → not_rule false → policy no-match →
        //              DENY-action no-match → ALLOW (Continue);
        //   /other   → inner false → not_rule true → policy match → DENY (StopAndSend).
        use envoy_config::*;
        let url = Permission::UrlPath(PathMatcher {
            path: StringMatcher {
                mode: StringMatcherMode::Exact("/allowed".into()),
                ignore_case: false,
            },
        });
        let cfg = RbacConfig {
            rules: Rules {
                action: Action::Deny,
                policies: [(
                    "p0".to_string(),
                    Policy {
                        permissions: vec![Permission::NotRule(Box::new(url))],
                        principals: vec![Principal::Any(true)],
                    },
                )]
                .into_iter()
                .collect(),
            },
        };
        let registry = std::sync::Arc::new(StatsRegistry::new());
        let mut f = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").unwrap();
        assert!(matches!(
            f.decode_headers(&mut req_with_path("/allowed")),
            Decision::Continue
        ));
        assert!(matches!(
            f.decode_headers(&mut req_with_path("/other")),
            Decision::StopAndSend(_)
        ));
    }

    #[test]
    fn url_path_composes_in_and_or_rules() {
        // SPEC §2.1.5: url_path composes inside and_rules / or_rules.
        let url = |p: &str| {
            RuntimeMatcher::UrlPath(StringMatcher {
                mode: StringMatcherMode::Prefix(p.into()),
                ignore_case: false,
            })
        };
        // and_rules: BOTH prefixes must match.
        let and = RuntimeMatcher::And(vec![url("/api"), url("/api/v2")]);
        assert!(eval(&and, &req_with_path("/api/v2/users")));
        assert!(!eval(&and, &req_with_path("/api/v1/users")));
        // or_rules: EITHER prefix matches.
        let or = RuntimeMatcher::Or(vec![url("/api"), url("/admin")]);
        assert!(eval(&or, &req_with_path("/admin/x")));
        assert!(!eval(&or, &req_with_path("/public/x")));
    }

    #[test]
    fn url_path_anchored_safe_regex_matches_without_panic() {
        // ADR-0090 §C: anchored ^/allowed/[0-9]+$ ; compiles at lowering, no
        // first-request panic.
        use envoy_config::*;
        let sr = StringMatcher {
            mode: StringMatcherMode::SafeRegex(SafeRegex {
                regex: "^/allowed/[0-9]+$".into(),
                compiled: None,
            }),
            ignore_case: false,
        };
        let cfg = RbacConfig {
            rules: Rules {
                action: Action::Allow,
                policies: [(
                    "p0".to_string(),
                    Policy {
                        permissions: vec![Permission::UrlPath(PathMatcher { path: sr })],
                        principals: vec![Principal::Any(true)],
                    },
                )]
                .into_iter()
                .collect(),
            },
        };
        let registry = std::sync::Arc::new(StatsRegistry::new());
        let mut f = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").unwrap();
        assert!(matches!(
            f.decode_headers(&mut req_with_path("/allowed/42")),
            Decision::Continue
        ));
        assert!(matches!(
            f.decode_headers(&mut req_with_path("/allowed/42?q=1")),
            Decision::Continue
        )); // query-strip
        assert!(matches!(
            f.decode_headers(&mut req_with_path("/allowed/xx")),
            Decision::StopAndSend(_)
        ));
        assert!(matches!(
            f.decode_headers(&mut req_with_path("/allowed")),
            Decision::StopAndSend(_)
        )); // full-anchor
    }

    // ---- Phase 37: url_path §D case 4 — malformed safe_regex is boot-fatal (Task 5) ----
    #[test]
    fn url_path_malformed_safe_regex_is_build_error() {
        use envoy_config::*;
        let bad = StringMatcher {
            mode: StringMatcherMode::SafeRegex(SafeRegex {
                regex: "[".into(),
                compiled: None,
            }),
            ignore_case: false,
        };
        let cfg = RbacConfig {
            rules: Rules {
                action: Action::Allow,
                policies: [(
                    "p0".to_string(),
                    Policy {
                        permissions: vec![Permission::UrlPath(PathMatcher { path: bad })],
                        principals: vec![Principal::Any(true)],
                    },
                )]
                .into_iter()
                .collect(),
            },
        };
        let registry = std::sync::Arc::new(StatsRegistry::new());
        assert!(matches!(
            RbacFilter::build_from_config(&cfg, &registry, "ingress_http"),
            Err(FilterError::InvalidConfig { .. })
        ));
    }

    #[test]
    fn any_true_permission_matches() {
        let req = req_with(vec![]);
        assert!(eval(&RuntimeMatcher::Any(true), &req));
    }

    #[test]
    fn any_false_permission_does_not_match() {
        let req = req_with(vec![]);
        assert!(!eval(&RuntimeMatcher::Any(false), &req));
    }

    #[test]
    fn header_permission_matches_when_value_equals() {
        let req = req_with(vec![("x-rbac-pass", "yes")]);
        let perm = RuntimeMatcher::Header(header_matcher_exact("x-rbac-pass", "yes"));
        assert!(eval(&perm, &req));
    }

    #[test]
    fn header_permission_does_not_match_when_value_differs() {
        let req = req_with(vec![("x-rbac-pass", "no")]);
        let perm = RuntimeMatcher::Header(header_matcher_exact("x-rbac-pass", "yes"));
        assert!(!eval(&perm, &req));
    }

    #[test]
    fn header_permission_does_not_match_when_header_absent() {
        let req = req_with(vec![("x-other", "yes")]);
        let perm = RuntimeMatcher::Header(header_matcher_exact("x-rbac-pass", "yes"));
        assert!(!eval(&perm, &req));
    }

    #[test]
    fn and_rules_short_circuits_on_first_false() {
        let req = req_with(vec![]);
        let perm = RuntimeMatcher::And(vec![
            RuntimeMatcher::Any(true),
            RuntimeMatcher::Any(false),
            RuntimeMatcher::Any(true),
        ]);
        assert!(!eval(&perm, &req));
    }

    #[test]
    fn and_rules_all_true_matches() {
        let req = req_with(vec![]);
        let perm = RuntimeMatcher::And(vec![RuntimeMatcher::Any(true), RuntimeMatcher::Any(true)]);
        assert!(eval(&perm, &req));
    }

    #[test]
    fn or_rules_short_circuits_on_first_true() {
        let req = req_with(vec![]);
        let perm = RuntimeMatcher::Or(vec![
            RuntimeMatcher::Any(false),
            RuntimeMatcher::Any(true),
            RuntimeMatcher::Any(false),
        ]);
        assert!(eval(&perm, &req));
    }

    #[test]
    fn or_rules_all_false_does_not_match() {
        let req = req_with(vec![]);
        let perm = RuntimeMatcher::Or(vec![RuntimeMatcher::Any(false), RuntimeMatcher::Any(false)]);
        assert!(!eval(&perm, &req));
    }

    #[test]
    fn not_rule_negates_inner() {
        let req = req_with(vec![]);
        let perm_t = RuntimeMatcher::Not(Box::new(RuntimeMatcher::Any(false)));
        let perm_f = RuntimeMatcher::Not(Box::new(RuntimeMatcher::Any(true)));
        assert!(eval(&perm_t, &req));
        assert!(!eval(&perm_f, &req));
    }

    #[test]
    fn nested_and_or_not_evaluates_correctly() {
        let req = req_with(vec![("x-a", "1"), ("x-b", "2")]);
        // (header x-a == "1") AND NOT(header x-b == "3")
        let perm = RuntimeMatcher::And(vec![
            RuntimeMatcher::Header(header_matcher_exact("x-a", "1")),
            RuntimeMatcher::Not(Box::new(RuntimeMatcher::Header(header_matcher_exact(
                "x-b", "3",
            )))),
        ]);
        assert!(eval(&perm, &req));
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
        let perm = RuntimeMatcher::Metadata(metadata_matcher(
            "envoy.filters.http.header_to_metadata",
            "tier",
            "prod",
        ));
        assert!(eval(&perm, &req));
    }

    #[test]
    fn metadata_permission_no_match_on_value_mismatch() {
        // tier=dev present but matcher wants exact "prod" → false
        let req = req_with_md("envoy.filters.http.header_to_metadata", "tier", "dev");
        let perm = RuntimeMatcher::Metadata(metadata_matcher(
            "envoy.filters.http.header_to_metadata",
            "tier",
            "prod",
        ));
        assert!(!eval(&perm, &req));
    }

    #[test]
    fn metadata_permission_no_match_on_absent_namespace() {
        // req has a DIFFERENT namespace → false
        let req = req_with_md("some.other.ns", "tier", "prod");
        let perm = RuntimeMatcher::Metadata(metadata_matcher(
            "envoy.filters.http.header_to_metadata",
            "tier",
            "prod",
        ));
        assert!(!eval(&perm, &req));
    }

    #[test]
    fn metadata_permission_no_match_on_absent_key() {
        // namespace present, but a different key → false
        let req = req_with_md("envoy.filters.http.header_to_metadata", "other_key", "prod");
        let perm = RuntimeMatcher::Metadata(metadata_matcher(
            "envoy.filters.http.header_to_metadata",
            "tier",
            "prod",
        ));
        assert!(!eval(&perm, &req));
    }

    #[test]
    fn metadata_principal_mirrors_permission() {
        // RuntimeMatcher::Metadata, same present-value match
        let req = req_with_md("envoy.filters.http.header_to_metadata", "tier", "prod");
        let prin = RuntimeMatcher::Metadata(metadata_matcher(
            "envoy.filters.http.header_to_metadata",
            "tier",
            "prod",
        ));
        assert!(eval(&prin, &req));
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
        assert!(eval(
            &RuntimeMatcher::Metadata(present_matcher(ns, "tier", true)),
            &req
        ));
    }

    #[test]
    fn metadata_present_match_true_no_match_when_absent() {
        let ns = "envoy.filters.http.header_to_metadata";
        let req = req_with_md(ns, "other", "x"); // key tier absent
        assert!(!eval(
            &RuntimeMatcher::Metadata(present_matcher(ns, "tier", true)),
            &req
        ));
    }

    #[test]
    fn metadata_present_match_false_never_matches() {
        // §A1: present_match:false → present && false → never matches, even when present.
        let ns = "envoy.filters.http.header_to_metadata";
        let present = req_with_md(ns, "tier", "staging");
        let absent = req_with(vec![]);
        assert!(!eval(
            &RuntimeMatcher::Metadata(present_matcher(ns, "tier", false)),
            &present
        ));
        assert!(!eval(
            &RuntimeMatcher::Metadata(present_matcher(ns, "tier", false)),
            &absent
        ));
    }

    #[test]
    fn principal_evaluator_mirrors_permission_evaluator() {
        let req = req_with(vec![("x-user", "alice")]);
        let prin = RuntimeMatcher::Or(vec![
            RuntimeMatcher::Header(header_matcher_exact("x-user", "bob")),
            RuntimeMatcher::Header(header_matcher_exact("x-user", "alice")),
        ]);
        assert!(eval(&prin, &req));
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
        let mut resp = crate::types::FilterResponse::test_200();
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

    /// 67.2 D3 (ADR-0133): the HTTP RBAC filter REJECTS the L4-only arms fail-loud.
    /// Upstream Envoy ACCEPTS them in an HTTP rbac filter (measured) — this is a
    /// deliberate divergence (ADR-0049 decision-2 (b)), not parity.
    #[test]
    fn http_rbac_rejects_destination_port_permission() {
        let err = lower_permission(&envoy_config::Permission::DestinationPort(8080))
            .expect_err("destination_port is L4-only in the HTTP filter");
        assert!(matches!(err, FilterError::InvalidConfig { .. }), "got {err:?}");
    }

    #[test]
    fn http_rbac_rejects_direct_remote_ip_principal() {
        let cidr = serde_yaml::from_str::<envoy_config::CidrRange>(
            "address_prefix: 10.0.0.0\nprefix_len: 8",
        )
        .unwrap();
        let err = lower_principal(&envoy_config::Principal::DirectRemoteIp(cidr))
            .expect_err("direct_remote_ip is L4-only in the HTTP filter");
        assert!(matches!(err, FilterError::InvalidConfig { .. }), "got {err:?}");
    }

    /// 67.2 D3 / Task 4: the remaining L4-only Permission arm (`destination_ip`).
    #[test]
    fn http_rbac_rejects_destination_ip_permission() {
        let cidr = serde_yaml::from_str::<envoy_config::CidrRange>(
            "address_prefix: 10.0.0.0\nprefix_len: 8",
        )
        .unwrap();
        assert!(matches!(
            lower_permission(&envoy_config::Permission::DestinationIp(cidr)),
            Err(FilterError::InvalidConfig { .. })
        ));
    }

    /// 67.2 D3 / Task 4: the remaining two source-IP Principal arms
    /// (`remote_ip`, `source_ip`) are each rejected fail-loud.
    #[test]
    fn http_rbac_rejects_remote_ip_and_source_ip_principals() {
        for ctor in [
            envoy_config::Principal::RemoteIp as fn(envoy_config::CidrRange) -> envoy_config::Principal,
            envoy_config::Principal::SourceIp,
        ] {
            let cidr = serde_yaml::from_str::<envoy_config::CidrRange>(
                "address_prefix: 10.0.0.0\nprefix_len: 8",
            )
            .unwrap();
            assert!(matches!(
                lower_principal(&ctor(cidr)),
                Err(FilterError::InvalidConfig { .. })
            ));
        }
    }

    /// 67.2 D3 / Task 4: the rejection is STARTUP-FATAL — it propagates through
    /// the whole `RbacFilter::build_from_config` builder, not just the private
    /// `lower_*` helpers. An HTTP rbac filter carrying an L4 principal fails to
    /// construct. (Upstream ACCEPTS it — deliberate divergence, ADR-0133.)
    #[test]
    fn http_rbac_build_from_config_rejects_l4_principal_startup_fatal() {
        use envoy_stats::StatsRegistry;
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let registry = Arc::new(StatsRegistry::new());
        let cidr = serde_yaml::from_str::<envoy_config::CidrRange>(
            "address_prefix: 10.0.0.0\nprefix_len: 8",
        )
        .unwrap();
        let mut policies = BTreeMap::new();
        policies.insert(
            "l4".to_string(),
            envoy_config::Policy {
                permissions: vec![envoy_config::Permission::Any(true)],
                principals: vec![envoy_config::Principal::DirectRemoteIp(cidr)],
            },
        );
        let cfg = envoy_config::RbacConfig {
            rules: envoy_config::Rules {
                action: envoy_config::Action::Allow,
                policies,
            },
        };
        assert!(matches!(
            RbacFilter::build_from_config(&cfg, &registry, "ingress_http"),
            Err(FilterError::InvalidConfig { .. })
        ));
    }
}
