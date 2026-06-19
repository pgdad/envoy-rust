# Phase 29 — `29-lb-maglev` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Read with zero prior context (D-3.4): everything you need is in this file, in `SPEC.md`, and in the cited source lines.

**Goal:** Implement Envoy v1.33.0's `MAGLEV` consistent-hashing cluster LB (Google's prime-sized permutation-table build + lookup) keyed by the route-level header `hash_policy` introduced in phase 28, behaviorally equivalent to upstream Envoy under the §7.2 differential contract.

**Architecture:** The **request side is identical to phase-28 `RING_HASH`** — the route `hash_policy` header → `xxHash64(value, seed 0)` → an `Option<u64>` key threaded through `Cluster::pick()`. Only the **table-build** differs: a new `crates/envoy-cluster/src/maglev.rs` replaces the ketama ring with Maglev's permutation table. The phase-28 `ring: Option<HashRing>` discriminator on `Cluster` is replaced by an explicit `hash_lb: Option<HashLb>` dispatch (the **M28-3 refactor**) so a SECOND consistent-hash policy coexists with `RING_HASH` without misclassification. The differential is locally observable (no reload trigger).

**Tech Stack:** Rust (workspace crates `envoy-config`, `envoy-cluster`); from-scratch xxHash64 (D-3.2 forbids a hashing crate); `testcontainers` differential harness against `envoyproxy/envoy:v1.33.0`.

---

## §A — The §6.2-LOCKED Maglev algorithm (empirical reconnaissance result)

Cracked at this PLAN-write against live upstream Envoy v1.33.0 (a from-scratch replica reproduced the live oracle **80/80** at the default `table_size` and **64/64** at `M=17` — the STRONG bar). This is the byte-for-byte contract `maglev.rs` MUST reproduce; **ADR-0072** locks it.

**Per-host permutation** (host key = the host address `asString()` = the `ip:port` string, e.g. `172.31.0.2:5678` — **NO `_i` replica suffix**, unlike the ring):
```
offset = xxHash64(key, seed = 0) % M
skip   = xxHash64(key, seed = 1) % (M - 1) + 1        # seed 1 — the cracked unknown
permutation[j] = (offset + j * skip) % M              for j = 0, 1, 2, ...
```
Rejected hypotheses (all scored ~chance, 32–44/80): `skip` from the high/low 32 bits of a single seed-0 hash; `skip` from seed-0 (same hash as offset); the ring-style `ip:port_0` key shape.

**Populate (claim) loop:** iterate hosts in **config / endpoint-list order** `[0, 1, …, n-1]`. Each round, each host claims the next still-unclaimed slot from its `permutation` (advancing its own cursor past already-claimed slots); repeat rounds until all `M` slots are filled. On contention the **earlier-config-order host wins** (disambiguated empirically at `M=17`: config order 64/64 vs reversed 60/64).

**`table_size` M:** default **65537**; must be **prime**; max **5000011**.

**Request lookup:** `table[ xxHash64(header_value, seed 0) % M ]` → host index. The request hash is the SAME as phase-28 RING_HASH (the existing `hash_request_key` / `xxh64` seed-0 path; empty-but-present value hashes to `xxh64(b"")` — NOT a fallback).

### Config dispositions (§6.2-verified)
| Config | Upstream Envoy v1.33.0 | envoy-rust (ADR-0072) |
|---|---|---|
| non-prime `table_size` (e.g. 100) | **startup-fatal** — `"The table size of maglev must be prime number"` | startup-fatal `ConfigError::MaglevTableSizeNotPrime` (ADR-0049 all-fatal) |
| `table_size` > 5000011 | **startup-fatal** — PGV `value must be less than or equal to 5000011` | startup-fatal `ConfigError::MaglevTableSizeTooLarge` |
| `table_size` == 5000011 | boots OK | accepted |
| `maglev_lb_config` on a **non-MAGLEV** cluster | **silently accepted-and-ignored** (boots, runs round-robin) | **accept-and-ignore** — validation GATED to MAGLEV clusters. This MATCHES Envoy **and** the phase-28 `ring_hash_lb_config`-on-non-RING_HASH precedent (`bootstrap.rs:2629`, test `accepts_ring_hash_lb_config_on_non_ring_hash_cluster`). ADR-0072 resolves the SPEC §2.1 "fatal" projection in favour of parity. |

