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

- [ ] **Task 1** — Seeded xxHash64 (`xxh64_seed(data, seed)`; `xxh64 = seed 0` byte-identical).
- [ ] **Task 2** — Config: `LbPolicy::Maglev` + `MaglevLbConfig { table_size }` + `maglev_lb_config` field.
- [ ] **Task 3** — Validation: `MaglevTableSizeNotPrime` / `MaglevTableSizeTooLarge`, MAGLEV-gated (`is_prime`). [ADR-0072]
- [ ] **Task 4** — `maglev.rs`: `MaglevTable::build` + `lookup` (§A oracle, the correctness gate). [ADR-0072]
- [ ] **Task 5** — M28-3 refactor: `ring: Option<HashRing>` → `hash_lb: Option<HashLb>` (`HashLb { Ring, Maglev }`).
- [ ] **Task 6** — Fixture `0037-lb-maglev` differential (STRONG; `{{BACKEND_IP}}` shared-IP).
- [ ] **Task 7** — Backstop tests (determinism, spread, fallback, single-host, M28-3 regression witness).
- [ ] **Task 8** — `parse_bootstrap` maglev fuzz seed + BEHAVIOR_CONTRACT MAGLEV row + M28-1 fold.

## Carry-forwards consumed / produced

- **M28-3** (the `ring.is_some()` discriminator → explicit hash-LB dispatch): CONSUMED by Task 5.
- **M28-1** (the `maximum_ring_size`-is-parse-validation-only doc sentence): folded into Task 8 (BEHAVIOR_CONTRACT).
- **M28-2** (no-hash-key fallback characterization): reconfirmed by §6.2 (header-absent → cursor path); backstop-covered (Task 7), not a differential assertion.

## Log

_(empty — first state-3 session appends here per task.)_
