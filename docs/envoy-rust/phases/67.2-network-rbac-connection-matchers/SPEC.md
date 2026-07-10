# Phase 67.2 — network RBAC connection-level matcher arms (`CidrRange`, IP + port)

> **Status:** SPEC authored at the phase-67 §6.1 split (`ADR-0129`). Blocked on `67.1` landing.
> Next, once `67.1` is `done`: §5 state-2 PLAN-write (`superpowers:writing-plans` → `PLAN.md`).
> **Parent phase:** `67` (`docs/envoy-rust/phases/67-network-filter-rbac/SPEC.md`), split into
> `67.1` + `67.2` per **ADR-0129**. Parent scope locked by **ADR-0128**.
> **ROADMAP row:** `67.2` (parent row `67`, `sub-phases = 67.1, 67.2`). **depends-on: `67.1`.**
> **Sibling:** `67.1-network-rbac-iteration-protocol` — the protocol, the filter, the fixtures.
> **Estimated size:** ~675 net LoC / ~7-9 TDD tasks (well under the §6.1 gate).
> **This sub-phase flips parent row `67` to `done` at its state-6 close-out.**

This document is written for a stranger with zero prior context (doctrine D-3.4).

---

## §1. Goal

Add the **connection-level matcher arms** that L4 RBAC actually needs, on top of the `any`-only
filter `67.1` shipped:

- `Principal::DirectRemoteIp(CidrRange)`, `Principal::RemoteIp(CidrRange)`,
  `Principal::SourceIp(CidrRange)`
- `Permission::DestinationPort(u16)`, `Permission::DestinationIp(CidrRange)`

…plus the new `CidrRange` type, and the **breaking-change fallout on the HTTP RBAC filter** these
arms force (§3, D3 — *the single biggest cost in this sub-phase, and the reason it is its own phase*).

---

## §2. Why this is a separate sub-phase, and why it is NOT a stub

`67.1` is independently green, differentially witnessed, and stub-free: `action: ALLOW` / `action:
DENY` over `any: true` completely exercises both decision paths, both counters, and the whole
iteration protocol, via fixtures `0072` and `0073`. Nothing in `67.1` says "TODO: extend later" and
nothing in it is unreachable by a test. §6.3 is satisfied.

`67.2` is likewise not a stub: every arm it adds is exercised by an in-process backstop that binds
`127.0.0.1` with a known port.

**Why the new arms are witnessed in-process rather than differentially** (parent PLAN-VERIFY V-4,
measured, and locked by ADR-0128):

- `direct_remote_ip` / `remote_ip` / `source_ip` see the **Docker bridge address** inside the
  differential harness — `192.168.65.2` on this dev host, an explicitly-documented host fragility
  (memory `differential-host-bridge-ip-192-168-65-2`). A fixture pinning a CIDR would be
  host-dependent.
- `destination_port` / `destination_ip` must match the `{{PORT}}`-substituted listener address, which
  **differs between the two proxies** by construction (each gets its own reserved host port).

So the IP/port arms are **not host-deterministic under the Docker harness**. They are covered
in-process, bound to `127.0.0.1` with a known port, where both the peer and local address are exact.
The **differential surface for `67.2` is regression-only**: all fixtures `0001`-`0073` stay green.
This is the same posture phase 25.1 took (a foundation slice with no new fixture, differentially
proven by its consumer) — recorded, not silent.

---

## §3. Deliverables

### D1 — `CidrRange` (`crates/envoy-config/`)

New type modelling Envoy's `config.core.v3.CidrRange`:

```rust
pub struct CidrRange { address_prefix: IpAddr, prefix_len: u8 }
```

with a `contains(&self, addr: &IpAddr) -> bool` supporting IPv4 and IPv6, and a new
`ConfigError` variant for an invalid prefix (`prefix_len > 32` on v4 / `> 128` on v6, or an
unparseable `address_prefix`).

**PLAN-VERIFY X-1 (unresolved — this is the wire-shape unknown).** Determine against the pinned image
`envoyproxy/envoy:v1.33.0`:
- Is `address_prefix` a bare string (`"10.0.0.0"`) that envoy-rust should parse to `IpAddr`, or does
  Envoy accept other forms?
- Is `prefix_len` a **bare integer** (`prefix_len: 24`) or Envoy's `{value: N}` `UInt32Value`
  wrapper (`prefix_len: {value: 24}`)? **This is a serde-shape decision that a fixture would catch
  and an in-process test would not** — probe it with `--mode validate` before writing the struct.