### Fallback / stats (§6.2-verified)
- **Header ABSENT** → non-deterministic across requests (cursor/round-robin path); matches phase-28 **M28-2** characterization (NOT a differential assertion).
- **Header PRESENT but EMPTY** → deterministic (hashes `xxh64(b"")`); preserves the phase-28 present-empty-vs-absent `.map()` distinction.
- **LB stats:** NONE portable (only generic `cluster.<name>.lb_*` exist; no maglev-table namespace). Emit no LB stat — the phase-21/24/28 discipline.

### Regression oracle (the `maglev.rs` unit-test ground truth — live-Envoy + replica confirmed)
Hosts in config order: **host 0 = `172.31.0.2:5678`**, **host 1 = `172.31.0.3:5678`**. `table_size = 65537` (default). `x-hash-key` value → selected host:

| key | host | key | host | key | host |
|---|---|---|---|---|---|
| `key-0` | 0 | `key-33` | 1 | `foo` | 0 |
| `key-2` | 1 | `key-41` | 1 | `bar` | 1 |
| `key-7` | 1 | `key-50` | 0 | `baz` | 0 |
| `key-10` | 0 | `key-63` | 1 | `a` | 1 |
| `key-11` | 0 | `user-alice` | 1 | `hello` | 1 |
| `key-14` | 1 | `user-bob` | 0 | `world` | 1 |
| `key-19` | 0 | `1.2.3.4` | 0 | `0` | 1 |
| `key-23` | 0 | `session-abc` | 1 | `""` (empty) | 0 |

Full-table distribution at M=65537: host0 = 32769 slots, host1 = 32768 (near-perfect). Use this oracle verbatim in Task 4.

---

## File structure

| File | Responsibility | Task |
|---|---|---|
| `crates/envoy-cluster/src/xxhash.rs` (modify) | generalize `xxh64` to a seeded `xxh64_seed(data, seed)`; keep `xxh64(d) = xxh64_seed(d, 0)` byte-identical | 1 |
| `crates/envoy-config/src/bootstrap.rs` (modify) | `LbPolicy::Maglev`; `MaglevLbConfig { table_size }`; `maglev_lb_config` field; MAGLEV-gated validation | 2, 3 |
| `crates/envoy-config/src/lib.rs` (modify) | `ConfigError::MaglevTableSizeNotPrime` / `MaglevTableSizeTooLarge` | 3 |
| `crates/envoy-cluster/src/maglev.rs` (create) | `MaglevTable::build` + `lookup` — the §A algorithm + pinned oracle test | 4 |
| `crates/envoy-cluster/src/cluster.rs` (modify) | M28-3 refactor: `hash_lb: Option<HashLb>` + `HashLb` enum + `pick()` dispatch + `from_bootstrap` build | 5 |
| `crates/envoy-cluster/src/lib.rs` (modify) | `mod maglev;` | 4 |
| `tests/fixtures/0037-lb-maglev/` (create) | the MAGLEV differential fixture (clone of 0036) | 6 |
| `tests/differential/src/lib.rs` (modify) | wire the 0037 key-sweep driver (STRONG: cross-proxy identical selection) | 6 |
| backstop tests (in `cluster.rs` / `maglev.rs`) | determinism, spread, fallback, single-host, RR + RING_HASH unchanged | 7 |
| `crates/envoy-config/fuzz/` seed + `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (modify) | `parse_bootstrap` maglev seed; "LB selection" MAGLEV row + M28-1 fold | 8 |

**§6.1 split decision: NOT split.** ~8 tasks / est. ~450–550 net LoC — well under the ~25-task / ~1500-LoC gate (xxHash64, the whole route-`hash_policy` request plumbing, the `LbPolicy`/`*LbConfig` config pattern, and the two-backend fixture harness already exist). **ADR-0073 does NOT fire** (it remains reserved).

---

## Task 1: Seeded xxHash64

**Files:**
- Modify: `crates/envoy-cluster/src/xxhash.rs`
- Test: same file `#[cfg(test)] mod tests`

Maglev's `skip` needs `xxHash64(key, seed = 1)`; the current `xxh64` hard-codes `SEED = 0`. Generalize, preserving seed-0 output byte-for-byte (phase-28 vectors must still pass).

