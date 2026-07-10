# Phase 67.2 — network RBAC connection-level matcher arms — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the five connection-level RBAC matcher arms (`Principal::{DirectRemoteIp,RemoteIp,SourceIp}`, `Permission::{DestinationIp,DestinationPort}`) plus the `CidrRange` type, wire them into the network `rbac` engine, and absorb the compile-break fallout on the three exhaustive match sites over the shared `Permission`/`Principal` enums — all witnessed in-process (no new differential fixture).

**Architecture:** `CidrRange` and the five enum arms live in `crates/envoy-config` (the shared config model). The **network** `rbac` engine (`crates/envoy-bin/src/network_rbac.rs`) evaluates the new arms against `ConnectionInfo { peer_addr, local_addr }`. The **HTTP** `rbac` filter (`crates/envoy-filter/src/rbac.rs`) — which shares the enums but cannot evaluate L4 attributes at phase-67.2 scope — rejects the new arms **fail-loud** at filter construction. The `67.1` L4 leaf allow-list widens to admit them; the shared `define_rbac_tree_validator!` macro gains a variadic extra-leaf parameter.

**Tech Stack:** Rust (workspace pinned to toolchain `1.95.0`), `serde` / `serde_yaml` 0.9, `thiserror` (library errors), `std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr}`.

## Global Constraints

- **`#![forbid(unsafe_code)]`** holds in every crate root (D-3.8). No `unsafe`.
- **No new crate, no new dependency, no new fuzz target** (SPEC §D7). The pre-existing `parse_bootstrap` fuzz target reaches the new `CidrRange` parser the moment it lands; only a corpus **seed** is added, and its `!`-un-ignore line in `crates/envoy-config/fuzz/.gitignore` must be **proven tracked with `git ls-files`** (memory `fuzz-corpus-seed-gitignored-by-default`). §7.5 gate (d) must be RECORDED EXPLICITLY at state-4.
- **No `_ =>` catch-all** at any of the exhaustive RBAC match sites — the compile break IS the forcing function (SPEC §D3). Every new arm is classified explicitly at every site.
- **NO new differential fixture** (parent PLAN-VERIFY V-4, measured): the IP arms would see this host's Docker bridge IP and `destination_port` a per-proxy `{{PORT}}`. Regression-only: fixtures `0001`–`0073` stay green. New arms are witnessed **in-process** bound to `127.0.0.1`.
- **`cargo build -p envoy-bin` before ANY local differential/integration run** — the harness and the `tests/network_filter_rbac.rs` integration tests execute `target/debug/envoy-bin` (memory `differential-harness-uses-debug-envoy-bin`).
- **`cargo test --workspace --no-fail-fast`** on this dev host (the invariant core of ~5 environmental REDs aborts the bare run at the first failing binary; memory `local-red-set-varies-run-to-run`). **Never pipe a verification run through `tail`** (memory `never-pipe-verification-runs-through-tail`). CI is authoritative.
- **Never weaken a fixture; never trim `tests/conformance/h2spec/known-failures.txt`.**
- **ADR-0133** (this phase) records every PLAN-VERIFY resolution below; cite it in code comments where a decision is non-obvious.

---

## Context every task needs

**The shared enums and their four break sites.** `Permission` and `Principal` (defined in `crates/envoy-config/src/bootstrap.rs:1634` / `:1681`) are shared by the HTTP RBAC filter (`envoy.filters.http.rbac`, `crates/envoy-filter/src/rbac.rs`) and the network RBAC filter (`envoy.filters.network.rbac`, `crates/envoy-bin/src/network_rbac.rs`). Adding a variant to either enum breaks **every exhaustive match** over it. Confirmed exhaustive with NO catch-all at commit `08f820d`:

1. `crates/envoy-config/src/bootstrap.rs:4247` — `define_rbac_tree_validator!` (a single macro instantiated for BOTH enums as `validate_permission_tree` / `validate_principal_tree`).
2. `crates/envoy-config/src/bootstrap.rs:4343` — `validate_l4_permission`; `:4387` — `validate_l4_principal` (the `67.1` D3 L4 leaf allow-list, two hand-written twins).
3. `crates/envoy-filter/src/rbac.rs:262` — `lower_permission`; `:291` — `lower_principal` (HTTP filter, called at filter construction inside `map(lower_permission).collect::<Result<_,_>>()?`, so a new `Err` arm is **startup-fatal**, fail-loud).
4. `crates/envoy-bin/src/network_rbac.rs:118` — `permission_matches`; `:138` — `principal_matches` (network engine).

Until ALL of these compile, `cargo build --workspace` is red. Therefore the enum arms and every site's classification land in **one task** (Task 2) to preserve a green build (D-3.6). Task 1 (CidrRange) precedes it; Tasks 3–6 only ADD tests/docs and do not touch the compile surface.

**PLAN-VERIFY resolutions (measured against `envoyproxy/envoy:v1.33.0` with `--mode validate`; ADR-0133):**

