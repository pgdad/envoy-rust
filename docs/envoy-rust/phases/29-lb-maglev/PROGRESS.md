# Phase 29 — `29-lb-maglev` — PROGRESS

> Running log, updated by the executor on each task completion (state-3
> `superpowers:subagent-driven-development`). Append command outputs at the
> state-4 verification gate (`superpowers:verification-before-completion`).
> Plan: `PLAN.md`. Spec: `SPEC.md`. Reconciliation: ADR-0072 (§6.2-locked).

## Status

**Lifecycle state:** state-2 PLAN-write COMPLETE → state-3-next (implementation).
**§6.2 reconnaissance:** DONE at the PLAN-write — algorithm §6.2-LOCKED in `PLAN.md §A`
(replica reproduced live Envoy v1.33.0 **80/80** at default `table_size`, **64/64** at M=17;
STRONG differential target confirmed). ADR-0072 landed.
**§6.1 split:** NOT fired (~8 tasks / ~450–550 LoC, well under the gate). ADR-0073 unused.

## Task checklist

- [x] **Task 1** — Seeded xxHash64 (`xxh64_seed(data, seed)`; `xxh64 = seed 0` byte-identical). DONE (`40f4e39`).
- [x] **Task 2** — Config: `LbPolicy::Maglev` + `MaglevLbConfig { table_size }` + `maglev_lb_config` field. DONE (`b6b4292`).
- [x] **Task 3** — Validation: `MaglevTableSizeNotPrime` / `MaglevTableSizeTooLarge`, MAGLEV-gated (`is_prime`). [ADR-0072] DONE (`36dd94c`).
- [x] **Task 4** — `maglev.rs`: `MaglevTable::build` + `lookup` (§A oracle, the correctness gate). [ADR-0072] DONE (`0f8b1be`).
- [x] **Task 5** — M28-3 refactor: `ring: Option<HashRing>` → `hash_lb: Option<HashLb>` (`HashLb { Ring, Maglev }`). DONE (`06e6645`).
- [ ] **Task 6** — Fixture `0037-lb-maglev` differential (STRONG; `{{BACKEND_IP}}` shared-IP).
- [ ] **Task 7** — Backstop tests (determinism, spread, fallback, single-host, M28-3 regression witness).
- [ ] **Task 8** — `parse_bootstrap` maglev fuzz seed + BEHAVIOR_CONTRACT MAGLEV row + M28-1 fold.