- [ ] **Step 1: Write the failing test** (seed-1 vector; generate the expected with the project's reference `xxhash` 3.7.0)

```python
# python3 -c "import xxhash; print(hex(xxhash.xxh64(b'172.31.0.2:5678', seed=1).intdigest()))"
```
Add to `mod tests`:
```rust
#[test]
fn seed1_host_key() {
    // xxhash 3.7.0: xxh64(b"172.31.0.2:5678", seed=1) — the maglev `skip` hash.
    assert_eq!(xxh64_seed(b"172.31.0.2:5678", 1), 0x<FILL_FROM_PYTHON>);
}
#[test]
fn seed0_equiv() {
    // The seeded fn at seed 0 is byte-identical to the phase-28 xxh64.
    assert_eq!(xxh64_seed(b"abc", 0), xxh64(b"abc"));
    assert_eq!(xxh64_seed(b"", 0), 0xEF46_DB37_51D8_E999);
}
```

- [ ] **Step 2: Run test, verify it fails** — `cargo test -p envoy-cluster xxhash::` → FAIL (`xxh64_seed` not found).

- [ ] **Step 3: Implement** — rename the body of `xxh64` to `xxh64_seed(data: &[u8], seed: u64)`, replacing the `const SEED: u64 = 0;` with the `seed` parameter (it flows into the four lane inits and the `<32` `SEED + PRIME64_5` branch). Keep `xxh64` as a thin wrapper:
```rust
pub(crate) fn xxh64(data: &[u8]) -> u64 {
    xxh64_seed(data, 0)
}

pub(crate) fn xxh64_seed(data: &[u8], seed: u64) -> u64 {
    let len = data.len();
    let mut input = data;
    let mut h: u64 = if len >= 32 {
        let mut v1 = seed.wrapping_add(PRIME64_1).wrapping_add(PRIME64_2);
        let mut v2 = seed.wrapping_add(PRIME64_2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(PRIME64_1);
        // ... (unchanged block loop + merges) ...
    } else {
        seed.wrapping_add(PRIME64_5)
    };
    // ... (unchanged tail + avalanche) ...
}
```
Update the module doc-comment: it now provides seeded xxHash64 (seed 0 = phase-28 ring/request default; seed 1 = the maglev `skip` hash).

- [ ] **Step 4: Run tests, verify pass** — `cargo test -p envoy-cluster xxhash::` → all PASS (the 5 existing seed-0 vectors + the 2 new).

- [ ] **Step 5: Commit** — `git add crates/envoy-cluster/src/xxhash.rs && git commit` (message: `phase 29: Task 1 — seeded xxHash64 (xxh64_seed)`).

---

## Task 2: Config — `LbPolicy::Maglev` + `MaglevLbConfig`

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`LbPolicy` :297; `Cluster` struct field near :254; add `MaglevLbConfig` near the `RingHashLbConfig` block :311)
- Test: same file `#[cfg(test)] mod tests` (near the phase-28 ring config tests :4508)

- [ ] **Step 1: Write failing tests** — mirror the ring-config tests (`parses_lb_policy_ring_hash`, `ring_hash_lb_config_empty_applies_defaults`, `ring_hash_lb_config_absent_is_none`, `accepts_ring_hash_lb_config_on_non_ring_hash_cluster`):
```rust
#[test] fn parses_lb_policy_maglev() { /* lb_policy: MAGLEV → LbPolicy::Maglev */ }
#[test] fn maglev_lb_config_empty_applies_default_table_size() { /* {} → table_size 65537 */ }
#[test] fn maglev_lb_config_absent_is_none() { /* no key → None */ }
#[test] fn maglev_lb_config_explicit_table_size() { /* table_size: 65537 round-trips */ }
#[test] fn accepts_maglev_lb_config_on_non_maglev_cluster() { /* on ROUND_ROBIN → parses, is_some, validate OK */ }
```

- [ ] **Step 2: Run, verify fail** — `cargo test -p envoy-config maglev` → FAIL (no `Maglev` variant).