- **X-1 `CidrRange` wire shape.** `address_prefix` is a bare IP string (`"10.0.0.0"`), parsed to `IpAddr`. `prefix_len` is Envoy's `UInt32Value`; the upstream loader accepts BOTH bare `prefix_len: 24` AND wrapper `prefix_len: {value: 24}`. envoy-rust models it as a **bare `u8`** and rejects the wrapper — matching the codebase's established UInt32Value posture (`Buffer::max_request_bytes`, ADR-0063) and the ADR-0049 fail-loud stance. IPv4-mapped-IPv6 peers (`::ffff:127.0.0.1`) are canonicalised to IPv4 before matching, so they match an IPv4 range. **No differential observable.**
- **`destination_port`.** Upstream models it as a plain `uint32` with PGV `lte: 65535` — it REJECTS the `{value:N}` wrapper AND values > 65535 (both measured). Modeling it as a Rust **`u16`** is therefore *exactly faithful*: serde rejects the wrapper (type error) and > 65535 (range error) for free.
- **X-2 `source_ip`.** Upstream accepts it but emits a **deprecated-field warning**; `direct_remote_ip` / `remote_ip` are clean. All three principals evaluate the **downstream connection source** = `peer_addr.ip()` (envoy-rust has no listener filters, so `remote_ip` ≡ `direct_remote_ip` ≡ `source_ip` today). Modeled as three distinct enum variants sharing ONE evaluation expression — not three code paths. envoy-rust does not replicate the deprecation warning (no differential observable); recorded in `BEHAVIOR_CONTRACT.md`.
- **X-3 macro shape.** `define_rbac_tree_validator!` gains a trailing `extra_leaves: [$($leaf:ident),*]` parameter emitting `crate::$node::$leaf(_) => Ok(())` arms. `Permission` instantiation passes `[DestinationIp, DestinationPort]`; `Principal` passes `[DirectRemoteIp, RemoteIp, SourceIp]`. The new arms are all non-recursive leaves, so `Ok(())` is correct (the tree validator only bounds depth + rejects empty sets; leaf validity for CidrRange is handled by the L4 walk).
- **X-4.** `lower_permission` / `lower_principal` are STILL exhaustive with no catch-all (confirmed at `08f820d`). Adding arms breaks the compile — the intended forcing function.
- **X-5.** The six shared RBAC tree/empty-set `ConfigError` variants AND `UnsupportedNetworkRbacMatcher` are ALREADY scope-neutral (`listener {listener:?}`, not `HCM listener`) — generalized by `67.1`'s W-1 (ADR-0130). The new `InvalidCidrRange` variant follows the same scope-neutral shape.
- **D3 correction (material).** Upstream Envoy **ACCEPTS** `destination_port` + `direct_remote_ip` in the HTTP RBAC filter (measured `configuration OK`). So envoy-rust's HTTP filter rejecting the L4 arms is a **deliberate fail-loud divergence** (ADR-0049 decision-2 (b)) — NOT the "symmetric parity" the SPEC §D3 prose implies. Recorded accurately in `BEHAVIOR_CONTRACT.md` and ADR-0133 (D-3.3).

**§6.1 GATE re-derivation (state-2 duty).** This PLAN is **6 tasks / ~695 net LoC** (Task table in each section below). Both are well under the ~25-task / ~1500-LoC thresholds. **The gate does NOT fire.** The mid-execution valve stays ARMED (it fired once this parent, carving `67.3`); if any single task's sub-steps blow past ~10 items on contact with reality, STOP and split per §6.2.

**STALE SPEC header.** `67.2/SPEC.md`'s header ("flips parent row `67` to `done` at its state-6") and its `D8` are STALE — written at the ADR-0129 split, before `67.3` was carved out (ADR-0132). The parent now has `sub-phases = 67.1, 67.2, 67.3` and flips `done` only when ALL THREE are `done`. `67.2`'s state-6 close-out flips ONLY row `67.2` → `done`; it does NOT touch parent row `67`. `ROADMAP.md`/`STATE.md`/`DECISIONS.md` are authoritative over the SPEC header.

---

### Task 1: `CidrRange` type — parse, validate, `contains`

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `CidrRange` + `prefix_match` helper near the `Permission` definition, ~line 1622)
- Modify: `crates/envoy-config/src/lib.rs` (add `ConfigError::InvalidCidrRange`; re-export `CidrRange`)
- Test: `crates/envoy-config/src/bootstrap.rs` (`#[cfg(test)]` — `CidrRange` unit tests)

**Interfaces:**
- Produces:
  - `pub struct CidrRange { pub address_prefix: std::net::IpAddr, pub prefix_len: u8 }` — `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]`, `#[serde(deny_unknown_fields)]`.
  - `pub fn CidrRange::contains(&self, addr: &std::net::IpAddr) -> bool`
  - `pub(crate) fn CidrRange::validate(&self) -> Result<(), String>` — `Err(detail)` when `prefix_len` exceeds the family max (32 / 128).
  - `ConfigError::InvalidCidrRange { listener: String, policy_name: String, path: String, detail: String }`
  - `CidrRange` re-exported from the crate root (so `envoy_config::CidrRange` resolves in `envoy-bin`).

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` in `crates/envoy-config/src/bootstrap.rs` (use whichever test module already covers RBAC parsing — the same one holding `network_rbac_*` tests):

```rust
#[test]
fn cidr_range_contains_ipv4() {
    let c: crate::CidrRange =
        serde_yaml::from_str("address_prefix: 10.1.2.0\nprefix_len: 24").unwrap();
    assert!(c.contains(&"10.1.2.5".parse().unwrap()));
    assert!(c.contains(&"10.1.2.255".parse().unwrap()));
    assert!(!c.contains(&"10.1.3.0".parse().unwrap()));
    assert!(!c.contains(&"11.1.2.5".parse().unwrap()));
}

#[test]
fn cidr_range_boundary_prefix_lengths_v4() {
    // /0 matches everything; /32 is an exact host match.
    let all: crate::CidrRange =
        serde_yaml::from_str("address_prefix: 0.0.0.0\nprefix_len: 0").unwrap();
    assert!(all.contains(&"203.0.113.9".parse().unwrap()));
    let host: crate::CidrRange =
        serde_yaml::from_str("address_prefix: 192.0.2.7\nprefix_len: 32").unwrap();
    assert!(host.contains(&"192.0.2.7".parse().unwrap()));
    assert!(!host.contains(&"192.0.2.8".parse().unwrap()));
}

#[test]
fn cidr_range_ipv6_and_128() {
    let c: crate::CidrRange =
        serde_yaml::from_str("address_prefix: 2001:db8::\nprefix_len: 32").unwrap();
    assert!(c.contains(&"2001:db8:1234::1".parse().unwrap()));
    assert!(!c.contains(&"2001:db9::1".parse().unwrap()));
    let host: crate::CidrRange =
        serde_yaml::from_str("address_prefix: ::1\nprefix_len: 128").unwrap();
    assert!(host.contains(&"::1".parse().unwrap()));
    assert!(!host.contains(&"::2".parse().unwrap()));
}

#[test]
fn cidr_range_ipv4_mapped_ipv6_peer_matches_ipv4_range() {
    // ADR-0133: a v4-mapped-v6 peer is canonicalised to IPv4 before matching.
    let c: crate::CidrRange =
        serde_yaml::from_str("address_prefix: 127.0.0.0\nprefix_len: 8").unwrap();
    assert!(c.contains(&"::ffff:127.0.0.1".parse().unwrap()));
}

