//! `envoy.filters.network.rbac` — the Network-filters family's FIRST
//! NON-TERMINAL filter (phase 67.1, ADR-0128 / ADR-0129).
//!
//! **DO NOT CONFUSE THIS WITH `crates/envoy-filter/src/rbac.rs`**, which
//! implements `envoy.filters.http.rbac` — a DIFFERENT feature that shares the
//! name, operating on HTTP requests rather than L4 connections. The two share
//! only the `Rules` / `Policy` / `Permission` / `Principal` config trees.
//!
//! The filter decides ONCE per connection, when the FIRST DOWNSTREAM BYTE
//! arrives — upstream Envoy's `ONE_TIME_ON_FIRST_BYTE` enforcement, measured
//! against `envoyproxy/envoy:v1.33.0` (**ADR-0131**, which corrects phase-67
//! SPEC R-2's "at establishment" reading). A connection that never sends a byte
//! is never evaluated and ticks NO counter, on either proxy.
//!
//! `envoy_listener::ChainHandler` owns that timing (it peeks, without consuming,
//! for the first byte). This filter inspects `peer_addr` / `local_addr` only and
//! never reads the payload — which is why the iteration protocol needs only
//! `on_new_connection` and no `on_data` hook (deferred as CF-67-3).
//!
//! On DENY the filter returns `StopIteration`; `envoy_listener::ChainHandler`
//! then closes the connection with ZERO bytes written and a clean EOF, never an
//! RST (SPEC R-2). This module never touches the socket.
//!
//! Phase 67.1 supports the `any` matcher plus the `and`/`or`/`not` combinators.
//! The connection-level arms (`direct_remote_ip`, `remote_ip`, `source_ip`,
//! `destination_port`, `destination_ip`) land in `67.2`; they are NOT stubbed
//! here — they do not exist, and the config parser rejects them as unknown keys.

use std::sync::Arc;

use envoy_config::{Action, NetworkRbacConfig, Permission, Principal, Rules};
use envoy_listener::{ConnectionInfo, NetworkFilter, NetworkFilterStatus};

pub struct NetworkRbacFilter {
    /// `None` ⇒ the filter is INERT: allow, and tick NEITHER counter
    /// (SPEC R-4, measured). Never materialise a default `Rules` here.
    rules: Option<Rules>,
    allowed: Arc<envoy_stats::Counter>,
    denied: Arc<envoy_stats::Counter>,
}

impl NetworkRbacFilter {
    /// Registers the four `<stat_prefix>.rbac.*` counters. All four register
    /// unconditionally — including when `rules` is `None` — so the stat TREE
    /// matches upstream's shape, which emits all four at 0 for an inert filter
    /// (SPEC R-3, R-4).
    ///
    /// `shadow_allowed` / `shadow_denied` are registered and NEVER incremented:
    /// shadow policies are not modeled, and a `shadow_rules` config field is
    /// rejected loudly by `deny_unknown_fields` (CF-67-1).
    pub fn new(
        cfg: &NetworkRbacConfig,
        registry: &envoy_stats::StatsRegistry,
    ) -> Result<Self, envoy_stats::StatsError> {
        let p = &cfg.stat_prefix;
        let allowed = registry.register_counter(&format!("{p}.rbac.allowed"))?;
        let denied = registry.register_counter(&format!("{p}.rbac.denied"))?;
        registry.register_counter(&format!("{p}.rbac.shadow_allowed"))?;
        registry.register_counter(&format!("{p}.rbac.shadow_denied"))?;
        Ok(Self {
            rules: cfg.rules.clone(),
            allowed,
            denied,
        })
    }
}

impl NetworkFilter for NetworkRbacFilter {
    fn on_new_connection(&self, conn: &ConnectionInfo) -> NetworkFilterStatus {
        // SPEC R-4: `rules` omitted ⇒ INERT. Allow, and tick NOTHING.
        let Some(rules) = self.rules.as_ref() else {
            return NetworkFilterStatus::Continue;
        };
        if engine_allows(rules, conn) {
            self.allowed.inc();
            NetworkFilterStatus::Continue
        } else {
            self.denied.inc();
            NetworkFilterStatus::StopIteration
        }
    }
}