- [ ] **Step 3: Implement:**
```rust
pub enum LbPolicy { RoundRobin, RingHash, Maglev }   // add Maglev

/// 29 D1 (ADR-0071/0072): MAGLEV LB tuning. Mirrors Envoy v1.33's
/// `Cluster.MaglevLbConfig`. `table_size` default 65537 (Envoy proto default),
/// must be prime, max 5000011. Validation (see `validate_cluster`) is gated to
/// MAGLEV clusters and is all-fatal for non-prime / over-max (ADR-0049); a
/// `maglev_lb_config` on a non-MAGLEV cluster is accepted-and-ignored (Envoy
/// parity + the phase-28 ring_hash precedent).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MaglevLbConfig {
    #[serde(default = "default_maglev_table_size")]
    pub table_size: u64,
}
fn default_maglev_table_size() -> u64 { 65537 }
```
Add to the `Cluster` struct (next to `ring_hash_lb_config` :254):
```rust
/// 29 D1 (ADR-0071/0072): OPTIONAL MAGLEV tuning. `None` when absent. A present
/// `maglev_lb_config` on a non-MAGLEV cluster is accepted-and-ignored (validation
/// gated to MAGLEV clusters — see `validate_cluster`).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub maglev_lb_config: Option<MaglevLbConfig>,
```

- [ ] **Step 4: Run, verify pass** — `cargo test -p envoy-config maglev` → PASS.

- [ ] **Step 5: Commit** — `phase 29: Task 2 — LbPolicy::Maglev + MaglevLbConfig config surface`.

---

## Task 3: Config validation (prime / over-max, MAGLEV-gated)

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (`ConfigError` enum near :655) ; `crates/envoy-config/src/bootstrap.rs` (`validate_cluster` :2626; add an `is_prime` helper)
- Test: `bootstrap.rs` tests (near the ring validation tests :4650–4770)

- [ ] **Step 1: Write failing tests:**
```rust
#[test] fn rejects_non_prime_maglev_table_size() { /* table_size: 100 on MAGLEV → MaglevTableSizeNotPrime */ }
#[test] fn rejects_over_max_maglev_table_size() { /* table_size: 5000012 → MaglevTableSizeTooLarge */ }
#[test] fn accepts_max_maglev_table_size() { /* table_size: 5000011 → OK */ }
#[test] fn accepts_default_prime_maglev_table_size() { /* 65537 → OK */ }
// accepts_maglev_lb_config_on_non_maglev_cluster (Task 2) also covers the gating.
```

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** — add `ConfigError` variants (mirror `RingSizeInversion`):
```rust
#[error("cluster '{cluster}' maglev_lb_config.table_size {table_size} is not a prime number")]
MaglevTableSizeNotPrime { cluster: String, table_size: u64 },
#[error("cluster '{cluster}' maglev_lb_config.table_size {table_size} exceeds the maximum 5000011")]
MaglevTableSizeTooLarge { cluster: String, table_size: u64 },
```
Add the MAGLEV-gated block in `validate_cluster` (BEFORE the EDS early-return, mirroring the RING_HASH block at :2633). Check over-max FIRST, then primality (so the bounded `is_prime` trial loop never runs on a huge value):
```rust
const MAGLEV_MAX_TABLE_SIZE: u64 = 5_000_011;
if cluster.lb_policy == LbPolicy::Maglev
    && let Some(cfg) = cluster.maglev_lb_config.as_ref()
{
    if cfg.table_size > MAGLEV_MAX_TABLE_SIZE {
        return Err(crate::ConfigError::MaglevTableSizeTooLarge {
            cluster: cluster.name.clone(), table_size: cfg.table_size });
    }
    if !is_prime(cfg.table_size) {
        return Err(crate::ConfigError::MaglevTableSizeNotPrime {
            cluster: cluster.name.clone(), table_size: cfg.table_size });
    }
}
```
`is_prime` (trial division — `sqrt(5000011) ≈ 2236`, cheap):
```rust
fn is_prime(n: u64) -> bool {
    if n < 2 { return false; }
    if n % 2 == 0 { return n == 2; }
    let mut d = 3u64;
    while d * d <= n { if n % d == 0 { return false; } d += 2; }
    true
}
```

- [ ] **Step 4: Run, verify pass** — `cargo test -p envoy-config maglev` → PASS.

- [ ] **Step 5: Commit** — `phase 29: Task 3 — MAGLEV table_size validation (prime/over-max, gated) [ADR-0072]`.

---

## Task 4: `maglev.rs` — the table build + lookup (load-bearing)

**Files:**
- Create: `crates/envoy-cluster/src/maglev.rs`
- Modify: `crates/envoy-cluster/src/lib.rs` (add `mod maglev;`)
- Test: `maglev.rs` `#[cfg(test)] mod tests` (the §A pinned oracle)