#[test]
fn cidr_range_cross_family_never_matches() {
    let v4: crate::CidrRange =
        serde_yaml::from_str("address_prefix: 10.0.0.0\nprefix_len: 8").unwrap();
    assert!(!v4.contains(&"2001:db8::1".parse().unwrap()));
}

#[test]
fn cidr_range_validate_rejects_over_wide_prefix() {
    let bad_v4: crate::CidrRange =
        serde_yaml::from_str("address_prefix: 10.0.0.0\nprefix_len: 33").unwrap();
    assert!(bad_v4.validate().is_err());
    let bad_v6: crate::CidrRange =
        serde_yaml::from_str("address_prefix: 2001:db8::\nprefix_len: 129").unwrap();
    assert!(bad_v6.validate().is_err());
    let ok: crate::CidrRange =
        serde_yaml::from_str("address_prefix: 10.0.0.0\nprefix_len: 32").unwrap();
    assert!(ok.validate().is_ok());
}

#[test]
fn cidr_range_rejects_unknown_field_and_wrapper_prefix_len() {
    // deny_unknown_fields + bare-u8 prefix_len (ADR-0133 divergence: Envoy also
    // accepts `prefix_len: {value: N}`, envoy-rust rejects it fail-loud).
    assert!(serde_yaml::from_str::<crate::CidrRange>(
        "address_prefix: 10.0.0.0\nprefix_len: 8\nextra: 1"
    )
    .is_err());
    assert!(serde_yaml::from_str::<crate::CidrRange>(
        "address_prefix: 10.0.0.0\nprefix_len: {value: 8}"
    )
    .is_err());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config cidr_range 2>&1 | tail -20`
Expected: FAIL — `cannot find type CidrRange` (type does not exist yet).

- [ ] **Step 3: Implement `CidrRange`**

Add near `crates/envoy-config/src/bootstrap.rs:1622` (just above the `Permission` enum):

```rust
/// Envoy `config.core.v3.CidrRange` — an IP prefix. `address_prefix` is a bare
/// IP string (`"10.0.0.0"`) parsed to `IpAddr`; `prefix_len` is the mask width.
///
/// WIRE-SHAPE (67.2 PLAN-VERIFY X-1, ADR-0133; measured against
/// `envoyproxy/envoy:v1.33.0` with `--mode validate`): `prefix_len` is Envoy's
/// `UInt32Value`, which upstream accepts as EITHER a bare integer
/// (`prefix_len: 24`) OR the wrapper (`prefix_len: {value: 24}`). envoy-rust
/// models it as a bare `u8` and REJECTS the wrapper — matching the codebase's
/// established UInt32Value posture (`Buffer::max_request_bytes`, ADR-0063) and
/// the ADR-0049 fail-loud stance. `prefix_len` is REQUIRED (absent → serde
/// missing-field → fatal); upstream defaults an absent `prefix_len` to 0, a
/// documented divergence with no differential observable (67.2 ships no fixture).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CidrRange {
    pub address_prefix: std::net::IpAddr,
    pub prefix_len: u8,
}

impl CidrRange {
    /// Validate the prefix width against the address family: ≤ 32 for IPv4,
    /// ≤ 128 for IPv6. `Err(detail)` is mapped to `ConfigError::InvalidCidrRange`
    /// by the L4 allow-list walk (`validate_l4_permission` / `_principal`).
    pub(crate) fn validate(&self) -> Result<(), String> {
        let (max, family) = match self.address_prefix {
            std::net::IpAddr::V4(_) => (32u8, "IPv4"),
            std::net::IpAddr::V6(_) => (128u8, "IPv6"),
        };
        if self.prefix_len > max {
            return Err(format!(
                "prefix_len {} exceeds {} for {}",
                self.prefix_len, max, family
            ));
        }
        Ok(())
    }

    /// Does `addr` fall inside this prefix? IPv4-mapped-IPv6 addresses are
    /// canonicalised to IPv4 first (ADR-0133), so `::ffff:127.0.0.1` matches an
    /// IPv4 `127.0.0.0/8` range — upstream Envoy's behavior. After
    /// canonicalisation, a cross-family comparison never matches.
    pub fn contains(&self, addr: &std::net::IpAddr) -> bool {
        fn canonical(ip: std::net::IpAddr) -> std::net::IpAddr {
            match ip {
                std::net::IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
                    Some(v4) => std::net::IpAddr::V4(v4),
                    None => std::net::IpAddr::V6(v6),
                },
                v4 => v4,
            }
        }
        match (canonical(self.address_prefix), canonical(*addr)) {
            (std::net::IpAddr::V4(net), std::net::IpAddr::V4(a)) => {
                prefix_match(&net.octets(), &a.octets(), self.prefix_len)
            }
            (std::net::IpAddr::V6(net), std::net::IpAddr::V6(a)) => {
                prefix_match(&net.octets(), &a.octets(), self.prefix_len)
            }
            _ => false,
        }
    }
}

/// Compare the leading `prefix_len` bits of two same-length octet slices.
/// `prefix_len == 0` matches everything; a full-byte boundary skips the mask.
fn prefix_match(net: &[u8], addr: &[u8], prefix_len: u8) -> bool {
    let bits = prefix_len as usize;
    let full = bits / 8;
    if net[..full] != addr[..full] {
        return false;
    }
    let rem = bits % 8;
    if rem == 0 {
        return true;
    }
    let mask = 0xff_u8 << (8 - rem);
    (net[full] & mask) == (addr[full] & mask)
}
```

Add the `ConfigError` variant to `crates/envoy-config/src/lib.rs` (in the RBAC scope-neutral group, just after `UnsupportedNetworkRbacMatcher`):

```rust
    /// 67.2 (ADR-0133): a `CidrRange` in a NETWORK rbac policy has an invalid
    /// prefix width for its address family (`prefix_len > 32` on IPv4 /
    /// `> 128` on IPv6). Config-load-time fatal (ADR-0049). Scope-neutral
    /// `listener {listener:?}` per the 67.1 W-1 generalization (ADR-0130).
    #[error(
        "listener {listener:?}: network rbac policy {policy_name:?} has an invalid CidrRange at {path}: {detail}"
    )]
    InvalidCidrRange {
        listener: String,
        policy_name: String,
        path: String,
        detail: String,
    },
