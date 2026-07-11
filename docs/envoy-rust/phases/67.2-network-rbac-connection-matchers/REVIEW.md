# Phase 67.2 — §5 state-5 CODE REVIEW (the FIRST REVIEW.md for `67.2`)

> Written by the §5 **state-5 code-review** session (`superpowers:requesting-code-review`), per
> `BOOTSTRAP_PROMPT.md` §5 state 5 and `SKILL_ROUTING.md`. The output of this session IS this file.
> Cold-started clean: `git status --porcelain` empty; branch `main`; `HEAD` = `origin/main` =
> `f2fb252` (the §5 state-4 verification commit); `git fetch origin --prune` showed no sibling ahead;
> CI run `29130784660` GREEN on the full 40-char SHA `f2fb2524bca2fae8f6e208461bb309ac5f0de6a3`.
>
> **Review surface:** the whole `67.2` sub-phase (`08f820d..f2fb252`) — the five connection-level
> matcher arms (`Permission::{DestinationIp(CidrRange), DestinationPort(u16)}`,
> `Principal::{DirectRemoteIp, RemoteIp, SourceIp}(CidrRange)`), the new `CidrRange` type, the V-1
> shared-enum fallout across the four exhaustive match sites, and the docs/fuzz seed.
>
> **Method (the load-bearing part — memory `state5-must-probe-untested-compositions`).** A green
> §7.5 (a)-(e) gate proves the code does what its tests ask, never that the tests ask the right
> question — the whole reason `67.1`'s C-1 shipped. So this review did not merely re-read the diff.
> It **LIVE-PROBED the untested `CidrRange` compositions**: (1) a standalone `rustc -O` reproduction
> copying `validate` / `contains` / `prefix_match` verbatim, and (2) a full end-to-end drive of
> `target/debug/envoy-bin` with a crafted config, plus (3) an independent adversarial
> `general-purpose` reviewer subagent tasked to reproduce-or-refute. Every measurement is quoted
> inline.

---

## VERDICT

> ### **NOT APPROVED. §7.5 gate (f) is NOT satisfied.**
>
> `67.2` ships a **config-reachable, release-mode data-plane PANIC** (Critical **C-1** below): a
> `CidrRange` whose `address_prefix` is an IPv4-mapped-IPv6 literal (e.g. `"::ffff:127.0.0.0"`) with
> `prefix_len` in `33..=128` **passes startup validation** but **panics the connection task** the
> first time the arm is evaluated — an index-out-of-bounds in `prefix_match`. It affects all four IP
> arms (`destination_ip` on `local_addr`; `direct_remote_ip`/`remote_ip`/`source_ip` on `peer_addr`),
> reproduced end-to-end against the shipped binary and independently confirmed. The codebase's own
> invariant — *"a data-plane path must never panic"* (`network_rbac.rs`, the `debug_assert!` arms) —
> is violated in release.
>
> Per `BOOTSTRAP_PROMPT.md` §5's asymmetry, a NOT-APPROVED review re-opens the phase at **§5 state 3
> (NOT state 4)**: the **SEPARATE next session** performs a §5.2 state-3 re-entry to land the C-1
> repair under TDD, then a fresh state-4 verification and a fresh state-5 re-review (which SUPERSEDES
> this file per D-3.5 — a review is never edited, only superseded by a later one). Per §5.1 /
> `ADR-0127` this session does **not** chain into the repair.
>
> Everything else in `67.2` is well-built (see §3). C-1 is a single, well-localised defect with a
> one-site fix.

---

## §1. C-1 (CRITICAL) — a validated `CidrRange` panics the data plane on IPv4-mapped-IPv6 prefixes

### What it is

`CidrRange::validate` (`crates/envoy-config/src/bootstrap.rs:1646`) picks the family cap from the
**pre-canonicalised** `address_prefix`:

```rust
let (max, family) = match self.address_prefix {
    IpAddr::V4(_) => (32u8, "IPv4"),
    IpAddr::V6(_) => (128u8, "IPv6"),
};
if self.prefix_len > max { return Err(...) }
```

`IpAddr`'s `FromStr` parses `"::ffff:127.0.0.0"` as `IpAddr::V6`, so `validate` treats it as IPv6 and
accepts any `prefix_len ≤ 128`. But `CidrRange::contains` (`:1664`) **canonicalises** an
IPv4-mapped-IPv6 address to a **4-byte** `Ipv4Addr` *before* indexing in `prefix_match` (`:1688`):