This is the correctness heart. The module MUST reproduce the §A algorithm byte-for-byte (the oracle test is the gate).

- [ ] **Step 1: Write the failing pinned-oracle test FIRST** — encode the §A regression oracle. Build over the two `ip:port` hosts at `table_size = 65537`, then assert every `(key, host)` pair from the §A table via `table.lookup(xxh64(key))`. Add an `M=17` small case and the empty-input host. Also assert the full-table host counts (32769 / 32768).

- [ ] **Step 2: Run, verify fail** — module doesn't exist yet.

- [ ] **Step 3: Implement:**
```rust
//! Maglev consistent-hashing lookup table for the `MAGLEV` load balancer.
//!
//! §6.2-LOCKED (ADR-0072; replica reproduced live Envoy v1.33.0 80/80 at the
//! default table_size, 64/64 at M=17). Per-host permutation from TWO xxHash64
//! invocations of the host `ip:port` string (NO `_i` suffix — unlike the ring):
//!   offset = xxh64_seed(key, 0) % M
//!   skip   = xxh64_seed(key, 1) % (M - 1) + 1        // seed 1 is load-bearing
//!   permutation[j] = (offset + j*skip) % M
//! Populate by the round-robin claim loop in host (config) order; earlier host
//! wins contention. Lookup = table[request_hash % M]. See the pinned-oracle test.
use crate::xxhash::{xxh64, xxh64_seed};

#[derive(Debug)]
pub(crate) struct MaglevTable {
    table: Vec<usize>, // length M; entry = host index into the build address slice
    table_size: u64,   // M (prime)
}

impl MaglevTable {
    /// Build over `addresses` (host index i = addresses[i]) for prime `table_size`.
    /// Empty `addresses` → empty table (lookup returns None).
    pub(crate) fn build(addresses: &[String], table_size: u64) -> MaglevTable {
        let m = table_size as usize;
        let n = addresses.len();
        if n == 0 || m == 0 {
            return MaglevTable { table: Vec::new(), table_size };
        }
        let offset: Vec<u64> = addresses.iter()
            .map(|a| xxh64_seed(a.as_bytes(), 0) % table_size).collect();
        let skip: Vec<u64> = addresses.iter()
            .map(|a| xxh64_seed(a.as_bytes(), 1) % (table_size - 1) + 1).collect();
        let mut next = vec![0u64; n];      // per-host permutation cursor j
        let mut table = vec![usize::MAX; m];
        let mut filled = 0usize;
        loop {
            for host in 0..n {
                // claim this host's next unclaimed permutation slot
                let mut c = ((offset[host] + next[host] * skip[host]) % table_size) as usize;
                while table[c] != usize::MAX {
                    next[host] += 1;
                    c = ((offset[host] + next[host] * skip[host]) % table_size) as usize;
                }
                table[c] = host;
                next[host] += 1;
                filled += 1;
                if filled == m {
                    return MaglevTable { table, table_size };
                }
            }
        }
    }

    /// Look up the host index for `key_hash` (the `xxh64` seed-0 of the request
    /// hash material). Returns `None` only for an empty table.
    pub(crate) fn lookup(&self, key_hash: u64) -> Option<usize> {
        if self.table.is_empty() {
            return None;
        }
        Some(self.table[(key_hash % self.table_size) as usize])
    }
}
```
> **Note on `next[host] * skip[host]`:** `next` and `skip` are `u64`; with `skip < M ≤ 5000011` and `next` bounded by the claim loop, the product never overflows `u64` in practice, but use `wrapping_mul`-free plain `*` and rely on the `% table_size` — if a clippy/overflow concern arises, cast through `u128` for the multiply. Keep the modular result identical to §A.

Add `mod maglev;` to `crates/envoy-cluster/src/lib.rs` (it will be `#![allow(dead_code)]`-clean once Task 5 consumes it; until then mirror the xxhash `#![allow(dead_code)]` precedent if the lint fires).

- [ ] **Step 4: Run, verify pass** — `cargo test -p envoy-cluster maglev::` → oracle PASS (every key maps to the §A host; counts match).

- [ ] **Step 5: Commit** — `phase 29: Task 4 — maglev.rs table build + lookup (§6.2-LOCKED oracle) [ADR-0072]`.