- **Task 2 (`b6b4292`)** — Added the MAGLEV config surface to `crates/envoy-config/src/bootstrap.rs`: `LbPolicy::Maglev`, `MaglevLbConfig { table_size }` (serde `deny_unknown_fields`, default 65537 via `default_maglev_table_size`), `maglev_lb_config: Option<MaglevLbConfig>` on `Cluster` — exact mirror of the `RingHashLbConfig` pattern. 5 parse tests via `parse_bootstrap` (variant / empty→65537 / absent→None / explicit / accept-and-ignore on a ROUND_ROBIN cluster). No validators, no table logic (YAGNI). No match-arm breakage (the only `LbPolicy` consumer is `cluster.rs:1379` `if == RingHash`). Spec ✅; quality APPROVED (0C/0I/3 Minor). **Carry to Task 3:** strengthen `accepts_maglev_lb_config_on_non_maglev_cluster` to feed a NON-PRIME `table_size` on a non-MAGLEV cluster (proving validation is genuinely gated, mirroring the ring precedent's teeth — Minor #1).

- **Task 3 (`36dd94c`)** — MAGLEV `table_size` validation. Two `ConfigError` variants (`MaglevTableSizeNotPrime`, `MaglevTableSizeTooLarge`) in `lib.rs`; a MAGLEV-gated block in `validate_cluster` (`bootstrap.rs`) checking over-max (>5_000_011) BEFORE primality (so `is_prime`'s bounded trial loop never runs huge); the `is_prime` helper (trial division to √n; clippy required `is_multiple_of`). Gated to MAGLEV clusters → `maglev_lb_config` on a non-MAGLEV cluster is accept-and-ignored (Envoy parity + ring precedent). Strengthened `accepts_maglev_lb_config_on_non_maglev_cluster` to feed a non-prime `table_size: 100` on a ROUND_ROBIN cluster (the real gating proof — Task-2 Minor #1 consumed). Tests: both rejections (via `matches!`), the 5000011 boundary, the 65537 default, `is_prime` units. Spec ✅ (5000011 independently confirmed prime); quality APPROVED (0C/1 Important/2 Minor). **Folded:** the Important fmt collapse (`cargo fmt`) + the doc-comment fix — the fold caught that the `is_prime` insertion had displaced `validate_cluster`'s doc-comment; relocated it back (cosmetic, no logic). `cargo fmt -p envoy-config --check` now passes.

- **Task 4 (`0f8b1be`)** — Created `crates/envoy-cluster/src/maglev.rs`: `MaglevTable::build(addresses, table_size)` + `lookup(key_hash) -> Option<usize>`, the §A-LOCKED algorithm byte-for-byte (offset=seed0%M, skip=seed1%(M-1)+1, permutation (offset+j·skip)%M, config-order round-robin claim, lookup table[hash%M]). `mod maglev;` added to `lib.rs`. **Pinned oracle test reproduces live Envoy 24/24** + distribution 32769/32768 + single-host/empty/determinism (5/5). Full envoy-cluster suite 133/133. Also added `maglev_lb_config: None` to 4 cluster.rs test literals (compile fix for Task 2's field — latent because Task 2 only ran `cargo build`, not the cluster tests). Spec ✅ with an **INDEPENDENT python replica re-deriving all 24 pairs + the distribution** (matches an independent impl of the locked algorithm, not just itself). Quality APPROVED (0C/0I/3 Minor optional) — termination (prime M + coprime skip → full cyclic permutation), OOB-safety, `usize::MAX` sentinel, and overflow all rigorously verified. No fold needed.

- **Task 5 (`06e6645`)** — The M28-3 discriminator refactor (`crates/envoy-cluster/src/cluster.rs`). Replaced `ring: Option<HashRing>` with `hash_lb: Option<HashLb>` (`HashLb { Ring(HashRing), Maglev(MaglevTable) }`, `pub(crate)`). `pick()` matches the variant (both `lookup` return `Option<usize>` → the `hi < total` guard preserved); `from_bootstrap` builds via an EXHAUSTIVE `match cfg.lb_policy` (RingHash→Ring/1024, Maglev→Maglev/65537, RoundRobin→None — no wildcard, so a future LB variant is a compile error, structurally killing the footgun). `addrs` hoisted + shared by both arms. Deleted the footgun guard comment; converted all `ring:` literals → `hash_lb:`; removed `maglev.rs`'s `#![allow(dead_code)]` (now consumed). **Behavior-preserving: 133→133 tests green** (RING_HASH + round-robin byte-identical), clippy+fmt clean, envoy-bin builds. Spec ✅ (fall-through semantics confirmed identical); quality APPROVED (0C/0I/2 Minor, pre-existing conventions). M28-3 carry-forward CONSUMED.

## Carry-forwards consumed / produced

- **M28-3** (the `ring.is_some()` discriminator → explicit hash-LB dispatch): CONSUMED by Task 5.
- **M28-1** (the `maximum_ring_size`-is-parse-validation-only doc sentence): folded into Task 8 (BEHAVIOR_CONTRACT).
- **M28-2** (no-hash-key fallback characterization): reconfirmed by §6.2 (header-absent → cursor path); backstop-covered (Task 7), not a differential assertion.

## Log

- **Task 1 (`40f4e39`)** — Generalized `crates/envoy-cluster/src/xxhash.rs` `xxh64` → seeded `xxh64_seed(data, seed)`; `xxh64(d) = xxh64_seed(d, 0)` wrapper. Seed threads into the four lane initializers + the `<32` branch (pure `SEED`→`seed` substitution; seed-0 path provably byte-identical — the 5 LOCKED phase-28 vectors unchanged + green). Added `seed1_host_key` (`xxh64_seed(b"172.31.0.2:5678", 1) == 0x57A5_6BCE_7E3A_E555`, independently generated via python xxhash 3.7.0) + `seed0_equiv` (empty/short/≥32-byte). TDD (failing test first); 7/7 green; clippy clean. Spec review ✅ (seed-1 value independently re-derived, non-circular). Quality review APPROVED (0C/0I/3 Minor — all folded: wrapper doc, reproducer-command comment, ≥32-byte equivalence assertion).
