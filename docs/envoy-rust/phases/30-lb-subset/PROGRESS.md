# Phase 30 — `30-lb-subset` — PROGRESS

> Running log, updated by the executor on each task completion (state-3
> `superpowers:subagent-driven-development`). Append command outputs at the
> state-4 verification gate (`superpowers:verification-before-completion`).
> Plan: `PLAN.md`. Spec: `SPEC.md`. Reconciliation: ADR-0074 (§6.2-locked).

## Status

**Lifecycle state:** **state-4 verification COMPLETE → state-5-next** (code review). The §7.5 phase-done
gate (a)-(e) is **GREEN at the AUTHORITATIVE Linux CI run [`27881837635`](https://github.com/pgdad/envoy-rust/actions/runs/27881837635)
@ HEAD `1acf78c`** (both jobs ✓; see the §7.5 evidence block below). `REVIEW.md` ABSENT → the next step
is the state-5 `superpowers:requesting-code-review`.
All 9 `PLAN.md` tasks were implemented TDD-per-task, each dispatched to a fresh implementer subagent
(SERIAL), two-stage-reviewed (spec-compliance THEN code-quality via fresh `superpowers:code-reviewer`
subagents), each committed separately on `main`. **The fixture-0038 differential RAN LOCALLY → GREEN**
(subset LB is locally observable; Docker `envoyproxy/envoy:v1.33.0`) and re-confirmed on Linux/Docker in CI.
`cargo fmt --all -- --check` was clean locally at the state-3 close (the `envoy-rust-state4-ci-first-execution`
discipline — pre-empted the mid-phase fmt red), and the run was green on the first push.

## §7.5 verification gate (state-4 — CI's first full execution; AUTHORITATIVE = Linux CI)

**CI run [`27881837635`](https://github.com/pgdad/envoy-rust/actions/runs/27881837635) (push, `main` @ `1acf78c`) — both jobs ✓ success:**
- **`build + test + lint`** (4m2s, job `82510649488`) ran, per `.github/workflows/ci.yml`: `cargo fmt --all -- --check` → clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean; `cargo build --workspace --all-targets` → `Finished` ok; h2spec (conformance; unchanged — no H2 codec change this phase); `cargo test --workspace` (**includes the Docker differential harness**, pre-pulling upstream `envoyproxy/envoy:v1.33.0`) → **0 failed workspace-wide**; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.
- **`fuzz`** (2m10s, job `82510649489`) ran `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` (corpus now incl. the new `cluster_lb_subset.yaml` seed) + `jwt_parse` → clean (no crash).

**§7.5 (a)-(e) mapping (evidence quoted from the CI test-job log):**
- **(a) new differential fixture green** — `test lb_subset_fixture ... ok` (fixture `0038-lb-subset` cross-proxy route-selection STRONG witness — `/prod`→prod, `/canary`→canary, `/nope`→503 NO_FALLBACK; on Linux/Docker vs live Envoy v1.33.0; node `envoy-rust-phase-30-fixture-0038`).
- **(b) pre-existing differential fixtures green** — all other fixture binaries `... ok`, incl. `lb_maglev_fixture` (0037) + `lb_ring_hash_fixture` (0036) + the `0001`–`0035` family (40 fixture/fixture-expectation tests total, `0 failed`); NO `FAILED`/`panicked`/`error[` anywhere in the run. The no-`lb_subset_config` no-op is a verified pass-through (ROUND_ROBIN + the consistent-hash fixtures behavior-identical).
- **(c) conformance** — `test h2spec_pass_rate_gate ... ok` (the ≥95% gate asserted green in the `build + test + lint` job); unchanged — no HTTP/2 codec change this phase.
- **(d) fuzz clean** — the `fuzz` job ✓; `parse_bootstrap` `Done 195277 runs in 31 second(s)` (cov 11048, corpus incl. the new `cluster_lb_subset.yaml` seed); `jwt_parse` `Done 4461621 runs in 31 second(s)`. NO new fuzz target.
- **(e) build/clippy/fmt/test/deny** — all clean in the green `build + test + lint` job (representative workspace counts: envoy lib 146 passed / 0 failed / 2 ignored; the differential lib + per-fixture integration binaries all `0 failed`; the differential summary binary `16 passed; 0 failed`).
- (f) `REVIEW.md` approved is the state-5 step (not part of state-4).

No CI iteration was needed — the run was green on the first (state-3) push (`cargo fmt --all -- --check` had already been run clean locally at the state-3 close, pre-empting the usual mid-phase fmt red). The known pre-existing flake `differential::tests::drive_http2_round_trip_against_in_process_listener` did NOT fire. Open carry-forwards (M29-1/M29-2 + M30-1 differential-driver wording/`extract_marker` fold; M30-2 `lb_policy` serde-default divergence) are cosmetic/future and do NOT affect any gate.

**ADRs this phase:** ADR-0073 (scope, state-1), ADR-0074 (§6.2 lock, state-2), **ADR-0075 (state-3 Task-2
correction — `default_subset` is a flat `google.protobuf.Struct`, not nested `core.v3.Metadata`;
consumed the reserved-but-UNFIRED §6.1-split slot)**. Ledger head: **ADR-0075** (count 76; next ADR-0076).

**§6.2 reconnaissance:** DONE at the PLAN-write — algorithm §6.2-LOCKED in `PLAN.md §A`
(STRONG cross-proxy differential target confirmed live vs Envoy v1.33.0). ADR-0074 landed.
**§6.1 split:** NOT fired (~9 tasks / ~900–1100 LoC, under the gate). ADR-0075 reserved + UNFIRED.

## Task checklist

- [x] **Task 1** — `LbMetadata` + endpoint `metadata` config (`bootstrap.rs`). DONE (`9e6eb6e` + follow-up `6b9c2c7`).
- [x] **Task 2** — `Cluster.lb_subset_config` (accept-all, NO fatal validator) [ADR-0074]. DONE (`45a2ec7` + correction `dd3c2c0` [ADR-0075]). Also Task-1 compile-fix `5548f8a`.
- [x] **Task 3** — route `metadata_match` config. DONE (`42d0442`).
- [x] **Task 4** — `subset.rs` index build + resolve (§6.2-LOCKED oracle — the correctness gate) [ADR-0074]. DONE (`30110f3` + fix `90f82de`).
- [x] **Task 5** — `pick()` subset narrowing (eligible-set, no-op when absent). DONE (`22f6eb90` + M-1 follow-up `d405f74`).
- [x] **Task 6** — HCM route `metadata_match` threading (H1 + H2). DONE (`9890554`).
- [x] **Task 7** — fixture `0038-lb-subset` differential (STRONG; `/prod`→prod, `/canary`→canary, `/nope`→503). DONE (`b134cad`). **RAN LOCALLY → GREEN.**
- [x] **Task 8** — subset + no-op backstop tests. DONE (`35a49b5`).
- [x] **Task 9** — `parse_bootstrap` subset fuzz seed + BEHAVIOR_CONTRACT subset row. DONE (`2783e85`).

## Per-task log

### Task 1 — `LbMetadata` + endpoint `metadata` config — DONE
- Commits: `9e6eb6e` (impl), `6b9c2c7` (review-minor follow-up).
- `crates/envoy-config/src/bootstrap.rs`: added `LbMetadata { envoy_lb: BTreeMap<String,String> }`
  parsed via a `#[serde(from = "MetadataWire")]` shim that pulls ONLY the `envoy.lb` namespace
  from `core.v3.Metadata.filter_metadata` (other namespaces parse-and-ignore); non-string scalars
  coerced to strings (`stringify_scalar`). Added `metadata: Option<LbMetadata>` to `LbEndpoint`
  (`deny_unknown_fields` preserved). Re-exported `LbMetadata` from `lib.rs`.
- Tests: 4 (envoy.lb parse; absent→None; non-envoy.lb namespace ignored; bool/number coercion→"true"/"2").
  `cargo test -p envoy-config lb_metadata` → 4 passed; full crate 459+ passed; clippy + fmt clean.
- Two-stage review: spec ✅ compliant; code-quality APPROVED (0C/0I/3 Minor). Folded Minor #1
  (Serialize-asymmetry doc note) + #3 (coercion test); Minor #2 (null-drop) recorded as a Task-4
  carry-note (live Envoy only emits strings → harmless).

### Task 1 compile-fix (controller, cross-cutting) — DONE
- Commit `5548f8a`. Task 1 added `LbEndpoint.metadata` but did not update the `LbEndpoint { … }`
  struct literals in `envoy-cluster`/`envoy-admin` test builders, breaking their `--tests`
  compilation (Task 1's reviews only ran `-p envoy-config`). Added `metadata: None` to the 4 sites
  (3 in `cluster.rs`, 1 in `admin/endpoint.rs`). Verified `cargo build -p envoy-cluster -p envoy-admin --tests` ok.

### Task 2 — `Cluster.lb_subset_config` (accept-all, NO fatal validator) — DONE
- Commits: `45a2ec7` (impl), `dd3c2c0` (correction [ADR-0075]).
- `bootstrap.rs`: added `LbSubsetFallbackPolicy` (SCREAMING_SNAKE enum, `#[default] NoFallback`/AnyEndpoint/DefaultSubset),
  `LbSubsetSelector { keys: Vec<String> }`, `LbSubsetConfig { fallback_policy, subset_selectors, default_subset }`;
  added `lb_subset_config: Option<LbSubsetConfig>` to `Cluster`. **NO `validate_cluster` block** (ADR-0074 #1:
  Envoy boots for all malformed subset configs). Exported the 3 types from `lib.rs`. Added `lb_subset_config: None`
  to the 4 `envoy_config::Cluster` test-builder literals in `cluster.rs`.
- **CORRECTION [ADR-0075]:** `default_subset` is Envoy's `google.protobuf.Struct` (FLAT `{key:value}`), NOT a nested
  `core.v3.Metadata`. The PLAN Task-2 snippet wrongly wrote `Option<LbMetadata>` (nested), contradicting its own
  §A-locked recon (flat). Changed to `Option<BTreeMap<String,String>>` with a scalar-stringifying `deserialize_with`
  shim → parses real-Envoy YAML (`default_subset: { stage: prod }`) AND yields the exact type Task-4's `SubsetIndex`
  expects. `LbEndpoint.metadata`/route `metadata_match` REMAIN nested `LbMetadata` (correct — those are `core.v3.Metadata`).
- Tests: 6 (round-trips for NO_FALLBACK default / ANY_ENDPOINT / DEFAULT_SUBSET+flat default_subset; absent→None;
  empty `subset_selectors:[]` → `validate_cluster` Ok; empty `keys:[]` → `validate_cluster` Ok). `cargo test -p envoy-config` 466 passed.
- Two-stage review: spec ✅ compliant; code-quality APPROVE-with-followup. Folded the one Important (default_subset
  wire shape → ADR-0075 correction). Two cosmetic Minors (doc "MAGLEV-style" phrasing; serialize note) NOT folded — no
  correctness impact.

### Task 3 — route `metadata_match` config — DONE
- Commit `42d0442`. Added `metadata_match: Option<LbMetadata>` to `RouteAction_Route` (after `hash_policy`);
  reuses nested `LbMetadata` (route `metadata_match` IS `core.v3.Metadata` — correct). 18 literals updated via
  compiler-driven completion: the 1 production deep-clone (`clone_route_action`, envoy-http1 hcm.rs) preserves
  the value via `ar.metadata_match.clone()`; 17 test literals get `None`. 2 tests (nested parse → Some{stage:prod};
  absent → None). `cargo test -p envoy-config` 468 passed; http1/http2 build + clippy + fmt clean.
- Two-stage review: spec ✅ compliant (incl. the load-bearing production-clone-preserves check); code-quality
  APPROVED (0C/0I/2 Minor — both optional test-thoroughness, covered by sibling tasks; not folded).

### Task 4 — `subset.rs` index build + resolve (§6.2-LOCKED oracle — correctness gate) — DONE
- Commits: `30110f3` (engine + oracle), `90f82de` (fix — multi-key value-tuple order alignment).
- Created `crates/envoy-cluster/src/subset.rs` (`mod subset;` in lib.rs). `SubsetIndex::build` groups each
  endpoint per `subset_selectors` entry by the SORTED-key value-tuple, EXCLUDING endpoints missing any selector
  key (Envoy parity); superset matching falls out (bucket keyed on selector keys only). `resolve(metadata_match)
  -> Eligible{All,Some(idxs),None}`: empty selectors → All (disabled-layer edge); else selector-key-SET match →
  value-tuple lookup → Some/fallback. fallback: NoFallback→None, AnyEndpoint→All, DefaultSubset→lookup(default)
  (None/empty default → All). `default_subset` consumed directly as flat `Option<BTreeMap>` (ADR-0075). `n` field
  + `LbMetadata` import dropped (unused). `#![allow(dead_code)]` until Task 5 consumes it.
- **SPEC-REVIEW BUG FOUND + FIXED:** the order-alignment trap — `build` built the value-tuple in `selector.keys`
  *declaration* order but `lookup` rebuilt it in `BTreeSet` *sorted* order → multi-key selectors whose declaration
  order ≠ sorted order silently mis-resolved (the single-key oracle couldn't catch it). Fix: `build` now builds the
  tuple from the sorted `keyset` (matching lookup). Added permanent regression test
  `multi_key_selector_tuple_order_independent` (`keys:[version,stage]`) — FAILED before / PASSES after.
- Tests: 12 (6 §A oracle rows + ANY_ENDPOINT + DEFAULT_SUBSET-empty + empty-selectors edge + the multi-key
  regression). `cargo test -p envoy-cluster` 150 passed; clippy + fmt clean.
- Two-stage review: spec ✅ compliant (after fix); code-quality APPROVED (0C/0I/4 Minor — micro-nits + tracked:
  I-2 endpoint-metadata index alignment → Task 5's job; missing-key/determinism backstops → Task 8's job).

### Task 5 — `pick()` subset narrowing (eligible-set, no-op when absent) — DONE
- Commits: `22f6eb90` (impl), `d405f74` (M-1 follow-up — factor shared `endpoint_eligible()`).
- `cluster.rs`: added `subset: Option<SubsetIndex>` field; `from_bootstrap` builds an index-aligned
  `endpoint_metadata: Vec<BTreeMap>` (I-2: pushed once per RESOLVED SocketAddr — Static/Eds once, StrictDns
  `resolved.len()` times; `debug_assert_eq!` guards alignment) → `SubsetIndex::build` only when `lb_subset_config`
  present. `pick()` gained `subset_match`; subset narrowing runs BEFORE the existing dispatch:
  `Eligible::None → return None` (503), `All`/absent → existing hash_lb/fast/slow path UNCHANGED (byte-identical
  no-op), `Some(idxs)` → ROUND_ROBIN cursor within the subset (hash_lb SKIPPED — subset+hash §2.2-deferred),
  intersected with HC/OD eligibility (I-1) + `i < total` guard. Removed `#![allow(dead_code)]` from subset.rs
  (now consumed). `pick_endpoint` signature threaded; ~70 callers pass `None` (compiler-driven; the 2 HCM sites
  pass `None` — real threading is Task 6).
- Tests: 2 new (`subset_match_is_inert_when_no_lb_subset_config` cross-cluster no-op witness;
  `subset_narrows_to_matched_endpoint` from_bootstrap prod/canary/none). `cargo test -p envoy-cluster` 152 passed;
  `lb_ring_hash` 6 passed; workspace builds; clippy + fmt clean.
- Two-stage review: spec ✅ (no-op invariant + I-2 alignment both verified); code-quality APPROVED (0C/0I/3 Minor).
  Folded M-1 (shared `endpoint_eligible()` helper). M-2 (multi-element subset rotation test) → Task 8; M-3 (path
  noise) cosmetic, skipped. NOTE for Task 8: add a multi-endpoint-subset rotation test (M-2).

### Task 6 — HCM route `metadata_match` threading (H1 + H2) — DONE
- Commit `9890554`. Added `subset_match: Option<BTreeMap<String,String>>` to `BuildOutcome::Proxy` (envoy-http1,
  reused by H2); populated at route-match from `ar.metadata_match.as_ref().map(|m| m.envoy_lb.clone())`; threaded
  to `run_attempt`/`run_h2_attempt` (`Option<&BTreeMap>`, per-attempt `.as_ref()` borrow in the retry loop);
  `pick_endpoint(request_hash_key, subset_match)`. Mirrors the phase-28 `request_hash_key` template. **M-2: H2
  502-on-pick-none path UNCHANGED** (H1 pick-none → 503; fixture 0038 is H1-only). `#[allow(clippy::too_many_arguments)]`
  on H1 run_attempt (8th arg) — documented, reviewer-endorsed (a params-struct refactor would obscure the mirror).
- Tests: 2 new H1 `build_response` tests (subset_match populated from metadata_match / None without). envoy-http1
  125 passed; envoy-http2 72 passed; workspace builds; clippy + fmt clean.
- Two-stage review: spec ✅ (incl. H2-502-path-unchanged check); code-quality APPROVED (0C/0I/3 cosmetic Minors,
  not folded).

### Task 7 — fixture `0038-lb-subset` differential (STRONG) — DONE, RAN LOCALLY GREEN
- Commit `b134cad`. Created `tests/fixtures/0038-lb-subset/{envoy.yaml, envoy-rust.yaml, expectations.yaml, README.md}`
  (cloned 0037 harness: `{{BACKEND_IP}}` shared-IP, two `--body-marker` backends; dropped MAGLEV/hash_policy; added
  endpoint `envoy.lb` metadata + `lb_subset_config` NO_FALLBACK selector `keys:[stage]` + three `metadata_match`
  routes `/prod`,`/canary`,`/nope`). NEW `Driver::Http1RouteSelect { probes: Vec<RouteSelectProbe> }` in
  `tests/differential/src/lib.rs` (NOT the hash-sweep): drives each path against both proxies, asserts cross-proxy
  identical `backend:<marker>` (STRONG) + the §A oracle (prod→backend_1, canary→backend_2) + the /nope 503
  `no healthy upstream`. Test binary `tests/differential/tests/lb_subset.rs`.
- **RAN LOCALLY (Docker `envoyproxy/envoy:v1.33.0`): `cargo test -p differential --test lb_subset` → 1 passed (all
  3 probes GREEN, cross-proxy identical).** No regression: lb_maglev + http1_router_upstream re-ran green. clippy+fmt clean.
- **Workaround:** envoy-rust's parser has NO serde default for `lb_policy` (Envoy defaults it to ROUND_ROBIN). Added
  explicit `lb_policy: ROUND_ROBIN` to BOTH yamls → configs identical (except bind addr); semantically the default.
  (Pre-existing parser-strictness divergence vs Envoy — NOT introduced this phase. Carry-forward note below.)
- Two-stage review: spec ✅ (STRONG cross-proxy assertion present; config identity + oracle mapping verified); code-quality
  APPROVED (0C/0I/3 Minor). **KEY WIN: did NOT repeat the M29-1/M29-2 RING_HASH-vocabulary mistake** — the new driver's
  messages are subset/route-select-worded.
- Minors NOT folded: M-1 (`extract_marker` duplicated with the hash-sweep driver) → folding would touch the hash-sweep
  driver, which the PLAN defers (M29-1/M29-2 carry forward); recorded as a carry-forward. M-2 (no serde round-trip unit
  test for the new driver) → the fixture parse exercises the schema end-to-end; optional. M-3 (plan-boilerplate
  inputs//ignore-list deviations) → benign, documented.

### Phase-30 carry-forwards OUT (weigh whenever `tests/differential/src/lib.rs` is next touched)
- **M29-1/M29-2** (hash-sweep driver's RING_HASH-worded `bail!` messages + comments) — STILL carry forward (fixture 0038
  used a NEW driver, not the hash-sweep).
- **M30-1 (NEW)** the route-select driver's `extract_marker` duplicates the hash-sweep driver's copy (~13 lines); factor a
  shared module-scope `extract_backend_marker` (neutral wording) when the hash-sweep driver is next touched (fold WITH M29-1/M29-2).
- **M30-2 (NEW, parser)** envoy-rust's `Cluster.lb_policy` has no serde default; Envoy defaults it to ROUND_ROBIN. A cluster
  config omitting `lb_policy` boots on Envoy but is REJECTED by envoy-rust (`missing field lb_policy`). Pre-existing
  divergence; consider a `#[serde(default)]` ROUND_ROBIN on `lb_policy` in a future config-hardening phase.

### Task 8 — subset + no-op backstop tests — DONE
- Commit `35a49b5`. 8 tests: subset.rs (3) — build determinism across a query battery; (I-3) missing-selector-key
  EXCLUSION; value-tuple→multiple-indices. cluster.rs (5) — ANY_ENDPOINT fallback round-robins all; DEFAULT_SUBSET
  routes to default (prod); NO_FALLBACK + no-metadata_match → None; (M-2) multi-member subset ROUND_ROBIN rotation
  (never leaks the out-of-subset host); empty-`subset_selectors` round-robins all at pick level. Added `subset_yaml`/
  `build_subset_handle`/`stage_match` test helpers. NO_FALLBACK no-match skipped (already in `subset_narrows_to_matched_endpoint`).
- `cargo test -p envoy-cluster` 160 passed; clippy + fmt clean. No production bug surfaced.
- Two-stage review: spec ✅ (reviewer mutation-tested 5 of the 8 → all non-vacuous, caught real exclusion/rotation/
  fallback behavior); code-quality APPROVED (0C/0I/3 cosmetic convergence Minors on pre-existing code, not folded).

### Task 9 — `parse_bootstrap` subset fuzz seed + BEHAVIOR_CONTRACT subset row — DONE
- Commit `2783e85` (+ a state-3-close clarity edit to the BEHAVIOR_CONTRACT stats bullet, committed with the close-out).
  Created `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_lb_subset.yaml` (ROUND_ROBIN cluster + `lb_subset_config`
  NO_FALLBACK selector `keys:[stage]` + flat `default_subset: {stage:prod}` [ADR-0075] + nested endpoint `metadata` +
  nested route `metadata_match`); registered in `.gitignore` (`!`-exception) + the `fuzz_corpus_seeds_parse_or_reject_cleanly`
  list (`// 30 Task 9`). Extended BEHAVIOR_CONTRACT.md "LB selection" with a `subset LB (NEW, phase 30)` subsection
  (match/fallback semantics, nested-vs-flat wire shapes, config-not-fatal, no-stat, no-op, fixture 0038, deferred non-goals).
- `fuzz_corpus_seeds_parse_or_reject_cleanly` passes; `cargo test -p envoy-config` 468 passed; fmt clean. (cargo-fuzz not
  installed locally → the short-budget fuzz run is the state-4 CI gate.) M29-1/M29-2 NOT folded (carry forward).
- Two-stage review: spec ✅ (seed parses + exercises the surface; contract prose factually accurate vs ADR-0074/0075);
  code-quality APPROVED (0C/0I/3 Minor). The reviewer's "dangling 66" Minor was a misread (66 IS the accurate observed
  opaque Envoy stat value per ADR-0074 #2) — addressed with a clarity rewording at the close-out, preserving the 66.

---

## State-3 close-out summary

**ALL 9 TASKS COMPLETE.** 14 commits on `main` (`9e6eb6e`..`2783e85` + this close-out commit). 0 Critical / 0 Important
review findings across all tasks; all Important findings folded inline (Task-2 default_subset wire shape → ADR-0075;
Task-4 multi-key tuple-order bug → fix `90f82de`; Task-5 shared `endpoint_eligible` → `d405f74`). The fixture-0038
differential ran LOCALLY GREEN. **Next session = state-4 `superpowers:verification-before-completion`** — the §7.5 gate
on Linux CI (full `cargo build/clippy/fmt/test --workspace` + `cargo deny` + the full Docker differential suite [0001–0038]
+ the short-budget `parse_bootstrap` fuzz with the new `cluster_lb_subset.yaml` seed). Carry-forwards for state-4/future:
M29-1/M29-2 + M30-1 (differential-driver `bail!`/`extract_marker` cleanup, fold when the hash-sweep driver is next touched);
M30-2 (envoy-rust `lb_policy` has no serde default — pre-existing parser-strictness divergence).