---

## Task 5: M28-3 discriminator refactor — `hash_lb: Option<HashLb>`

**Files:**
- Modify: `crates/envoy-cluster/src/cluster.rs` (`ring` field :209; `pick()` dispatch :355–377; `from_bootstrap` build :1379–1389; the footgun comment :1375–1378; the `ring: None` / `ring: Some(ring)` struct literals at :1520, :1558, and test literals :2057, :2666, :3080)
- Test: existing `cluster.rs` tests stay green (RR + RING_HASH byte-identical); add the dispatch witness in Task 7.

The phase-28 footgun comment at `cluster.rs:1375` EXPLICITLY prescribes this. Replace the single-ring discriminator with an enum so MAGLEV and RING_HASH coexist.

- [ ] **Step 1:** Introduce the enum + field (replace `ring: Option<crate::ring_hash::HashRing>` at :209):
```rust
/// 29 (ADR-0071): the consistent-hash LB dispatch. `Some` iff `lb_policy` is a
/// hash policy (built in `from_bootstrap`); `None` for ROUND_ROBIN (the cursor
/// path). Replaces the phase-28 `ring: Option<HashRing>` discriminator (M28-3):
/// `ring.is_some()` could not distinguish a SECOND ring-building policy.
pub(crate) hash_lb: Option<HashLb>,
```
```rust
#[derive(Debug)]
pub(crate) enum HashLb {
    Ring(crate::ring_hash::HashRing),
    Maglev(crate::maglev::MaglevTable),
}
```

- [ ] **Step 2:** Rewrite the `pick()` dispatch (:372) to match the variant; both lookups return `Option<usize>`, so the `host_index < total` guard is preserved verbatim:
```rust
if let (Some(hlb), Some(kh)) = (self.hash_lb.as_ref(), key_hash) {
    let host_index = match hlb {
        HashLb::Ring(r) => r.lookup(kh),
        HashLb::Maglev(t) => t.lookup(kh),
    };
    if let Some(hi) = host_index && hi < total {
        return Some(eps[hi]);
    }
}
```
Update the surrounding doc-comment (:355–371): the dispatch is now on `hash_lb` (Ring → ring lookup, Maglev → table lookup); a ROUND_ROBIN cluster has `hash_lb == None` and falls through with `key_hash` inert; the RING_HASH path is byte-identical to phase 28.

- [ ] **Step 3:** Rewrite the `from_bootstrap` build (:1379) — DELETE the footgun comment (now resolved) and build per `lb_policy`:
```rust
let addrs: Vec<String> = endpoints.iter().map(|a| a.to_string()).collect();
let hash_lb = match cfg.lb_policy {
    envoy_config::LbPolicy::RingHash => {
        let min_ring_size = cfg.ring_hash_lb_config.as_ref()
            .map(|c| c.minimum_ring_size).unwrap_or(1024);
        Some(HashLb::Ring(crate::ring_hash::HashRing::build(&addrs, min_ring_size)))
    }
    envoy_config::LbPolicy::Maglev => {
        let table_size = cfg.maglev_lb_config.as_ref()
            .map(|c| c.table_size).unwrap_or(65537);
        Some(HashLb::Maglev(crate::maglev::MaglevTable::build(&addrs, table_size)))
    }
    envoy_config::LbPolicy::RoundRobin => None,
};
```
Replace every `ring: None` → `hash_lb: None` and `ring: Some(ring)` → `hash_lb` (the built value) in the struct literals and test fixtures (:1520, :1558, :2057, :2666, :3080 — grep `\bring:` to find them all).

- [ ] **Step 4: Run** — `cargo test -p envoy-cluster` → ALL existing tests PASS (round-robin AND the RING_HASH fixture-0036-equivalent unit tests byte-identical — the no-op proof). `cargo clippy -p envoy-cluster --all-targets -- -D warnings` clean.

- [ ] **Step 5: Commit** — `phase 29: Task 5 — M28-3 discriminator refactor (hash_lb: Option<HashLb>)`.

---

## Task 6: Differential fixture `0037-lb-maglev` (STRONG)