```rust
fn prefix_match(net: &[u8], addr: &[u8], prefix_len: u8) -> bool {
    let full = (prefix_len as usize) / 8;
    if net[..full] != addr[..full] { ... }   // :1691  ← panics: full can be 5..16 on a 4-byte slice
```

So `validate` and `contains` **disagree on the address family**. A prefix `validate` sized against
128 bits is then indexed as 4 bytes → `net[..full]` / `net[full]` slices past the array.

`ADR-0133` only ever contemplated the mapped-**peer** direction ("IPv4-mapped-IPv6 **peers** are
canonicalised to IPv4 before matching"); the mapped-**prefix** direction — the `address_prefix` side
— was never considered, and no test exercises it (see I-1).

### It is reachable through the real config path, for all four IP arms

- **Deserialize → validate → accept.** `parse_bootstrap` runs `serde_yaml` then `validate`;
  `validate_l4_permission` / `validate_l4_principal` (`bootstrap.rs:4477`, `:4532`) call
  `cidr.validate()`, which returns `Ok` for the mapped prefix. Config loads clean.
- **Engine → contains → panic.** `permission_matches`/`principal_matches`
  (`crates/envoy-bin/src/network_rbac.rs:123`, `:148`) call `cidr.contains(&conn.local_addr.ip())`
  and `cidr.contains(&conn.peer_addr.ip())`. On an IPv4 listener both addresses are `V4`, so any
  connection that reaches first-byte evaluation panics.

### Trigger range (measured)

With a v4-mapped-IPv6 `address_prefix` and the compared address canonicalising to V4 (the normal
IPv4-connection case):

- `prefix_len ∈ 40..=128`: **unconditional** panic — `net[..full]` with `full ≥ 5` slices past the
  4-byte array before any comparison, regardless of the peer/local address.
- `prefix_len ∈ 33..=39`: `full == 4`, so `net[..4]` is in-bounds; panics via `net[4]` **only when
  the first four octets match** (comparison passes, then indexes byte 4).
- `prefix_len ≤ 32`: safe.

### Why it matters

An operator writing an IPv4-mapped-IPv6 CIDR (a legal, if unusual, way to spell an IPv4 range) gets a
config that **starts cleanly and then crashes the connection task on the first matching-ish
connection** — a latent, client-triggerable denial of service shipped as "valid." It is exactly the
"untested composition" shape `state5-must-probe-untested-compositions` warns about: the existing
`cidr_range_ipv4_mapped_ipv6_peer_matches_ipv4_range` test covers the *safe* mapped-peer direction
and gives false confidence about the mapped-prefix direction.

### How to fix (for the state-3 re-entry — do NOT apply this session)

Make `validate` size the prefix against the **canonical** family, matching `contains`. Concretely:
canonicalise `address_prefix` via `to_ipv4_mapped()` first, then a mapped prefix is bounded at 32 and
`prefix_len: 40` is rejected fail-loud with `InvalidCidrRange` at config load. (Belt-and-braces, also
guard `prefix_match` so a length mismatch can never index OOB — but the family-consistent `validate`
is the root-cause fix.) Add a regression unit test: `address_prefix: "::ffff:127.0.0.0"`,
`prefix_len: 40` asserts `validate()` is `Err`, and a `contains()` call proves no panic. See I-1 for
the coverage gap that let this through.

---

## §2. First-hand measurement performed this session

### (a) Standalone reproduction of the exact logic (`rustc -O`, release opt — index OOB always panics)

Copying `validate` / `contains` / `prefix_match` verbatim from `bootstrap.rs`:

```
address_prefix parsed as: ::ffff:127.0.0.0
validate() => Ok(())                       ← config is ACCEPTED
about to call contains() ...
thread 'main' panicked at cidr_probe.rs:49: range end index 5 out of range for slice of length 4
```

### (b) End-to-end against the shipped binary

`target/debug/envoy-bin -c <cfg>` where `<cfg>` is a `[rbac, echo]` listener with
`permissions: [{ destination_ip: { address_prefix: "::ffff:127.0.0.0", prefix_len: 40 } }]`,
`action: ALLOW`, `principals: [{ any: true }]`:

```
=== Step 1: listener is UP (config ACCEPTED) ===
=== Step 2: drive a loopback connection ===
recv: b''                                  ← connection dropped, zero bytes
=== envoy-bin output ===
thread 'tokio-rt-worker' panicked at crates/envoy-config/src/bootstrap.rs:1691:11:
range end index 5 out of range for slice of length 4
 WARN connection task panicked error=task 69 panicked with message "range end index 5 out of range for slice of length 4"
```

The proxy process survives (tokio isolates the panic to the per-connection task), but the connection
is aborted with an internal panic — a data-plane path panicking on accepted config.

### (c) Independent adversarial reviewer — CONFIRMED

An independent `general-purpose` reviewer subagent, given the diff and asked to reproduce-or-refute,
returned **CONFIRMED** with its own `rustc -O` probe (`prefix_len 40 → "range end index 5"`;
`prefix_len 128 → "range end index 16 out of range for slice of length 4"`), independently traced
reachability through `parse_bootstrap → validate → contains` for both the `local_addr` and
`peer_addr` arms, and independently derived the `33..=39` conditional sub-range. It found **no other**
validated-but-panics input (pure-v4 can never exceed /32 past validate; pure-v6 stays 16 bytes;
cross-family hits the `_ => false` arm; `/0` and full-byte boundaries are fine) — so the mapped-prefix
direction is the lone hole.

---

## §3. Strengths (the rest of the change is sound)

1. **Exhaustive, catch-all-free classification at every site.** `permission_matches` /
   `principal_matches`, `validate_l4_permission` / `_principal`, the HTTP `lower_permission` /
   `lower_principal`, and the `define_rbac_tree_validator!` macro all enumerate every arm with no
   `_ =>`. A future shared-enum arm breaks the build at each classification site — the intended safety,
   preserved.
2. **The HTTP-rejects-L4-arms divergence is correct and complete** (confirmed WAI, see §5). All five
   arms return `FilterError::InvalidConfig` (`rbac.rs:288`, `:328`), and
   `http_rbac_build_from_config_rejects_l4_principal_startup_fatal` pins that the rejection is
   *startup-fatal*, not merely private to `lower_*`. Every arm has a witness.
3. **`destination_port: u16`** is exactly faithful to upstream's `uint32 + PGV lte:65535` — rejects
   both the `{value:N}` wrapper and `>65535` via serde, pinned by
   `cidr_range_rejects_unknown_field_and_wrapper_prefix_len`.
4. **The three source-IP arms correctly share one `peer_addr.ip()` evaluation**, and
   `remote_ip_and_source_ip_evaluate_peer_like_direct_remote_ip` *proves* the coincidence rather than
   assuming it.
5. **The `extra_leaves` macro parameter is correct for both instantiations** — the tree validator only
   bounds depth + rejects empty sets, so leaf `Ok(())` is right; the CidrRange width check is (meant to
   be) deferred to the L4 walk, which does run for both permission and principal, nested and
   combinators.
6. **The two removed `#[allow(clippy::only_used_in_recursion)]` attrs are correctly removed** — the new
   arms read `conn`, so clippy is clean (verified at the state-4 gate).
7. **Docs kept in step with code** — `BEHAVIOR_CONTRACT.md` item 14 records the arms, the
   `remote_ip ≡ direct_remote_ip ≡ source_ip` equivalence, the bare-`u8` `prefix_len` divergence, and
   the corrected fail-loud framing; the stale `network_rbac.rs` module header was rewritten.

---

## §4. Issues

### CRITICAL

- **C-1** — the config-reachable data-plane panic. Detailed in §1–§2. **Blocks merge.**

### IMPORTANT

- **I-1 — the coverage/fuzz shape is structurally blind to C-1.** No test anywhere calls
  `CidrRange::contains()` with a v4-mapped-IPv6 *prefix*, and the `parse_bootstrap` fuzz target only
  exercises deserialize + `validate` — `contains` is a data-plane-only entry point the fuzzer never
  reaches, so the "20000 runs, no crash" gate (d) is *structurally* incapable of finding this panic,
  and the new corpus seed cannot change that. The state-3 re-entry should add (a) the mapped-prefix
  regression unit test from §1, and (b) a property test or fuzz target over `CidrRange::contains(cidr,
  addr)` asserting no panic for any `validate`-passing `cidr` and any `IpAddr` — which both catches
  C-1 and guards the regression. (No obligation to add a *fuzz target* if the property test covers it;
  weigh against the "NO new fuzz target" SPEC posture.)

### MINOR — none blocks

- **N-1 (defensive depth, advisory).** Even after the `validate` fix, `contains`/`prefix_match` remain
  `pub` and will panic on any `prefix_len` that outruns the octet length. A `debug_assert!` (or a
  saturating `full.min(net.len())` guard with an early cross-length bail) at the top of `prefix_match`
  would turn a future "validate forgot a family" regression into a caught assertion rather than a
  data-plane panic — the same defense-in-depth the `permission_matches` unreachable arms already use.
  Optional; the family-consistent `validate` is the real fix.

---

## §5. Findings explicitly considered and REJECTED by this review

Recorded so a future session (and the state-3 re-entry) does not "fix" what is deliberate:

- **"The HTTP RBAC filter rejecting `destination_ip`/`destination_port`/`*_remote_ip`/`source_ip` is a
  parity bug — make it accept them."** **NO.** This is a DELIBERATE FAIL-LOUD DIVERGENCE
  (`BEHAVIOR_CONTRACT.md` item 14, ADR-0133, ADR-0049 decision-2 (b)): upstream Envoy *accepts* these
  arms in an HTTP rbac filter (measured), envoy-rust rejects them startup-fatal because they can never
  match at L7. Confirmed the framing; do **not** edit `crates/envoy-filter/src/rbac.rs` toward
  HTTP-accepts-L4 parity.
- **"The bare-`u8` `prefix_len` (rejecting the `{value:N}` wrapper) is a divergence bug."** **NO.**
  Deliberate, matching the `max_request_bytes` UInt32Value precedent (ADR-0063) and the fail-loud
  posture (ADR-0049). Pinned by test.
- **"There is no differential fixture for the IP/port arms — add one."** **NO.** The arms are
  structurally host-dependent under the Docker harness (parent V-4, ADR-0128, SPEC §2): the source-IP
  arms see the bridge address `192.168.65.2` and the destination arms see per-proxy reserved ports.
  In-process + loopback coverage is the recorded posture; regression surface `0001`–`0073` stays green.
  This is acceptable and is NOT what C-1 is about — C-1 is a panic reachable *without* any differential.
- **"Add a `_ =>` catch-all to the four exhaustive RBAC match sites."** **NO.** The compile break is
  the intended forcing function; never add a catch-all.

---

## §6. Assessment

**Ready to merge?** **No.**

**Reasoning.** The change is otherwise well-structured, exhaustive, and well-tested — but it admits a
config-reachable, release-mode data-plane panic (C-1) across all four IP arms, independently
reproduced end-to-end, because `validate()` and `contains()` disagree on the address family of an
IPv4-mapped-IPv6 `address_prefix`. That violates the codebase's own "data plane must never panic"
invariant and cannot ship.

**Next session (SEPARATE, per §5.1 / `ADR-0127`; a NOT-APPROVED review re-opens at §5 state 3, NOT
state 4):** a §5.2 **state-3 re-entry** — land the C-1 repair (family-consistent `validate`) + the I-1
regression coverage under TDD, then a fresh state-4 verification, then a fresh state-5 re-review that
SUPERSEDES this file. ROADMAP row `67.2` stays `in-progress`; parent row `67` stays `in-progress`
(it flips `done` only when `67.1`+`67.2`+`67.3` are all `done`). `67.2` must NOT touch `67.3`'s scope.

---

## §7. Carry-forward ledger

This review **opens C-1 (Critical, blocking) and I-1 (Important)** against `67.2`, both consumed by
the imminent state-3 re-entry; and one advisory Minor **N-1**.

- **State as of `ADR-0133` (unchanged by this review):** `CF-67-1` (`shadow_rules`), `CF-67-2`
  (`Action::LOG`), `CF-67-3` (`on_data`-time iteration) stay live, none blocks; the parent SPEC's V-1
  was CONSUMED by `67.2`.
- **DEFERRED to `67.3` (unchanged):** the `ConnectionHandler` establishment/data-phase split, the
  correct `[rbac, tcp_proxy]` composition (which DELETES `UnsupportedNetworkFilterChainComposition`),
  the per-terminal data-less-FIN semantics.
- **This review needs no ADR.** **`DECISIONS.md` ledger head: `ADR-0133`; next available: `ADR-0134`,
  unreserved.** (The state-3 re-entry likewise needs none unless the fix changes a measured wire
  shape, which it does not — it corrects an internal family classification.)
- **Numbering:** `M66-1` was never allocated; the ledger does not backfill.