/// Upstream Envoy's RBAC verdict: a policy matches when ANY permission matches
/// AND ANY principal matches; the engine's verdict is `action` when SOME policy
/// matches, and the INVERSE of `action` otherwise.
fn engine_allows(rules: &Rules, conn: &ConnectionInfo) -> bool {
    let matched = rules.policies.values().any(|policy| {
        policy
            .permissions
            .iter()
            .any(|p| permission_matches(p, conn))
            && policy.principals.iter().any(|p| principal_matches(p, conn))
    });
    match rules.action {
        Action::Allow => matched,
        Action::Deny => !matched,
    }
}

/// EXHAUSTIVE, no `_ =>` catch-all. `67.2` adds `DestinationPort` /
/// `DestinationIp`; this must fail to compile until they are implemented, which
/// is the GOOD failure mode. **Never add a catch-all.**
///
/// `Any(b) => *b` — `any: false` never matches. Mirrors the landed HTTP RBAC
/// evaluator (`crates/envoy-filter/src/rbac.rs`, `RuntimeMatcher::Any(b) => *b`).
///
/// `Header` / `Metadata` / `UrlPath` are UNREACHABLE: `envoy-config`'s
/// `validate_l4_permission` (67.1 D3, CF-67-4) rejects them at config load. They
/// return `false` rather than panicking — a data-plane path must never panic —
/// with a `debug_assert!` to catch a validator regression in test builds.
///
/// `conn` is threaded through but not read by any arm 67.1 ships: `any` and the
/// combinators ignore the connection. `67.2`'s `destination_port` /
/// `destination_ip` arms read it. Keeping it in the signature now is what lets
/// `67.2` add those arms without touching every call site.
#[allow(clippy::only_used_in_recursion)]
fn permission_matches(p: &Permission, conn: &ConnectionInfo) -> bool {
    match p {
        Permission::Any(b) => *b,
        Permission::AndRules(set) => set.rules.iter().all(|c| permission_matches(c, conn)),
        Permission::OrRules(set) => set.rules.iter().any(|c| permission_matches(c, conn)),
        Permission::NotRule(inner) => !permission_matches(inner, conn),
        Permission::Header(_) | Permission::Metadata(_) | Permission::UrlPath(_) => {
            debug_assert!(
                false,
                "validate_l4_permission must reject this arm at config load"
            );
            false
        }
    }
}