- IPv6 handling: does an IPv4-mapped IPv6 peer (`::ffff:127.0.0.1`) match an IPv4 `CidrRange`?
  Envoy's behavior here is the contract (D-3.3).

### D2 — Five new matcher arms (`crates/envoy-config/`)

- `Principal`: `DirectRemoteIp`, `RemoteIp`, `SourceIp` (3 arms).
- `Permission`: `DestinationPort`, `DestinationIp` (2 arms).

Each needs: an enum variant, a `#[serde(rename)]`, **and** a dispatch line in the hand-rolled
`impl_single_key_oneof!` deserializer (`crates/envoy-config/src/bootstrap.rs:1627` for `Permission`,
`:1675` for `Principal` — `serde_yaml` 0.9 cannot do externally-tagged enums from plain YAML maps).

**PLAN-VERIFY X-2.** Is `source_ip` merely a deprecated alias of `direct_remote_ip` upstream (both
validate — parent SPEC R-0.4)? If so, **model it as an alias, not a third code path.** Probe the
runtime semantics, not just config acceptance: drive a connection through a proxy configured with
each and compare.

### D3 — **The V-1 fallout: three exhaustive match sites break** *(the load-bearing cost)*

`Permission` and `Principal` are **shared** with the **HTTP** RBAC filter
(`envoy.filters.http.rbac`). Adding arms to those enums is a **hard compile break** at three sites,
confirmed by code-read at `6cfa8be`:

**(a) `crates/envoy-filter/src/rbac.rs:262-284` — `lower_permission`.**
Matches all 7 `Permission` arms with **no `_ =>` catch-all**. Returns
`Result<RuntimeMatcher, FilterError>`, so the two new arms get `Err` arms:

```rust
envoy_config::Permission::DestinationPort(_) | envoy_config::Permission::DestinationIp(_) => {
    return Err(FilterError::InvalidConfig { message: "…is an L4-only matcher…".into() })
}
```

**(b) `crates/envoy-filter/src/rbac.rs:291-314` — `lower_principal`.**
Same shape; three new `Err` arms.

This mirrors **Envoy's own HTTP-vs-L4 split**: upstream rejects `header` matchers at L4 (measured,
parent SPEC R-0.4), and envoy-rust here rejects the L4-only arms at L7. Symmetric, and fail-loud per
ADR-0049 decision-2 (b).

**(c) `crates/envoy-config/src/bootstrap.rs:4087-4148` — `define_rbac_tree_validator!`.**
**This site is the one the parent SPEC did not enumerate, and it is the awkward one.** It is a
**single macro** whose body is instantiated for **both** `Permission` and `Principal`
(`validate_permission_tree`, `validate_principal_tree`), matching arms by the shared names
`Any`/`Header`/`Metadata`/`UrlPath` plus macro-parameterized `$and | $or` / `$not`. Its match is
exhaustive.