```

Ensure `CidrRange` is re-exported from the crate root the same way `Permission` / `Principal` are (add to the `pub use bootstrap::{...}` list in `crates/envoy-config/src/lib.rs`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-config cidr_range 2>&1 | tail -20`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 67.2 task 1: CidrRange type + contains + prefix validation [ADR-0133]"
```

---

### Task 2: The five enum arms + the V-1 fallout across all match sites

> This is the load-bearing task and the reason parent `67` split. Adding the arms breaks the compile at four match sites across three crates; ALL must be classified for `cargo build --workspace` to go green (D-3.6). Keep it atomic. If sub-steps blow past ~10 items on contact with reality, STOP and split per §6.2 (the mid-execution valve is ARMED).

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`Permission` `:1634` + dispatch `:1653`; `Principal` `:1681` + dispatch `:1701`; `define_rbac_tree_validator!` `:4247` + its two instantiations `:4310`/`:4321`; `validate_l4_permission` `:4343`; `validate_l4_principal` `:4387`; **update** the now-stale test `network_rbac_connection_matcher_arms_do_not_exist_yet` `:5905`)
- Modify: `crates/envoy-filter/src/rbac.rs` (`lower_permission` `:262`; `lower_principal` `:291`)
- Modify: `crates/envoy-bin/src/network_rbac.rs` (`permission_matches` `:118`; `principal_matches` `:138`)
- Test: the three modified crates' `#[cfg(test)]` modules (one witness per site; exhaustive coverage is Tasks 3–5)

**Interfaces:**
- Consumes: `CidrRange`, `CidrRange::validate`, `ConfigError::InvalidCidrRange` (Task 1).
- Produces:
  - `Permission::DestinationIp(CidrRange)`, `Permission::DestinationPort(u16)`
  - `Principal::DirectRemoteIp(CidrRange)`, `Principal::RemoteIp(CidrRange)`, `Principal::SourceIp(CidrRange)`
  - network engine: `permission_matches` / `principal_matches` evaluate the arms (consumed by Tasks 3 & 5).
  - HTTP filter: `lower_permission` / `lower_principal` reject the arms with `FilterError::InvalidConfig` (consumed by Task 4).

- [ ] **Step 1: Write the failing tests (one witness per new behavior)**

(a) In `crates/envoy-config/src/bootstrap.rs` tests — REPLACE `network_rbac_connection_matcher_arms_do_not_exist_yet` (it asserted the arms do NOT deserialize; they now do). New test:

```rust
/// 67.2 D2/D4: the connection-level arms now EXIST, deserialize, and pass the
/// widened L4 allow-list. (Supersedes `..._do_not_exist_yet`, which pinned the
/// pre-67.2 unknown-key rejection.)
#[test]
fn network_rbac_accepts_connection_matcher_arms() {
    for arm in ["direct_remote_ip", "remote_ip", "source_ip"] {
        let mut b = network_rbac_bootstrap(&format!(
            "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{{ any: true }}]\n                      principals: [{{ {arm}: {{ address_prefix: 10.0.0.0, prefix_len: 8 }} }}]"
        ));
        validate(&mut b).unwrap_or_else(|e| panic!("{arm} must validate: {e:?}"));
    }
    let mut b = network_rbac_bootstrap(
        "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{ destination_port: 8080 }, { destination_ip: { address_prefix: 127.0.0.0, prefix_len: 8 } }]\n                      principals: [{ any: true }]",
    );
    validate(&mut b).expect("destination_port + destination_ip must validate");
}

/// 67.2 D1/D4: an out-of-range prefix in a network rbac CidrRange is rejected
/// with `InvalidCidrRange`, naming the policy + path.
#[test]
fn network_rbac_rejects_invalid_cidr_prefix_len() {
    let mut b = network_rbac_bootstrap(
        "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{ any: true }]\n                      principals: [{ direct_remote_ip: { address_prefix: 10.0.0.0, prefix_len: 99 } }]",
    );
    let err = validate(&mut b).expect_err("prefix_len 99 on IPv4 is invalid");
    assert!(
        matches!(err, crate::ConfigError::InvalidCidrRange { ref policy_name, ref path, .. }
            if policy_name == "p0" && path == "principals[0]"),
        "got {err:?}",
    );
}
```

(b) In `crates/envoy-filter/src/rbac.rs` tests — the HTTP filter rejects each L4-only arm at construction (add to the existing `#[cfg(test)] mod tests`). Use the crate's established filter-build helper; if the test module lacks one, build the `RbacFilter` directly the way the neighbouring tests do. One representative witness here (exhaustive per-arm coverage is Task 4):

```rust
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
```

(c) In `crates/envoy-bin/src/network_rbac.rs` tests — the engine evaluates the arms over a synthetic `ConnectionInfo` (one witness here; exhaustive coverage is Task 3). The existing `conn()` helper yields `peer_addr 10.0.0.1:54321`, `local_addr 127.0.0.1:10000`:

```rust
/// 67.2 D5: direct_remote_ip matches the connection's source IP (peer_addr).
#[test]
fn direct_remote_ip_matches_peer() {
    let reg = envoy_stats::StatsRegistry::new();
    let f = NetworkRbacFilter::new(
        &cfg(
            "dr",
            Some(
                "  action: ALLOW\n  policies:\n    p0:\n      permissions: [{ any: true }]\n      principals: [{ direct_remote_ip: { address_prefix: 10.0.0.0, prefix_len: 8 } }]",
            ),
        ),
        &reg,
    )
    .unwrap();
    assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue);
    assert_eq!(stat(&reg, "dr.rbac.allowed"), 1);
}

/// 67.2 D5: destination_port matches the listener's local port.
#[test]
fn destination_port_matches_local_port() {
    let reg = envoy_stats::StatsRegistry::new();
    let f = NetworkRbacFilter::new(
        &cfg(
            "dp",
            Some("  action: ALLOW\n  policies:\n    p0:\n      permissions: [{ destination_port: 10000 }]\n      principals: [{ any: true }]"),
        ),
        &reg,
    )
    .unwrap();
    assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue);
    assert_eq!(stat(&reg, "dp.rbac.allowed"), 1);
}
```