**Files:**
- Create: `tests/fixtures/0037-lb-maglev/{envoy.yaml, envoy-rust.yaml, README.md, inputs/, expectations.yaml}` — clone `tests/fixtures/0036-lb-ring-hash/` and change `lb_policy: RING_HASH` → `MAGLEV` (drop `ring_hash_lb_config`; optionally add `maglev_lb_config: { table_size: 65537 }`). Keep the `{{BACKEND_IP}}` placeholders and the two `--body-marker` backends IDENTICAL to 0036.
- Modify: `tests/differential/src/lib.rs` — add the `0037` key-sweep driver (clone the 0036 driver).

> **MEMORY (`consistent-hash-lb-differential-needs-identical-endpoint-strings`):** the Maglev table is `ip:port`-string-sensitive — BOTH proxies MUST build it from ONE shared host-LAN-IP via `{{BACKEND_IP}}` / `discover_host_lan_ip`, NEVER a per-side IP split. This is non-negotiable for the STRONG target.

- [ ] **Step 1:** Create the fixture (clone 0036; flip the policy). Write `README.md` noting the §A algorithm + STRONG target + the IP-string-sensitivity.
- [ ] **Step 2:** Add the differential test fn (clone the 0036 key-sweep): send a sweep of distinct `x-hash-key` values; per value assert envoy-rust's backend (by body marker) == Envoy's backend; assert same-key→same-backend stability; assert both backends are hit (distribution).
- [ ] **Step 3: Run** the differential locally (this Docker-Desktop host — no Linux-CI dependency; maglev has no reload trigger). `cargo test -p differential maglev` (or the harness's invocation) → GREEN; all of `0001`–`0036` still green.
- [ ] **Step 4: Commit** — `phase 29: Task 6 — fixture 0037-lb-maglev differential (STRONG, cross-proxy identical selection)`.

---

## Task 7: In-process backstop tests

**Files:**
- Test: `crates/envoy-cluster/src/maglev.rs` + `crates/envoy-cluster/src/cluster.rs` `mod tests`

Cover what the differential cannot exercise deterministically on this host:
- [ ] table determinism (build twice → identical table); same-key→same-host; distinct keys spread across both hosts.
- [ ] single-host table (n=1 → every lookup → host 0).
- [ ] the **M28-3 regression witness**: a `MAGLEV` cluster's `pick(Some(kh))` returns the §A host; a `RING_HASH` cluster's `pick` is byte-identical to phase 28; a `ROUND_ROBIN` cluster's `pick` ignores `key_hash` (cursor path).
- [ ] the no-hash-key fallback: `pick(None)` → cursor path (matches M28-2); empty-but-present value hashes (`pick(Some(xxh64(b"")))` deterministic).
- [ ] **Commit** — `phase 29: Task 7 — maglev + M28-3 backstop tests`.

---

## Task 8: Fuzz seed + BEHAVIOR_CONTRACT + M28-1 fold

**Files:**
- Create: a `parse_bootstrap` fuzz seed corpus file exercising `maglev_lb_config` (under the existing `crates/envoy-config/fuzz/` corpus dir — match the phase-28 ring seed's location/shape; NO new fuzz target).
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — extend the "LB selection" section with a MAGLEV row (the §A algorithm summary + STRONG target + the dispositions); **fold M28-1**: a sentence that `RingHashLbConfig.maximum_ring_size` is parse-validation-only (the ring build is `minimum_ring_size`-governed).

- [ ] **Step 1:** Add the fuzz seed (a minimal valid bootstrap with a `MAGLEV` cluster + `maglev_lb_config`). Run `cargo fuzz run parse_bootstrap -- -runs=<short>` (CI short-budget) → clean.
- [ ] **Step 2:** Extend BEHAVIOR_CONTRACT.md (MAGLEV row + M28-1 sentence).
- [ ] **Step 3: Commit** — `phase 29: Task 8 — parse_bootstrap maglev fuzz seed + BEHAVIOR_CONTRACT MAGLEV row (M28-1 folded)`.

---

## §7.5 phase-done gate (verified at state-4)

(a) fixture `0037` green + (b) all `0001`–`0036` green + (c) h2spec ≥95% (unchanged) + (d) the `parse_bootstrap` fuzz seed clean + (e) `cargo build --workspace --all-targets` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds (D-3.8).

_Scope locked by ADR-0071; the §6.2 reconciliation locked by ADR-0072. ADR-0073 (split) did NOT fire. Next state-3 skill: `superpowers:subagent-driven-development`._