The five new arms are **asymmetric** — 3 are `Principal`-only, 2 are `Permission`-only — so the
shared body **cannot name them uniformly**. The macro needs a new variadic "extra leaf arms"
parameter (or the two validators must be de-macro'd). **PLAN-VERIFY X-3: settle the shape.**
Recommendation: add a `extra_leaves: [$($leaf:ident),*]` parameter emitting `crate::$node::$leaf(_)
=> Ok(())` arms — the new arms are all leaves (no recursion), so this is mechanical.

### D4 — Widen `67.1`'s L4 leaf allow-list

`67.1` (its D3, closing **CF-67-4**) ships a validation walk over a **network** `rbac` filter's
policy trees rejecting every leaf except `any`. `67.2` widens that allow-list to admit the five new
arms. The `header` / `url_path` / `metadata` rejections **stay forever**:

- rejecting `header` is **parity** with upstream Envoy (measured);
- rejecting `url_path` / `metadata` is a **deliberate fail-loud divergence** (ADR-0049 decision-2
  (b)) where upstream accepts a matcher that can never match at L4. **No differential observable.**

### D5 — Engine arms (`crates/envoy-bin/src/network_rbac.rs`)

Five new match arms over `ConnectionInfo { peer_addr, local_addr }`:

| Arm | Evaluates against |
|---|---|
| `direct_remote_ip` / `remote_ip` / `source_ip` | `peer_addr.ip()` |
| `destination_ip` | `local_addr.ip()` |
| `destination_port` | `local_addr.port()` |

(`remote_ip` differs from `direct_remote_ip` upstream only when a PROXY-protocol / XFF listener filter
rewrites the remote address; envoy-rust has no listener filters, so the two coincide today.
**Record this in `BEHAVIOR_CONTRACT.md` rather than silently aliasing them.**)

### D6 — In-process backstops (`crates/envoy-bin/tests/`)

Bound to `127.0.0.1` with a known port, so peer and local addresses are exact:

- `direct_remote_ip` match / no-match; `remote_ip`; `source_ip` (per X-2's finding).
- `destination_port` match / no-match; `destination_ip` match / no-match.
- `not_id` / `and_ids` / `or_ids` composition over the new leaves.
- IPv4 and IPv6 `CidrRange::contains` unit tests, including boundary prefix lengths (`/0`, `/32`,
  `/128`) and the invalid-prefix rejections.
- **HTTP RBAC rejects the L4-only arms** (D3 a/b) — one test per arm.

### D7 — Documentation + fuzz corpus seed

- `BEHAVIOR_CONTRACT.md`: the five arms and what each evaluates against; the
  `remote_ip` ≡ `direct_remote_ip` note (D5); the HTTP-rejects-L4-arms / L4-rejects-`header`
  symmetry; whatever X-1 settles about `prefix_len`'s wrapper and IPv4-mapped-IPv6.
- A fuzz **corpus seed** exercising a `CidrRange` in a network `rbac` `typed_config`. **NO new fuzz
  target** — the pre-existing `parse_bootstrap` target reaches the new `CidrRange` parser the moment
  it lands. The seed needs an explicit `!`-un-ignore line in `crates/envoy-config/fuzz/.gitignore`,
  **proven tracked with `git ls-files`** (memory `fuzz-corpus-seed-gitignored-by-default`). **The
  §7.5 gate (d) must be RECORDED EXPLICITLY at state-4**, not passed over in silence.

### D8 — Parent close-out

`67.2`'s state-6 close-out flips **parent row `67` to `done`** (both sub-phases `done`), per ROADMAP
invariant 4.1.2 and the `02`/`12`/`13`/`14`/`25` parent-row precedent.

---

## §4. Out of scope

- **`shadow_rules`** and the shadow counters (emitted as constant `0` by `67.1`; the config field
  rejected loudly). **CF-67-1** stays live.
- **`Action::LOG`.** **CF-67-2** stays live.
- **`on_data`-time filter iteration + buffering.** **CF-67-3** stays live — the first
  payload-parsing network filter (`mongo_proxy` / `zookeeper_proxy` / `kafka_broker`) needs it.
- **`authenticated` / `filter_state` / `requested_server_name` principals.** Need TLS-peer identity or
  a listener-filter concept envoy-rust lacks. Not opened here.
- **A differential fixture for the IP/port arms.** Structurally host-dependent (§2). If a future phase
  lands a listener that binds a *fixed, identical* port on both proxies, revisit.

---

## §5. Differential surface at sub-phase end

- **NO new fixture.** Regression-only: all fixtures `0001`-`0073` stay green (§7.5(a),(b)) — including
  `67.1`'s `0072`/`0073`, which continue to exercise the `any` path through the now-widened allow-list.
- The new arms are witnessed by **in-process backstops** (D6). Rationale recorded in §2 and in
  `BEHAVIOR_CONTRACT.md`.
- Conformance: unchanged. **Never trim `tests/conformance/h2spec/known-failures.txt`** (memory
  `h2spec-3-5-2-preface-host-sensitive`).

---

## §6. Estimated size

| Area | Net LoC (est.) |
|---|---|
| D1 `CidrRange` + Deserialize + v4/v6 `contains` + prefix validation + `ConfigError` + tests | ~230 |
| D2 5 enum arms + 5 `impl_single_key_oneof!` dispatch lines | ~110 |
| D3 **V-1 fallout:** `lower_permission` / `lower_principal` reject arms + `define_rbac_tree_validator!` re-parameterization + tests | ~85 |
| D4 widen the `67.1` L4 leaf allow-list | ~20 |
| D5 `network_rbac.rs` engine: 5 new matcher arms + tests | ~90 |
| D6 in-process matcher backstops | ~120 |
| D7 `BEHAVIOR_CONTRACT.md` + fuzz corpus seed | ~40 |
| **Total** | **~695** |

**~695 net LoC, ~7-9 TDD tasks.** Well under the §6.1 thresholds (~1500 LoC OR ~25 tasks).

---

## §7. PLAN-VERIFY items (resolve at `67.2`'s state-2 PLAN-write)

- **X-1 (the wire-shape unknown).** `CidrRange.address_prefix` type; `prefix_len` bare-integer vs
  `{value: N}` `UInt32Value` wrapper; IPv4-mapped-IPv6 matching. **Probe against the pinned image with
  `--mode validate` + a live drive before writing the struct.** (D1.)
- **X-2.** Is `source_ip` a deprecated alias of `direct_remote_ip`? Probe runtime semantics, not just
  config acceptance. If an alias, model it as one. (D2, D5.)
- **X-3.** The `define_rbac_tree_validator!` re-parameterization shape for the asymmetric leaves.
  Recommendation: an `extra_leaves: [$($leaf:ident),*]` macro parameter. (D3 c.)
- **X-4.** Re-confirm `lower_permission` / `lower_principal` are still exhaustive with no `_ =>`
  catch-all (they were at `6cfa8be`; a sibling phase could have added one). If a catch-all appeared,
  the compile break becomes a **silent** behavior change — **add the explicit `Err` arms anyway.**
- **X-5.** Whether the six reused RBAC `ConfigError` variants still carry the
  `"HCM listener {listener:?}: …"` prefix, or whether `67.1`'s W-1 already generalized them.

---

## §8. Standing traps

Identical to `67.1` §9, and re-stated here because this file must stand alone:

1. **`cargo build -p envoy-bin` before ANY local differential run** (memory
   `differential-harness-uses-debug-envoy-bin`) — the harness executes `target/debug/envoy-bin`.
2. **Never pipe a verification run through `tail`** (memory `never-pipe-verification-runs-through-tail`).
3. **`cargo test --workspace` needs `--no-fail-fast`** on this dev host; the invariant core of ~5 REDs
   is environmental. **CI is authoritative.**
4. **Never weaken a fixture. Never trim `known-failures.txt`.** h2spec is a SKIP, not a pass, locally.
5. **Do NOT re-open BLOCK-66-1** (ADR-0126).
6. **ROADMAP rows must escape literal `|` as `\|`.** Rows `36`/`38`/`39`/`52`/`54` are already
   malformed and must NOT be "fixed" (append-only). Verify with `re.split(r'(?<!\\)\|', line)[1:-1]`
   → exactly 6 cells. **Never use `awk -F'|'`.**
7. **Confirm CI with the FULL 40-char SHA** (memory `gh-run-list-commit-needs-full-sha`).
8. **`cargo deny check` can red on a freshly-published advisory** against an existing dep — patch-bump
   it, don't treat it as a phase regression (memory `cargo-deny-reds-on-unrelated-advisory`).

---

## §9. Carry-forward ledger

- **CONSUMED by `67.2`:** the parent SPEC's **V-1** (the shared-enum fallout — D3).
- **Already consumed by `67.1`:** CF-66-2, M66-3, M66-4, CF-67-4.
- **Already closed by the parent recon (no code change): M66-5.**
- **Still live after `67.2`, none blocks:** **CF-67-1** (`shadow_rules`), **CF-67-2** (`Action::LOG`),
  **CF-67-3** (`on_data`-time iteration + buffering) + M66-6, M66-7, CF-66-1, M64-2, M64-3, M65-1,
  M57-1, M55-1, M53-2, M53-3, M48-2, M42-1, the `DC`/retry-budget-overflow slices of M45-2, the
  phase-58 candidate carry-forward, M40-1, M39-1/M39-2, M38-1/M38-2, CF-39-1, M37-*, M36-*, M34-*,
  M33-*, the empty-`metadata_match` doc-comment, M29-*/M30-*, the phase-31 cosmetics, and the
  HTTP-filters-family (1)-(4).
- **Numbering: M66-1 was never allocated.** Do not "fix" the gap.

---

## §10. What lands after `67.2`

The Network-filters family continues: `sni_cluster` (needs a `tls_inspector` **listener filter** — a
subsystem envoy-rust wholly lacks, `crates/envoy-config/src/lib.rs:230` defers it by name),
`redis_proxy` / `thrift_proxy` (terminal codecs), and `mongo_proxy` / `zookeeper_proxy` /
`kafka_broker` (non-terminal **and** payload-parsing — they consume **CF-67-3**, the `on_data`
iteration protocol `67.1` deliberately did not build).

**Mission is NOT complete.** Beyond this family: HTTP filters, load balancing, upstream robustness,
HTTP/3 + QUIC, gRPC, xDS / dynamic config, observability, runtime / hot-restart, and the WASM host all
remain unbuilt.