- [ ] **Step 2: Run to verify they fail (compile error — the forcing function)**

Run: `cargo build --workspace --all-targets 2>&1 | grep -E "non-exhaustive|not covered|error\[" | head`
Expected: FAIL — non-exhaustive `match` at `define_rbac_tree_validator!`, `validate_l4_*`, `lower_*`, `permission_matches`, `principal_matches` (the compile break is intended).

- [ ] **Step 3: Add the enum arms + dispatch lines** (`crates/envoy-config/src/bootstrap.rs`)

`Permission` enum (after `UrlPath`, `:1650`):

```rust
    /// 67.2: L4 destination IP prefix. Evaluated against `local_addr.ip()` by the
    /// network engine; REJECTED fail-loud by the HTTP filter (ADR-0133).
    #[serde(rename = "destination_ip")]
    DestinationIp(CidrRange),
    /// 67.2: L4 destination port. Plain `uint32` upstream with PGV `lte: 65535`,
    /// so a bare `u16` is exactly faithful (rejects the wrapper AND > 65535).
    #[serde(rename = "destination_port")]
    DestinationPort(u16),
```

`Permission` dispatch (`impl_single_key_oneof!`, after the `url_path` line `:1664`):

```rust
        "destination_ip" => Permission::DestinationIp(map.next_value::<CidrRange>()?),
        "destination_port" => Permission::DestinationPort(map.next_value::<u16>()?),
```

`Principal` enum (after `UrlPath`, `:1698`):

```rust
    /// 67.2: the downstream connection source IP (`peer_addr.ip()`). `remote_ip`
    /// and `source_ip` coincide with this today (no listener filters, ADR-0133).
    #[serde(rename = "direct_remote_ip")]
    DirectRemoteIp(CidrRange),
    #[serde(rename = "remote_ip")]
    RemoteIp(CidrRange),
    /// Deprecated alias of `direct_remote_ip` upstream (emits a deprecation
    /// warning); identical evaluation here. ADR-0133 / SPEC X-2.
    #[serde(rename = "source_ip")]
    SourceIp(CidrRange),
```

`Principal` dispatch (after the `url_path` line `:1712`):

```rust
        "direct_remote_ip" => Principal::DirectRemoteIp(map.next_value::<CidrRange>()?),
        "remote_ip" => Principal::RemoteIp(map.next_value::<CidrRange>()?),
        "source_ip" => Principal::SourceIp(map.next_value::<CidrRange>()?),
```

- [ ] **Step 4: Extend `define_rbac_tree_validator!` with `extra_leaves`** (`crates/envoy-config/src/bootstrap.rs:4247`)

Add a trailing macro parameter and emit `Ok(())` arms for the extra leaves. In the macro signature, after `$empty_err:ident`:

```rust
        $empty_err:ident,
        extra_leaves: [ $($leaf:ident),* $(,)? ]
```

In the macro body's `match node { ... }`, just before the closing brace (after the `UrlPath` arm `:4304`):

```rust
                $( crate::$node::$leaf(_) => Ok(()), )*
```

Update both instantiations:

```rust
define_rbac_tree_validator!(
    validate_permission_tree,
    Permission,
    AndRules | OrRules,
    NotRule,
    rules,
    "{path}.rules[{idx}]",
    "{path}.not_rule",
    EmptyRbacPermissionSet,
    extra_leaves: [DestinationIp, DestinationPort]
);

define_rbac_tree_validator!(
    validate_principal_tree,
    Principal,
    AndIds | OrIds,
    NotId,
    ids,
    "{path}.ids[{idx}]",
    "{path}.not_id",
    EmptyRbacPrincipalSet,
    extra_leaves: [DirectRemoteIp, RemoteIp, SourceIp]
);
```

- [ ] **Step 5: Widen the L4 leaf allow-list** (`crates/envoy-config/src/bootstrap.rs`)

`validate_l4_permission` — add before the `AndRules | OrRules` arm:

```rust
        crate::Permission::DestinationPort(_) => Ok(()),
        crate::Permission::DestinationIp(cidr) => {
            cidr.validate().map_err(|detail| crate::ConfigError::InvalidCidrRange {
                listener: listener_name.to_string(),
                policy_name: policy_name.to_string(),
                path: path.to_string(),
                detail,
            })
        }
```

`validate_l4_principal` — add the symmetric arm:

```rust
        crate::Principal::DirectRemoteIp(cidr)
        | crate::Principal::RemoteIp(cidr)
        | crate::Principal::SourceIp(cidr) => {
            cidr.validate().map_err(|detail| crate::ConfigError::InvalidCidrRange {
                listener: listener_name.to_string(),
                policy_name: policy_name.to_string(),
                path: path.to_string(),
                detail,
            })
        }
```

- [ ] **Step 6: Reject the L4 arms in the HTTP filter** (`crates/envoy-filter/src/rbac.rs`)

`lower_permission` — add inside the `Ok(match p { ... })`:

```rust
        envoy_config::Permission::DestinationIp(_) | envoy_config::Permission::DestinationPort(_) => {
            return Err(FilterError::InvalidConfig {
                message: "envoy.filters.http.rbac: destination_ip / destination_port are \
                          L4-only matchers, unsupported in the HTTP RBAC filter (ADR-0133)"
                    .into(),
            });
        }
```

`lower_principal` — add:

```rust
        envoy_config::Principal::DirectRemoteIp(_)
        | envoy_config::Principal::RemoteIp(_)
        | envoy_config::Principal::SourceIp(_) => {
            return Err(FilterError::InvalidConfig {
                message: "envoy.filters.http.rbac: direct_remote_ip / remote_ip / source_ip are \
                          connection-level matchers, unsupported in the HTTP RBAC filter (ADR-0133)"
                    .into(),
            });
        }
```

- [ ] **Step 7: Implement the network engine arms** (`crates/envoy-bin/src/network_rbac.rs`)