/// The `Principal` twin of [`permission_matches`]. EXHAUSTIVE, no catch-all:
/// `67.2` adds `DirectRemoteIp` / `RemoteIp` / `SourceIp`, which are what will
/// read `conn` (see [`permission_matches`] on why it is threaded through now).
#[allow(clippy::only_used_in_recursion)]
fn principal_matches(p: &Principal, conn: &ConnectionInfo) -> bool {
    match p {
        Principal::Any(b) => *b,
        Principal::AndIds(set) => set.ids.iter().all(|c| principal_matches(c, conn)),
        Principal::OrIds(set) => set.ids.iter().any(|c| principal_matches(c, conn)),
        Principal::NotId(inner) => !principal_matches(inner, conn),
        Principal::Header(_) | Principal::Metadata(_) | Principal::UrlPath(_) => {
            debug_assert!(
                false,
                "validate_l4_principal must reject this arm at config load"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> ConnectionInfo {
        ConnectionInfo {
            peer_addr: "10.0.0.1:54321".parse().unwrap(),
            local_addr: "127.0.0.1:10000".parse().unwrap(),
        }
    }

    fn cfg(stat_prefix: &str, rules_yaml: Option<&str>) -> envoy_config::NetworkRbacConfig {
        let yaml = match rules_yaml {
            Some(r) => format!("stat_prefix: {stat_prefix}\nrules:\n{r}"),
            None => format!("stat_prefix: {stat_prefix}"),
        };
        serde_yaml::from_str(&yaml).expect("NetworkRbacConfig parses")
    }

    const ANY_POLICY: &str = "  policies:\n    p0:\n      permissions: [{ any: true }]\n      principals: [{ any: true }]";

    /// Read a counter's value. **Do not use this to assert REGISTRATION** —
    /// `register_counter` is get-or-create (`envoy_stats::StatsRegistry`), so it
    /// would happily mint the counter and read 0 for a name nobody registered.
    /// Use [`registered_stat`] for that (M-1).
    fn stat(reg: &envoy_stats::StatsRegistry, name: &str) -> u64 {
        reg.register_counter(name).expect("counter").value()
    }

    /// M-1: a NON-CREATING lookup. `snapshot()` reads the registry map without
    /// inserting, so a missing name is a genuine "never registered" failure rather
    /// than a silently-created zero.
    ///
    /// This is what makes the registration half of
    /// `shadow_counters_register_at_zero_and_never_tick` non-vacuous. With `stat`
    /// it passed even if `NetworkRbacFilter::new` had registered nothing at all.
    fn registered_stat(reg: &envoy_stats::StatsRegistry, name: &str) -> u64 {
        let (_, handle) = reg
            .snapshot()
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| {
                panic!("counter {name} must be REGISTERED by NetworkRbacFilter::new")
            });
        match handle {
            envoy_stats::StatHandle::Counter(c) => c.value(),
            other => panic!(
                "{name} registered as {:?}, expected a counter",
                other.kind_str()
            ),
        }
    }

    /// D6 / SPEC R-2: `action: ALLOW` + a matching policy ⇒ Continue, `allowed` ticks.
    #[test]
    fn allow_action_with_matching_policy_continues_and_ticks_allowed() {
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(
            &cfg("a", Some(&format!("  action: ALLOW\n{ANY_POLICY}"))),
            &reg,
        )
        .unwrap();
        assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue);
        assert_eq!(stat(&reg, "a.rbac.allowed"), 1);
        assert_eq!(stat(&reg, "a.rbac.denied"), 0);
    }

    /// D6 / SPEC R-2: `action: DENY` + a matching policy ⇒ StopIteration, `denied` ticks.
    #[test]
    fn deny_action_with_matching_policy_stops_and_ticks_denied() {
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(
            &cfg("d", Some(&format!("  action: DENY\n{ANY_POLICY}"))),
            &reg,
        )
        .unwrap();
        assert_eq!(
            f.on_new_connection(&conn()),
            NetworkFilterStatus::StopIteration
        );
        assert_eq!(stat(&reg, "d.rbac.denied"), 1);
        assert_eq!(stat(&reg, "d.rbac.allowed"), 0);
    }

    /// D6: the verdict on NO match is the INVERSE of `action`.
    #[test]
    fn no_matching_policy_inverts_the_action() {
        let never = "  policies:\n    p0:\n      permissions: [{ any: false }]\n      principals: [{ any: true }]";
        let reg = envoy_stats::StatsRegistry::new();
        let allow =
            NetworkRbacFilter::new(&cfg("x", Some(&format!("  action: ALLOW\n{never}"))), &reg)
                .unwrap();
        assert_eq!(
            allow.on_new_connection(&conn()),
            NetworkFilterStatus::StopIteration
        );
        assert_eq!(stat(&reg, "x.rbac.denied"), 1);

        let reg2 = envoy_stats::StatsRegistry::new();
        let deny =
            NetworkRbacFilter::new(&cfg("y", Some(&format!("  action: DENY\n{never}"))), &reg2)
                .unwrap();
        assert_eq!(
            deny.on_new_connection(&conn()),
            NetworkFilterStatus::Continue
        );
        assert_eq!(stat(&reg2, "y.rbac.allowed"), 1);
    }

    /// D6: a policy matches only when SOME permission AND SOME principal match.
    #[test]
    fn policy_requires_both_a_permission_and_a_principal_match() {
        let half = "  policies:\n    p0:\n      permissions: [{ any: true }]\n      principals: [{ any: false }]";
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("h", Some(&format!("  action: ALLOW\n{half}"))), &reg)
            .unwrap();
        assert_eq!(
            f.on_new_connection(&conn()),
            NetworkFilterStatus::StopIteration,
            "permission matched but principal did not ⇒ policy does not match"
        );
    }

    /// D6: ANY policy matching is enough.
    #[test]
    fn any_matching_policy_decides() {
        let two = "  policies:\n    p0:\n      permissions: [{ any: false }]\n      principals: [{ any: true }]\n    p1:\n      permissions: [{ any: true }]\n      principals: [{ any: true }]";
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("m", Some(&format!("  action: DENY\n{two}"))), &reg)
            .unwrap();
        assert_eq!(
            f.on_new_connection(&conn()),
            NetworkFilterStatus::StopIteration
        );
    }

    /// D6: the combinators. `and` = all, `or` = any, `not` = negate; nested.
    #[test]
    fn combinators_and_or_not() {
        let pol = "  policies:\n    p0:\n      permissions:\n        - and_rules:\n            rules:\n              - any: true\n              - not_rule: { any: false }\n      principals:\n        - or_ids:\n            ids:\n              - any: false\n              - not_id: { any: false }";
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("c", Some(&format!("  action: ALLOW\n{pol}"))), &reg)
            .unwrap();
        assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue);
        assert_eq!(stat(&reg, "c.rbac.allowed"), 1);
    }

    /// D6: an `and_rules` set with ONE non-matching child does not match.
    #[test]
    fn and_rules_requires_every_child() {
        let pol = "  policies:\n    p0:\n      permissions:\n        - and_rules:\n            rules:\n              - any: true\n              - any: false\n      principals: [{ any: true }]";
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("n", Some(&format!("  action: ALLOW\n{pol}"))), &reg)
            .unwrap();
        assert_eq!(
            f.on_new_connection(&conn()),
            NetworkFilterStatus::StopIteration
        );
    }

    /// D6 / SPEC R-4 — THE INERTNESS WITNESS (PLAN-VERIFY W-6).
    ///
    /// `rules` omitted ⇒ the filter is INERT: the connection is allowed and
    /// NEITHER counter increments. Measured against upstream Envoy: `allowed`
    /// stays 0, not 1. A naive default `Rules { action: ALLOW }` would tick
    /// `allowed` — a STAT divergence with NO body divergence, invisible to a
    /// body-only fixture.
    ///
    /// All four counters are still REGISTERED (at 0), so the stat tree matches.
    #[test]
    fn rules_omitted_is_inert_and_ticks_neither_counter() {
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(&cfg("norules", None), &reg).unwrap();
        for _ in 0..3 {
            assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue);
        }
        assert_eq!(
            stat(&reg, "norules.rbac.allowed"),
            0,
            "INERT: allowed must NOT tick"
        );
        assert_eq!(
            stat(&reg, "norules.rbac.denied"),
            0,
            "INERT: denied must NOT tick"
        );
        assert_eq!(stat(&reg, "norules.rbac.shadow_allowed"), 0);
        assert_eq!(stat(&reg, "norules.rbac.shadow_denied"), 0);
    }

    /// D6 / CF-67-1: all four counters register even with rules present, and the
    /// two shadow counters NEVER tick (shadow policies are not modeled).
    ///
    /// M-1: the REGISTRATION half asserts through the non-creating
    /// [`registered_stat`]. It previously used `stat`, i.e. `register_counter`,
    /// which is **get-or-create** — so it would have created each counter and read
    /// 0 even had `NetworkRbacFilter::new` registered nothing. Only the
    /// *behavioral* half (shadow counters never tick) was sound.
    ///
    /// DELETE A `register_counter` CALL IN `NetworkRbacFilter::new` AND THIS TEST
    /// MUST FAIL.
    #[test]
    fn shadow_counters_register_at_zero_and_never_tick() {
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(
            &cfg("s", Some(&format!("  action: DENY\n{ANY_POLICY}"))),
            &reg,
        )
        .unwrap();
        for _ in 0..5 {
            let _ = f.on_new_connection(&conn());
        }
        // Stat-tree parity: all four names exist, whether or not they ever tick.
        assert_eq!(registered_stat(&reg, "s.rbac.allowed"), 0);
        assert_eq!(registered_stat(&reg, "s.rbac.denied"), 5);
        assert_eq!(registered_stat(&reg, "s.rbac.shadow_allowed"), 0);
        assert_eq!(registered_stat(&reg, "s.rbac.shadow_denied"), 0);
    }

    /// D6: counters accumulate across connections.
    #[test]
    fn counters_accumulate_across_connections() {
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(
            &cfg("acc", Some(&format!("  action: ALLOW\n{ANY_POLICY}"))),
            &reg,
        )
        .unwrap();
        for _ in 0..7 {
            assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue);
        }
        assert_eq!(stat(&reg, "acc.rbac.allowed"), 7);
    }
}
