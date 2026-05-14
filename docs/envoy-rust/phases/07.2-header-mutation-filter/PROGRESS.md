# Phase 07.2 (`07.2-header-mutation-filter`) — PROGRESS

> Per-task narrative log. CREATED at the state-2 standalone-PLAN.md commit with
> the Task 1 preamble (the `dc00750` 06.2 cadence — back to the 06.x norm;
> divergence from 07.1's "PROGRESS created at Task 1"). Each subsequent task
> commit appends a per-task section: work summary, tests landed (names + LoC
> tally), per-task deviations from PLAN (D-3.5 append-only discipline), LoC
> delta, and the test-bucket attestation. Stranger-readable per D-3.4.

## Task 1 preamble — PLAN-write SPEC corrections + architecture-decision lock-ins

This preamble lands at the state-2 standalone-PLAN.md commit (alongside
`PLAN.md`); it carries NO code. The Task 1 *implementation* narrative appends
below it at Task 1's own state-3 commit.

### PLAN-write SPEC corrections (8)

The 07.2 SPEC landed at the parent-07 state-2 split commit `6db5a01`, BEFORE
the 07.1 execution arc. Eight SPEC details drifted against the 07.1-landed
tree (verified against HEAD `3abcc8c`). Per the user's standing preference
`feedback_pick_recommendation`, each correction picks the working option; all
are folded into the PLAN's task steps. Full text in PLAN.md "PLAN-write SPEC
corrections" — summarized here:

1. **`header_mutation.rs` uses `FilterRequest` / `FilterResponse`**, not
   `envoy_http1::codec::{Request, Response}`. ADR-0031 (07.1 Task 5.5) re-homed
   filter-visible types into `envoy-filter::types` and removed `envoy-http1`
   from `envoy-filter`'s deps. The SPEC §3 Task 3/4 code blocks predate ADR-0031.
2. **Signpost 2's `#[cfg(test)]` test-only `HttpFilterInstance` variant does not
   work cross-crate** — `#[cfg(test)]` in `envoy-filter` is not active when
   `envoy-http1` / `envoy-http2` compile their test suites. Use the SPEC's own
   documented "Alternative — visible-via-feature-flag": a `test-util` Cargo
   feature on `envoy-filter` (Task 5 group B). Within the SPEC's offered option
   space — not ADR-worthy.
3. **Task 5's "deferred test stubs 3-7" are net-new tests**, not stubs to fill
   in. The 07.1 commits `84d68c1` / `3e041c5` deferred *writing* them to 07.2
   Task 5; no placeholder functions exist in the `hcm.rs` files.
4. **Fixture 0013's `expectations.yaml` mirrors fixture 0008's actual shape**
   (`driver: { kind: http1, method, path, host, expected_status, expected_body:
   { kind: byte_exact, body }, expected_headers: set_equal_modulo_allow_list }`
   + `equivalence:` block), not the SPEC §3 Task 8 sketch.
5. **No existing RFC 7230 token helper to reuse** — Task 2 lands
   `is_valid_rfc7230_token` inline in `bootstrap.rs` (the 04.2 HeaderMatcher
   work referenced RFC 7230 in comments only; no token-set *validator* exists).
6. **`ConfigError` lives in `crates/envoy-config/src/lib.rs`**, not
   `bootstrap.rs` — the 3 new variants append to `lib.rs`; the validator
   function + helper land in `bootstrap.rs` (mirrors the 07.1 Task 4 split).
7. **New schema types follow the existing derive convention** —
   `#[derive(Debug, Deserialize, PartialEq)]` + `#[serde(deny_unknown_fields)]`
   (not `Clone` / `Serialize` as the SPEC §3 Task 1 block shows); `AppendAction`
   adds `Clone, Copy, Eq`; the existing `HttpFilterTypedConfig` keeps its
   `#[serde(tag = "@type", deny_unknown_fields)]`.
8. **The `http1-echo-server` helper is a standalone subprocess binary**, not a
   library exposing `serve_ephemeral()` — Task 9's in-process backstop follows
   the `crates/envoy-bin/tests/http1_router_upstream.rs` precedent (inline
   upstream + `format!` YAML + `tempfile::tempdir()` + `CARGO_BIN_EXE_envoy-bin`);
   its inline upstream echoes request headers into the body.

### Architecture-decision lock-ins (per `feedback_pick_recommendation`)

All 10 SPEC §6 signposts + 6 additional decisions locked at PLAN-write time —
full table in PLAN.md "Architecture decisions locked at PLAN-write time".
Headline picks: no RFC 7230 helper exists → land inline at Task 2 (signpost 1);
`test-util` Cargo feature for the StopAndSend stubs (signpost 2); `Vec`-held
mutation lists, not `Arc` (signpost 3); lowercase-key-at-build-time normalization
(signpost 4); pseudo-header mutation out-of-scope, diff-equivalent no-op
(signpost 5); slice-order `apply_mutations` (signpost 8); helper already echoes
sorted headers — Task 7 verify-only (signpost 10). Carryforwards: **07.1 REVIEW
I1** (`finalize_h2_stream` 3-dead-parameter cleanup) is the named structural
prerequisite of Task 5 (Step group A); **07.1 REVIEW M1** (unused `tracing` dep)
closes at Task 3; **07.1 REVIEW M2** (`UnsupportedFilterType` constructable)
partially closes at Task 3 (`RouterNotTerminal` / `DuplicateRouter` stay
defense-in-depth-only). **No new ADRs** — ledger head stays ADR-0031; ADR-0032
reserved-available. **No new top-level Cargo deps.** **Every code-changing
task's PROGRESS attestation MUST quote `cargo deny check` output** (07.1-REVIEW
doctrine reminder — 07.1 CI run `25758889478` failed at `cargo deny check`).

### Split-gate evaluation

10 tasks (< 25-task gate). ~1600 LoC projected (production ~440; tests ~740;
fixture/doc ~410) — ~+7% over the ~1500-LoC soft gate, test/fixture-concentrated.
**Accept the drift; do NOT nest-split** — parent-07 SPEC §5 + ADR-0030 reject
nested splits of a split-produced sub-phase; the 06.x accept-drift precedent
(06.1 SPEC ~1300 → PLAN ~2010 LoC) ratifies. In-execution release valve if a
task inflates past ~10 sub-steps: per-step commit splitting recorded in PROGRESS
(e.g. Task 5a/5b/5c), NOT a phase-level nest-split.

### LoC ground-truth (per 07.2 SPEC §3 task budgets)

Task 1 ~200 + Task 2 ~170 + Task 3 ~210 + Task 4 ~185 + Task 5 ~300 + Task 6 ~52
+ Task 7 ~0-30 + Task 8 ~290 + Task 9 ~150 + Task 10 ~30 = ~1587-1617 LoC of
net change across the substantive surface. PLAN.md narrative overhead is
separate (~2900 lines). State-4 re-checkpoint at Task 10.

---

<!-- Task 1 implementation narrative appends here at Task 1's state-3 commit. -->