`permission_matches` (`:118`) — REMOVE the `#[allow(clippy::only_used_in_recursion)]` (conn is now read) and add:

```rust
        Permission::DestinationIp(cidr) => cidr.contains(&conn.local_addr.ip()),
        Permission::DestinationPort(port) => conn.local_addr.port() == *port,
```

`principal_matches` (`:138`) — REMOVE the `#[allow(clippy::only_used_in_recursion)]` and add:

```rust
        Principal::DirectRemoteIp(cidr) | Principal::RemoteIp(cidr) | Principal::SourceIp(cidr) => {
            cidr.contains(&conn.peer_addr.ip())
        }
```

Update each function's doc comment: the `conn` argument is NO LONGER "threaded through but not read" — the new arms read it. Fix the two stale doc paragraphs accordingly.

- [ ] **Step 8: Build green, then run the witnesses**

Run: `cargo build --workspace --all-targets 2>&1 | tail -5`
Expected: builds clean (no non-exhaustive-match errors).

Run: `cargo test -p envoy-config network_rbac_accepts network_rbac_rejects_invalid_cidr && cargo test -p envoy-filter http_rbac_rejects && cargo build -p envoy-bin && cargo test -p envoy-bin --lib direct_remote_ip_matches_peer destination_port_matches_local_port 2>&1 | tail -25`
Expected: PASS.

Run (clippy, since `#[allow]` attrs were removed): `cargo clippy -p envoy-bin --all-targets -- -D warnings 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-filter/src/rbac.rs crates/envoy-bin/src/network_rbac.rs
git commit -m "phase 67.2 task 2: five connection-level matcher arms + V-1 shared-enum fallout [ADR-0133]"
```

---

### Task 3: Exhaustive engine backstops (synthetic `ConnectionInfo`)

**Files:**
- Test: `crates/envoy-bin/src/network_rbac.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: the engine arms + `cfg()` / `conn()` / `stat()` helpers (Task 2).

- [ ] **Step 1: Write the tests** — match/no-match for every arm + combinators over the new leaves. Add a second connection helper so no-match cases are meaningful:

```rust
/// A connection whose peer is NOT in 10.0.0.0/8 and whose local port is 9999.
fn conn2() -> ConnectionInfo {
    ConnectionInfo {
        peer_addr: "192.0.2.5:40000".parse().unwrap(),
        local_addr: "127.0.0.1:9999".parse().unwrap(),
    }
}

#[test]
fn direct_remote_ip_no_match_denies() {
    let reg = envoy_stats::StatsRegistry::new();
    let f = NetworkRbacFilter::new(
        &cfg("drn", Some("  action: ALLOW\n  policies:\n    p0:\n      permissions: [{ any: true }]\n      principals: [{ direct_remote_ip: { address_prefix: 10.0.0.0, prefix_len: 8 } }]")),
        &reg,
    ).unwrap();
    // conn2's peer 192.0.2.5 is NOT in 10.0.0.0/8 ⇒ no policy match ⇒ inverse of ALLOW.
    assert_eq!(f.on_new_connection(&conn2()), NetworkFilterStatus::StopIteration);
    assert_eq!(stat(&reg, "drn.rbac.denied"), 1);
}

#[test]
fn remote_ip_and_source_ip_evaluate_peer_like_direct_remote_ip() {
    for arm in ["remote_ip", "source_ip"] {
        let reg = envoy_stats::StatsRegistry::new();
        let f = NetworkRbacFilter::new(
            &cfg("ali", Some(&format!("  action: ALLOW\n  policies:\n    p0:\n      permissions: [{{ any: true }}]\n      principals: [{{ {arm}: {{ address_prefix: 10.0.0.0, prefix_len: 8 }} }}]"))),
            &reg,
        ).unwrap();
        // conn()'s peer 10.0.0.1 IS in 10.0.0.0/8.
        assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue, "{arm} matches peer");
        assert_eq!(f.on_new_connection(&conn2()), NetworkFilterStatus::StopIteration, "{arm} no-match");
    }
}

#[test]
fn destination_port_no_match_denies() {
    let reg = envoy_stats::StatsRegistry::new();
    let f = NetworkRbacFilter::new(
        &cfg("dpn", Some("  action: ALLOW\n  policies:\n    p0:\n      permissions: [{ destination_port: 10000 }]\n      principals: [{ any: true }]")),
        &reg,
    ).unwrap();
    assert_eq!(f.on_new_connection(&conn2()), NetworkFilterStatus::StopIteration); // local port 9999 != 10000
}

#[test]
fn destination_ip_matches_local_ip() {
    let reg = envoy_stats::StatsRegistry::new();
    let f = NetworkRbacFilter::new(
        &cfg("di", Some("  action: ALLOW\n  policies:\n    p0:\n      permissions: [{ destination_ip: { address_prefix: 127.0.0.0, prefix_len: 8 } }]\n      principals: [{ any: true }]")),
        &reg,
    ).unwrap();
    assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue); // local 127.0.0.1
    assert_eq!(stat(&reg, "di.rbac.allowed"), 1);
}

#[test]
fn combinators_over_new_leaves() {
    // not_id over a non-matching direct_remote_ip ⇒ matches; and_rules of two
    // destination predicates.
    let reg = envoy_stats::StatsRegistry::new();
    let f = NetworkRbacFilter::new(
        &cfg("cmb", Some(
            "  action: ALLOW\n  policies:\n    p0:\n      permissions:\n        - and_rules:\n            rules:\n              - destination_port: 10000\n              - destination_ip: { address_prefix: 127.0.0.0, prefix_len: 8 }\n      principals:\n        - not_id: { direct_remote_ip: { address_prefix: 192.0.2.0, prefix_len: 24 } }")),
        &reg,
    ).unwrap();
    // conn(): local 127.0.0.1:10000 (both perms match); peer 10.0.0.1 NOT in 192.0.2.0/24 ⇒ not_id true.
    assert_eq!(f.on_new_connection(&conn()), NetworkFilterStatus::Continue);
}
```

- [ ] **Step 2: Run** — `cargo build -p envoy-bin && cargo test -p envoy-bin --lib 2>&1 | tail -20`. Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/envoy-bin/src/network_rbac.rs
git commit -m "phase 67.2 task 3: exhaustive network-rbac engine backstops for the L4 arms"
```

