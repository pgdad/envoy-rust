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

---

## Task 2 — five enum arms + V-1 fallout across all match sites — ✅ DONE

**Commit:** (see `phase 67.2 task 2`). The load-bearing, atomic task (the reason parent 67 split).
Adding the arms broke the compile at four exhaustive match sites across three crates; all
classified in ONE green commit (D-3.6). NO `_ =>` catch-all anywhere — the compile break was
the forcing function (confirmed: `cargo build --workspace --all-targets` went RED on the missing
variants, then GREEN once every site was classified).

- **Enum arms + dispatch** (`crates/envoy-config/src/bootstrap.rs`):
  `Permission::{DestinationIp(CidrRange), DestinationPort(u16)}`,
  `Principal::{DirectRemoteIp, RemoteIp, SourceIp}(CidrRange)`, each with a `#[serde(rename)]`
  and a line in the hand-rolled `impl_single_key_oneof!` deserializer.
- **Site 1 — `define_rbac_tree_validator!`:** added a trailing `extra_leaves: [ $($leaf),* ]`
  variadic macro param emitting `crate::$node::$leaf(_) => Ok(())`; instantiations pass
  `[DestinationIp, DestinationPort]` / `[DirectRemoteIp, RemoteIp, SourceIp]`.
- **Site 2 — `validate_l4_permission` / `validate_l4_principal`:** widened the L4 allow-list to
  ADMIT the arms (the `header`/`url_path`/`metadata` rejections stay); each CidrRange's width is
  validated via `CidrRange::validate` → `ConfigError::InvalidCidrRange`.
- **Site 3 — HTTP filter `lower_permission` / `lower_principal`** (`crates/envoy-filter/src/rbac.rs`):
  the L4-only arms are rejected fail-loud (`FilterError::InvalidConfig`) — startup-fatal (this runs
  inside `collect::<Result<_,_>>()?` at filter build). Upstream ACCEPTS them in an HTTP rbac filter
  (measured, ADR-0133), so this is a deliberate divergence (ADR-0049 decision-2 (b)), NOT parity.
- **Site 4 — network engine `permission_matches` / `principal_matches`**
  (`crates/envoy-bin/src/network_rbac.rs`): `destination_ip` → `local_addr.ip()`, `destination_port`
  → `local_addr.port()`, the three source-IP arms → `peer_addr.ip()` (one shared expression).
  REMOVED both `#[allow(clippy::only_used_in_recursion)]` attrs (`conn` is now read) and rewrote the
  two stale "threaded through but not read" doc paragraphs + the stale module header.
- **Replaced** the now-stale test `network_rbac_connection_matcher_arms_do_not_exist_yet` with
  `network_rbac_accepts_connection_matcher_arms` (they now deserialize + validate) and added
  `network_rbac_rejects_invalid_cidr_prefix_len`. Added HTTP-reject witnesses
  (`http_rbac_rejects_destination_port_permission`, `http_rbac_rejects_direct_remote_ip_principal`)
  and engine witnesses (`direct_remote_ip_matches_peer`, `destination_port_matches_local_port`).

**Note for the state-4 session:** `envoy-bin` is a BINARY crate (no lib target), so the plan's
`cargo test -p envoy-bin --lib ...` fails with "no library targets"; use `--bins` (or plain
`cargo test -p envoy-bin`). Same applies to Tasks 3/5 commands in the plan.

**Result:** `cargo build --workspace --all-targets` GREEN; `cargo clippy -p envoy-bin --all-targets
-- -D warnings` clean; envoy-config network_rbac (16), envoy-filter http_rbac_rejects (2), envoy-bin
network_rbac (13) all pass.

---

## Task 3 — exhaustive engine backstops (synthetic `ConnectionInfo`) — ✅ DONE

**Commit:** (see `phase 67.2 task 3`). Tests only (`crates/envoy-bin/src/network_rbac.rs`), no impl change.

- Added a `conn2()` helper (peer `192.0.2.5:40000` NOT in 10.0.0.0/8; local port 9999) so no-match
  cases are meaningful.
- `direct_remote_ip_no_match_denies` (peer outside range ⇒ inverse of ALLOW, `denied` ticks);
  `remote_ip_and_source_ip_evaluate_peer_like_direct_remote_ip` (both aliases match/no-match on peer);
  `destination_port_no_match_denies` (local 9999 ≠ 10000); `destination_ip_matches_local_ip`
  (local 127.0.0.1 ∈ 127.0.0.0/8); `combinators_over_new_leaves` (`and_rules` of two destination
  predicates + `not_id` over a non-matching `direct_remote_ip`).

**Result:** `cargo test -p envoy-bin --bins network_rbac` → **18 passed**.

---

## Task 4 — HTTP RBAC rejects every L4-only arm (fail-loud) — ✅ DONE

**Commit:** (see `phase 67.2 task 4`). Tests only (`crates/envoy-filter/src/rbac.rs`).

- Added the remaining per-arm `lower_*` witnesses: `http_rbac_rejects_destination_ip_permission`
  and `http_rbac_rejects_remote_ip_and_source_ip_principals` (all five arms now covered, alongside
  Task 2's `destination_port` / `direct_remote_ip` witnesses).
- Added `http_rbac_build_from_config_rejects_l4_principal_startup_fatal` — mirrors the existing
  `build_from_config_allow_with_header_principal_creates_filter`, proving the rejection is
  STARTUP-FATAL (propagates through `RbacFilter::build_from_config`, not just the private `lower_*`).

**Result:** `cargo test -p envoy-filter http_rbac` → **5 passed**.

---

## Task 5 — end-to-end loopback backstops (real socket, 127.0.0.1) — ✅ DONE

**Commit:** (see `phase 67.2 task 5`). Tests only (`crates/envoy-bin/tests/network_filter_rbac.rs`).
Boots the real `target/debug/envoy-bin` and connects over loopback so `peer_addr`/`local_addr`
are EXACT.

- Added an `allow_rules(permissions, principals)` helper at `rbac_echo_cfg`'s required 16-space
  indentation, and three tests: `direct_remote_ip_loopback_allows_end_to_end` (peer 127.0.0.1 ∈
  127.0.0.0/8 ⇒ ALLOW, echo round-trips `ping`), `direct_remote_ip_non_loopback_denies_end_to_end`
  (peer ∉ 10.0.0.0/8 ⇒ DENY, zero bytes clean EOF), and `destination_port_end_to_end` (rule naming
  the listener's own port ⇒ ALLOW; rule naming a different port ⇒ DENY, zero bytes). All follow the
  ADR-0131 first-byte / 67.1 DENY-wire-shape mechanics of the two named 67.1 tests.

**Two corrections vs the PLAN's Task-5 skeleton (found on contact with reality):**
1. The harness helper is `reserve_port()` (from `mod common`), not the plan's `free_port()`.
2. The plan's skeleton `rules` strings used the engine `cfg()` indentation (`"  action: …"`); the
   integration `rbac_echo_cfg` splices a FULL `rules:` block at 16-space indent (mirroring the
   file's `ALLOW_ALL`/`DENY_ALL` consts). Used the `allow_rules` helper to get it exactly right.

**Result:** `cargo build -p envoy-bin` then `cargo test -p envoy-bin --test network_filter_rbac`
→ **21 passed** (18 pre-existing 67.1 + 3 new).
