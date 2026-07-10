# Phase 67.2 — network RBAC connection-level matcher arms — Progress Log

> Running log, updated on each task completion (§5 state-3, `superpowers:executing-plans`).
> This is the §5 state-3 IMPLEMENTATION session. It does NOT chain into state-4
> verification (§5.1) — the §7.5 gate is a separate session. Written for a stranger
> with zero prior context (D-3.4).

**Plan:** `docs/envoy-rust/phases/67.2-network-rbac-connection-matchers/PLAN.md` — 6 tasks / ~695 net LoC.
**Started from:** clean `main` at `af89137` (= `origin/main`; the §5 state-2 PLAN-write commit). No sibling ahead.

TDD per task: write failing test → confirm RED → implement → confirm GREEN → commit.

---

## Task 1 — `CidrRange` type (parse, validate, `contains`) — ✅ DONE

**Commit:** (see `phase 67.2 task 1`).

- Added `pub struct CidrRange { address_prefix: std::net::IpAddr, prefix_len: u8 }`
  (`#[serde(deny_unknown_fields)]`) just above the `Permission` enum in
  `crates/envoy-config/src/bootstrap.rs`, plus `CidrRange::validate` (family-max
  prefix width, `Err(detail)`), `CidrRange::contains` (v4/v6, IPv4-mapped-IPv6
  canonicalised to IPv4 first per ADR-0133), and the free `prefix_match` bit helper.
- Added `ConfigError::InvalidCidrRange { listener, policy_name, path, detail }`
  (scope-neutral `listener {listener:?}`, per 67.1 W-1 / ADR-0130) in
  `crates/envoy-config/src/lib.rs`, and re-exported `CidrRange` from the crate root.
- **7 unit tests** in the RBAC `mod tests`: v4 contains, /0 + /32 boundaries, v6 + /128,
  IPv4-mapped-IPv6 peer matches an IPv4 range, cross-family never matches, over-wide
  prefix rejected by `validate`, and `deny_unknown_fields` + bare-`u8` `prefix_len`
  (the ADR-0133 wrapper rejection).

**Deviation from the PLAN's verbatim test text (found on contact with reality):** the
plan's IPv6 test YAML used unquoted `address_prefix: 2001:db8::` / `::1`. serde_yaml 0.9
scans a plain scalar ending in a colon-before-newline as a mapping key
(`"mapping values are not allowed in this context"`), so those two literals must be
QUOTED (`"2001:db8::"`, `"::1"`). This is inherent YAML behavior for IPv6 literals ending
in `::`, not a `CidrRange` logic issue; IPv4 literals and Rust `.parse()` literals are
unaffected. No `CidrRange` code changed — only the test YAML was quoted.

**Result:** `cargo test -p envoy-config cidr_range` → **7 passed**.