---

### Task 4: HTTP RBAC rejects every L4-only arm (fail-loud)

**Files:**
- Test: `crates/envoy-filter/src/rbac.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `lower_permission` / `lower_principal` reject arms (Task 2).

- [ ] **Step 1: Write the tests** — one per arm, plus a construction-level test proving the rejection is startup-fatal (propagates through the filter builder, not just the private `lower_*` helpers):

```rust
#[test]
fn http_rbac_rejects_destination_ip_permission() {
    let cidr = serde_yaml::from_str::<envoy_config::CidrRange>("address_prefix: 10.0.0.0\nprefix_len: 8").unwrap();
    assert!(matches!(
        lower_permission(&envoy_config::Permission::DestinationIp(cidr)),
        Err(FilterError::InvalidConfig { .. })
    ));
}

#[test]
fn http_rbac_rejects_remote_ip_and_source_ip_principals() {
    for ctor in [
        envoy_config::Principal::RemoteIp as fn(envoy_config::CidrRange) -> envoy_config::Principal,
        envoy_config::Principal::SourceIp,
    ] {
        let cidr = serde_yaml::from_str::<envoy_config::CidrRange>("address_prefix: 10.0.0.0\nprefix_len: 8").unwrap();
        assert!(matches!(lower_principal(&ctor(cidr)), Err(FilterError::InvalidConfig { .. })));
    }
}
```

Plus a full-filter-construction witness using this crate's existing `RbacFilter` builder + a stats registry (mirror the closest existing construction test; the config carries `principals: [{ direct_remote_ip: { address_prefix: 10.0.0.0, prefix_len: 8 } }]` and construction must return `Err`). If no builder test exists to mirror, keep the two `lower_*` unit tests above — they already prove the fail-loud classification at the site that construction calls.

- [ ] **Step 2: Run** — `cargo test -p envoy-filter http_rbac_rejects 2>&1 | tail -15`. Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/envoy-filter/src/rbac.rs
git commit -m "phase 67.2 task 4: HTTP RBAC rejects the L4-only arms fail-loud [ADR-0133]"
```

---

### Task 5: End-to-end integration backstops (real socket, 127.0.0.1)

> Boots the real `target/debug/envoy-bin` via `spawn_envoy_bin` and connects over a loopback socket, so `peer_addr` / `local_addr` are EXACT — the in-process witness the SPEC's §2 rationale requires (no differential fixture). Extends the `67.1` harness in `tests/network_filter_rbac.rs`.

**Files:**
- Test: `crates/envoy-bin/tests/network_filter_rbac.rs`

**Interfaces:**
- Consumes: `spawn_envoy_bin`, `rbac_echo_cfg(port, stat_prefix, rules_block)`, and the `free_port` / connect helpers already in this file (67.1). Reuse them; do not re-implement.

- [ ] **Step 1: Write the tests** — an ALLOW and a DENY end-to-end for a peer-IP arm and for a port arm. The client connects from loopback, so `peer_addr.ip() == 127.0.0.1`; the listener binds the chosen `port`, so `local_addr.port() == port`. Follow the exact shape of `allow_yields_to_the_terminal_echo_filter` / `deny_writes_zero_bytes_and_closes_cleanly_discarding_client_bytes` (write a probe byte first — `ONE_TIME_ON_FIRST_BYTE`, ADR-0131 — then read to EOF):

```rust
/// 67.2 D6: direct_remote_ip 127.0.0.0/8 matches a loopback client ⇒ ALLOW,
/// the echo terminal round-trips the payload.
#[tokio::test]
async fn direct_remote_ip_loopback_allows_end_to_end() {
    let port = free_port();
    let rules = "  action: ALLOW\n              policies:\n                p0:\n                  permissions: [{ any: true }]\n                  principals: [{ direct_remote_ip: { address_prefix: 127.0.0.0, prefix_len: 8 } }]";
    let (_child, _tmp) = spawn_envoy_bin(&rbac_echo_cfg(port, "dr", rules));
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    // ... connect, write b"ping", read_to_end, assert echoed b"ping" ...
}

/// 67.2 D6: a direct_remote_ip range that EXCLUDES loopback ⇒ DENY, zero bytes,
/// clean EOF (the 67.1 DENY wire shape).
#[tokio::test]
async fn direct_remote_ip_non_loopback_denies_end_to_end() {
    let port = free_port();
    let rules = "  action: ALLOW\n              policies:\n                p0:\n                  permissions: [{ any: true }]\n                  principals: [{ direct_remote_ip: { address_prefix: 10.0.0.0, prefix_len: 8 } }]";
    let (_child, _tmp) = spawn_envoy_bin(&rbac_echo_cfg(port, "dr2", rules));
    // ... connect, write b"ping", read_to_end, assert 0 bytes read (DENY) ...
}

/// 67.2 D6: destination_port bound to the listener port ⇒ ALLOW; a wrong port ⇒ DENY.
#[tokio::test]
async fn destination_port_end_to_end() {
    let port = free_port();
    let allow_rules = format!("  action: ALLOW\n              policies:\n                p0:\n                  permissions: [{{ destination_port: {port} }}]\n                  principals: [{{ any: true }}]");
    // ... spawn, connect, write, assert echo round-trips ...
    let wrong = free_port();
    let deny_rules = format!("  action: ALLOW\n              policies:\n                p0:\n                  permissions: [{{ destination_port: {wrong} }}]\n                  principals: [{{ any: true }}]");
    // ... spawn on `port` (listener), rule names `wrong` ⇒ DENY ...
}
```

Fill the `// ...` bodies by copying the connect / write / `read_to_end` / assertion mechanics verbatim from the two named `67.1` tests in this same file. Match `rbac_echo_cfg`'s exact indentation for the `rules_block` (it is spliced into a YAML template — mis-indentation is the most likely failure).

- [ ] **Step 2: Run** — `cargo build -p envoy-bin && cargo test -p envoy-bin --test network_filter_rbac 2>&1 | tail -25`. Expected: PASS. (If a test flakes on port reuse under parallel load, rerun in isolation — memory `differential-fixtures-flake-under-parallel-load`.)

- [ ] **Step 3: Commit**

```bash
git add crates/envoy-bin/tests/network_filter_rbac.rs
git commit -m "phase 67.2 task 5: end-to-end loopback backstops for the L4 matcher arms"
```

---

### Task 6: `BEHAVIOR_CONTRACT.md` + fuzz corpus seed

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (extend the `envoy.filters.network.rbac` section)
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/rbac_cidr_range_seed` (or the seed layout matching the existing `parse_bootstrap` corpus)
- Modify: `crates/envoy-config/fuzz/.gitignore` (add the `!`-un-ignore line for the seed)

- [ ] **Step 1: Document the arms in `BEHAVIOR_CONTRACT.md`.** Under the `envoy.filters.network.rbac` heading, add a numbered item covering: the five arms and what each evaluates against (`direct_remote_ip`/`remote_ip`/`source_ip` → `peer_addr.ip()`; `destination_ip` → `local_addr.ip()`; `destination_port` → `local_addr.port()`); the `remote_ip` ≡ `direct_remote_ip` ≡ `source_ip` equivalence today (no listener filters); `source_ip` is a deprecated upstream alias (envoy-rust does not replicate the warning); the `prefix_len` bare-`u8` vs wrapper divergence (X-1); the IPv4-mapped-IPv6 canonicalisation; and — explicitly, correcting the SPEC's "parity" wording — that the **HTTP RBAC filter rejecting these L4 arms is a deliberate fail-loud divergence** (upstream ACCEPTS them, measured), per ADR-0049 decision-2 (b). Cite ADR-0133 throughout. Also update item **11** of that section (which currently says the arms "do not exist… the parser rejects them as unknown keys") to note they now exist as of 67.2.

- [ ] **Step 2: Add the fuzz corpus seed.** Create a seed input for the pre-existing `parse_bootstrap` target that exercises a network `rbac` `typed_config` carrying a `CidrRange` (a full minimal bootstrap YAML with `[rbac, echo]` and a `direct_remote_ip` principal). Add the `!`-un-ignore line to `crates/envoy-config/fuzz/.gitignore` (match the existing corpus-seed convention there — inspect it first).

- [ ] **Step 3: Prove the seed is tracked**

Run: `git add -A crates/envoy-config/fuzz && git ls-files crates/envoy-config/fuzz/corpus | grep rbac_cidr`
Expected: the seed path is listed (memory `fuzz-corpus-seed-gitignored-by-default` — an un-tracked seed is invisible to CI).

- [ ] **Step 4: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/envoy-config/fuzz
git commit -m "phase 67.2 task 6: BEHAVIOR_CONTRACT rows + parse_bootstrap CidrRange corpus seed [ADR-0133]"
```

---

## State-4 verification checklist (next-but-one session; recorded here so it is not skipped)

Per §7.5 (the six-part gate), the state-4 session runs and quotes into `PROGRESS.md`:

- `cargo build --workspace --all-targets`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` (watch the two removed `#[allow(clippy::only_used_in_recursion)]` attrs)
- `cargo fmt --all -- --check`
- `cargo test --workspace --no-fail-fast` (the ~5 environmental REDs are CI-authoritative; cross-check `local passed+failed == CI passed`)
- `cargo deny check` (patch-bump on an unrelated fresh advisory, don't treat as a regression — memory `cargo-deny-reds-on-unrelated-advisory`)
- **§7.5 gate (d) — fuzz — RECORD EXPLICITLY:** no NEW fuzz target; the pre-existing `parse_bootstrap` target reaches the new `CidrRange` parser. Run it short-budget in CI and state that gate (d) is satisfied by the corpus seed, not passed over in silence (memory `new-fuzz-target-needs-a-ci-yml-step` — there is no new target to wire, but the state-4 record must say so).
- **Differential surface: regression-only** — fixtures `0001`–`0073` stay green (no new fixture). Rebuild the debug binary first (`cargo build -p envoy-bin`).
- Conformance: unchanged; never trim `known-failures.txt`.

## Out of scope (do NOT touch — carried forward)

- `shadow_rules` / shadow counters (**CF-67-1**); `Action::LOG` (**CF-67-2**); `on_data`-time iteration + buffering (**CF-67-3**); `authenticated` / `filter_state` / `requested_server_name` principals.
- The `ConnectionHandler` establishment/data-phase split, the `[rbac, tcp_proxy]` composition, `UnsupportedNetworkFilterChainComposition` deletion — ALL owned by **`67.3`** (ADR-0132). Do not touch.
- A differential fixture for the IP/port arms (structurally host-dependent — parent V-4).
- **Parent row `67`** — `67.2`'s state-6 close-out flips ONLY row `67.2` → `done`; `67.3` still remains before parent `67` flips `done` (STALE SPEC header notwithstanding).

## Self-review (run against `67.2/SPEC.md`)

- **D1** → Task 1. **D2** → Task 2 (steps 3). **D3** (V-1 fallout, all three sites) → Task 2 (steps 4/5/6) + the fail-loud correction recorded (Task 6). **D4** (widen L4 allow-list) → Task 2 (step 5). **D5** (engine arms) → Task 2 (step 7). **D6** (backstops: engine unit + HTTP-reject + CidrRange unit + integration) → Tasks 1/3/4/5. **D7** (BEHAVIOR_CONTRACT + fuzz seed) → Task 6. **D8** (parent close-out) → NOT a state-3 task; the STALE-header correction means `67.2` close-out flips only its own row (handled at state-6, out of this plan). PLAN-VERIFY X-1..X-5 → all resolved above (ADR-0133).
- No placeholder steps remain except the two integration-test bodies in Task 5, which are explicitly "copy the mechanics verbatim from these two named 67.1 tests" — grounded, not vague.
- Type consistency: `CidrRange` (Task 1) is consumed by the enum arms (Task 2) and the engine (`cidr.contains`) / validator (`cidr.validate`) exactly as produced; `ConfigError::InvalidCidrRange` field set is identical at definition (Task 1) and both use sites (Task 2 step 5).
