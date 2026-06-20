# Phase 30 — `30-lb-subset` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Read with zero prior context (D-3.4): everything you need is in this file, in `SPEC.md`, and in the cited source lines.

**Goal:** Implement Envoy v1.33.0's `subset LB` (`Cluster.lb_subset_config` — metadata-based endpoint-subset selection: endpoint `envoy.lb` metadata + route `metadata_match` narrow the candidate endpoints to the metadata-superset match BEFORE the cluster's `lb_policy` picks within the subset), behaviorally equivalent to upstream Envoy under the §7.2 differential contract.

**Architecture:** subset LB is **NOT a `LbPolicy` value** — it is an ORTHOGONAL pre-dispatch layer. A new `crates/envoy-cluster/src/subset.rs` builds, at cluster construction, a subset index over the endpoints (grouped per `subset_selectors` entry by the tuple of its `keys`' values). At request time the matched route's `metadata_match` is threaded to `pick()` (alongside the existing `Option<u64>` hash key — the phase-28 template), resolved to an eligible-endpoint set (the metadata-superset match, or the `fallback_policy` set when no subset matches), and the EXISTING `hash_lb`/cursor dispatch runs WITHIN that eligible set. A cluster with NO `lb_subset_config` resolves "eligible = all endpoints" → the existing behavior is byte-identical (the no-op regression proof).

**Tech Stack:** Rust (workspace crates `envoy-config`, `envoy-cluster`, `envoy-http1`, `envoy-http2`); `testcontainers` differential harness against `envoyproxy/envoy:v1.33.0`.

---

## §A — The §6.2-LOCKED facts (empirical reconnaissance result) [ADR-0074]

Cracked at this PLAN-write by running live upstream Envoy v1.33.0 in Docker against two distinguishable marker-echoing backends (`backend: prod` / `backend: canary`). Every fact below was OBSERVED (not from docs). **ADR-0074 FIRES** to lock these + record the two divergences from the SPEC projection.

### Match + selection (MATCHES the SPEC projection — STRONG differential target FIRES)
- **Wire shape (accepted verbatim, no field renamed):**
  - Endpoint metadata: `LbEndpoint.metadata.filter_metadata."envoy.lb": { <key>: <value>, ... }` (a map of string→string under the `envoy.lb` namespace; sibling to `endpoint`).
  - Cluster: `lb_subset_config: { fallback_policy: <NO_FALLBACK|ANY_ENDPOINT|DEFAULT_SUBSET>, subset_selectors: [ { keys: [<key>, ...] } ], default_subset: { <key>: <value> } }`.
  - Route: `route.metadata_match.filter_metadata."envoy.lb": { <key>: <value> }`.
- **Superset match:** an endpoint is a candidate iff its `envoy.lb` metadata is a SUPERSET of the route `metadata_match` (endpoint `{stage:prod,version:v2}` matches a route asking only `{stage:prod}`). CONFIRMED.
- **Selector choice:** the `subset_selectors` entry USED is the one whose `keys` SET EQUALS the route `metadata_match`'s key set. A `{stage}` route with ONLY a `keys:[stage,version]` selector present → no usable subset → fallback (503 under NO_FALLBACK). CONFIRMED (removing the `[stage]` selector forced a 503).
- **Lookup:** group the cluster's endpoints by the selector's key-tuple values → subsets; the route's `metadata_match` value-tuple selects one subset; the inner `lb_policy` (ROUND_ROBIN) picks within it. Deterministic across repeats.

### Fallback dispositions (§6.2-VERIFIED)
| Route `metadata_match` | `fallback_policy` | OBSERVED result |
|---|---|---|
| matches a subset | (any) | the matched subset's host(s) |
| matches NO subset (e.g. `{stage:nonexistent}`) | **NO_FALLBACK** | **`HTTP/1.1 503`**, `content-type: text/plain`, `content-length: 19`, `server: envoy`, body `no healthy upstream` (verbatim) |
| matches NO subset | **ANY_ENDPOINT** | round-robin over ALL endpoints |
| matches NO subset | **DEFAULT_SUBSET** (+ `default_subset: {stage:prod}`) | the `default_subset` subset (→ `backend: prod`, deterministic) |
| route has NO `metadata_match` on a subset cluster | NO_FALLBACK / ANY_ENDPOINT / DEFAULT_SUBSET | treated as a fallback request → 503 / round-robin-all / default subset respectively |

### Regression oracle (the `subset.rs` + fixture-0038 ground truth — live-Envoy confirmed)
Endpoints: **A = `{stage:prod, version:v2}`** (→ `backend: prod`), **B = `{stage:canary, version:v1}`** (→ `backend: canary`). Single selector `keys:[stage]`.

| Route `metadata_match` | `fallback_policy` | → backend |
|---|---|---|
| `{stage: prod}` | NO_FALLBACK | **prod** |
| `{stage: canary}` | NO_FALLBACK | **canary** |
| `{stage: prod}` (vs A's superset metadata) | NO_FALLBACK | **prod** (superset matches) |
| `{stage: nonexistent}` | NO_FALLBACK | **503 `no healthy upstream`** |
| `{stage: nonexistent}` | DEFAULT_SUBSET `{stage:prod}` | **prod** |
| (no `metadata_match`) | NO_FALLBACK | **503 `no healthy upstream`** |

### DIVERGENCES from the SPEC projection (the reason ADR-0074 FIRES)
1. **Config validation is NOT startup-fatal (CORRECTS SPEC §2.1.2 / §3.3).** Envoy BOOTS (exit 0, container stays up) for ALL of: empty `subset_selectors: []`; a selector with empty `keys: []`; a `default_subset` whose keys are not covered by any selector; `fallback_policy: DEFAULT_SUBSET` with NO `default_subset` set. Consequences surface only at REQUEST time (a 503 or a fallthrough), NEVER at boot. → **envoy-rust ACCEPTS all four (no fatal validator)**, matching Envoy + ADR-0049 (which mandates fatal ONLY where Envoy rejects — here it does not). Observed edge behaviors: `subset_selectors:[]` → the subset layer is effectively disabled, requests round-robin ALL hosts even under NO_FALLBACK; `DEFAULT_SUBSET` with no `default_subset` → `default_subset` defaults to `{}` (matches all → round-robin all).
2. **Subset LB stats are NON-PORTABLE → emit NONE this phase (CORRECTS SPEC §2.1.8).** Envoy's `cluster.<name>.lb_subsets_active`/`lb_subsets_created` read **66** for a 2-endpoint / single-`[stage]`-selector cluster (reproduced across configs — NOT the naive "1 subset per distinct value" = 2; the value's derivation is opaque). Per the phase-21/24/28/29 "emit a stat only if §6.2 confirms a PORTABLE + DETERMINISTIC value" discipline, **envoy-rust emits NO `lb_subsets_*` stat this phase**; the fixture-0038 `expectations.yaml` IGNORE-LISTS the `cluster.subset_cluster.lb_subsets_*` names (subset stats are a §2.2 deferred non-goal). The request-driven `lb_subsets_selected`/`lb_subsets_fallback` counters ARE deterministic but are deferred WITH the rest for MVP cleanliness.

**Net scope effect:** the validation task collapses to "accept everything" (no fatal validators) and there is NO stat task → the phase is LIGHTER than the SPEC's upper-edge projection. **§6.1 split does NOT fire (ADR-0075 stays reserved + UNFIRED).**

---

## File structure

| File | Responsibility | Task |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` (modify) | `LbMetadata` type (the `envoy.lb` map); `metadata: Option<LbMetadata>` on `LbEndpoint`; `metadata_match: Option<LbMetadata>` on `RouteAction_Route`; `lb_subset_config: Option<LbSubsetConfig>` on `Cluster` (`LbSubsetConfig` + `LbSubsetFallbackPolicy` enum + `LbSubsetSelector { keys }` + `default_subset`) | 1, 2, 3 |
| `crates/envoy-cluster/src/subset.rs` (create) | `SubsetIndex::build` + `resolve(metadata_match) -> Eligible` — the §A algorithm + the pinned oracle test | 4 |
| `crates/envoy-cluster/src/lib.rs` (modify) | `mod subset;` | 4 |
| `crates/envoy-cluster/src/cluster.rs` (modify) | `subset: Option<SubsetIndex>` field; `pick()`/`pick_endpoint()` gain a 2nd arg (the route `metadata_match`); narrow to the eligible set BEFORE the `hash_lb`/cursor dispatch; `from_bootstrap` builds the index | 5 |
| `crates/envoy-http1/src/hcm.rs` + `crates/envoy-http2/src/hcm.rs` (modify) | thread the matched route's `metadata_match` to `pick_endpoint()` (the phase-28 `request_hash_key` template) | 6 |
| `tests/fixtures/0038-lb-subset/` (create) | the subset differential fixture (two metadata'd backends + `metadata_match` routes + a NO_FALLBACK 503 probe) | 7 |
| `tests/differential/...` (modify) | the 0038 route-selection driver (NOT the hash-sweep driver) | 7 |
| backstop tests (in `cluster.rs` / `subset.rs`) | determinism, superset, the 3 fallbacks, the no-`lb_subset_config` no-op, empty-selector edge | 8 |
| `crates/envoy-config/fuzz/` seed + `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (modify) | `parse_bootstrap` subset seed; "LB selection" subset row | 9 |

**§6.1 split decision: NOT split.** ~9 tasks / est. ~900–1100 net LoC — under the ~25-task / ~1500-LoC gate (the validation task collapsed [non-fatal per §A] and there is no stat task). **ADR-0075 does NOT fire** (it remains reserved). **M29-1/M29-2 do NOT fold here** — fixture 0038 uses a NEW route-selection driver, NOT the RING_HASH-worded `Http1HashSweep` driver, so its `bail!` messages are untouched; M29-1/M29-2 carry forward.

---

## Task 1: Config — `LbMetadata` + endpoint `metadata`

**Files:** Modify `crates/envoy-config/src/bootstrap.rs` (`LbEndpoint` :431) + tests (same file `#[cfg(test)] mod tests`).

The `envoy.lb` metadata is a map string→string under `filter_metadata."envoy.lb"`. Model the minimal slice (other `filter_metadata` namespaces parsed-and-ignored).

- [ ] **Step 1: Write the failing test** — a `parse_bootstrap` test that a `LbEndpoint` with `metadata: { filter_metadata: { envoy.lb: { stage: prod, version: v2 } } }` parses into `Some(LbMetadata)` with the `envoy.lb` map `{stage:prod, version:v2}`; and an endpoint with NO metadata → `None`; and an endpoint with a NON-`envoy.lb` namespace → the `envoy.lb` map is absent/empty (ignored, not an error).
- [ ] **Step 2: Run, verify fail** — `cargo test -p envoy-config lb_metadata` → FAIL.
- [ ] **Step 3: Implement:**
```rust
/// 30 D1 (ADR-0073/0074): the `envoy.lb` filter-metadata slice used by subset LB.
/// Mirrors Envoy `core.v3.Metadata.filter_metadata["envoy.lb"]` — a map of
/// string key → string value. Other filter_metadata namespaces are parsed and
/// ignored (they belong to other consumers). Ordered map for deterministic
/// subset-key tuples (BTreeMap).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LbMetadata {
    /// the `envoy.lb` namespace map (empty when absent).
    pub envoy_lb: std::collections::BTreeMap<String, String>,
}
```
Implement a custom `Deserialize` (or a `#[serde(from)]` shim) that reads `filter_metadata` → picks the `"envoy.lb"` entry → its map; ignores other namespaces; non-string scalar values stringify (§6.2: only strings observed — coerce or reject non-strings is a PLAN call: COERCE via `to_string` of the scalar, keep it permissive). Add `metadata: Option<LbMetadata>` to `LbEndpoint`:
```rust
pub struct LbEndpoint {
    pub endpoint: Endpoint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<LbMetadata>,
}
```
(Note: `LbEndpoint` has `#[serde(deny_unknown_fields)]` — adding `metadata` keeps that.)
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** — `phase 30: Task 1 — LbMetadata + endpoint metadata config`.

## Task 2: Config — `Cluster.lb_subset_config`

**Files:** Modify `crates/envoy-config/src/bootstrap.rs` (`Cluster` near `maglev_lb_config` :260; add the structs near the `MaglevLbConfig` block) + tests.

- [ ] **Step 1: Write failing tests** — `lb_subset_config` with `subset_selectors: [{keys:[stage]}]` + `fallback_policy: NO_FALLBACK` (default) / `ANY_ENDPOINT` / `DEFAULT_SUBSET` + `default_subset: {stage:prod}` round-trips; absent → `None`; **per §A divergence #1, an empty `subset_selectors: []` and a selector with `keys: []` PARSE OK (NOT rejected)** — assert `validate_cluster` returns `Ok`.
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement:**
```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LbSubsetFallbackPolicy {
    #[default]
    NoFallback,
    AnyEndpoint,
    DefaultSubset,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LbSubsetSelector {
    #[serde(default)]
    pub keys: Vec<String>,
}

/// 30 D1 (ADR-0073/0074): MAGLEV-style optional subset-LB tuning. NO validator is
/// fatal (§6.2/ADR-0074: Envoy boots for empty selectors / empty keys / uncovered
/// default_subset / DEFAULT_SUBSET-without-default — consequences are request-time).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct LbSubsetConfig {
    #[serde(default)]
    pub fallback_policy: LbSubsetFallbackPolicy,
    #[serde(default)]
    pub subset_selectors: Vec<LbSubsetSelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_subset: Option<LbMetadata>,
}
```
Add to `Cluster`:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub lb_subset_config: Option<LbSubsetConfig>,
```
**No `validate_cluster` block** (per §A divergence #1 — nothing is fatal). Add `lb_subset_config: None` to any `Cluster` test literals that need it (grep `Cluster {`).
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** — `phase 30: Task 2 — Cluster.lb_subset_config (accept-all, no fatal validator) [ADR-0074]`.

## Task 3: Config — route `metadata_match`

**Files:** Modify `crates/envoy-config/src/bootstrap.rs` (`RouteAction_Route` :1382) + tests.

- [ ] **Step 1: Write failing test** — a route with `metadata_match: { filter_metadata: { envoy.lb: { stage: prod } } }` parses into `Some(LbMetadata)`; absent → `None`.
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** — add to `RouteAction_Route` (next to `hash_policy` :1393):
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub metadata_match: Option<LbMetadata>,
```
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** — `phase 30: Task 3 — route metadata_match config`.

## Task 4: `subset.rs` — the subset index build + resolve (load-bearing)

**Files:** Create `crates/envoy-cluster/src/subset.rs`; modify `crates/envoy-cluster/src/lib.rs` (`mod subset;`); test the §A pinned oracle.

This is the correctness heart. It MUST reproduce the §A regression oracle.

- [ ] **Step 1: Write the failing pinned-oracle test FIRST** — build over endpoints A `{stage:prod,version:v2}` / B `{stage:canary,version:v1}` with selector `keys:[stage]` and assert: `resolve({stage:prod})` → eligible = `[A]`; `resolve({stage:canary})` → `[B]`; `resolve({stage:prod})` against A's superset → `[A]`; `resolve({stage:nonexistent})` under NO_FALLBACK → empty; under DEFAULT_SUBSET `{stage:prod}` → `[A]`; under ANY_ENDPOINT → `[A,B]`; `resolve(None)` (no metadata_match) → fallback per policy. Add the empty-`subset_selectors` edge (resolve → all hosts).
- [ ] **Step 2: Run, verify fail** — module doesn't exist.
> **Build rule (the superset encoding — be explicit):** for each endpoint, for each selector, the endpoint is placed under the value-tuple of that selector's `keys` (in `keys` order) **iff it has a value for EVERY key in the selector**; an endpoint MISSING any selector key is EXCLUDED from that selector's subsets (Envoy parity). Because the bucket key is the selector's keys ALONE, an endpoint with EXTRA `envoy.lb` keys still lands in the right bucket → superset matching falls out naturally. (T8 adds a backstop with an endpoint missing a selector key, pinning the exclusion.)

- [ ] **Step 3: Implement** (the §A algorithm):
```rust
//! 30 (ADR-0073/0074): metadata-based subset LB. §6.2-LOCKED (live Envoy v1.33.0):
//! group endpoints per selector by the tuple of its `keys`' values; a route
//! `metadata_match` selects the selector whose `keys` SET EQUALS the match's keys,
//! then the value-tuple → the subset; superset match (endpoint metadata ⊇ match);
//! fallback per `LbSubsetFallbackPolicy`. NO config is fatal (§A divergence #1).
use std::collections::{BTreeMap, BTreeSet};
use envoy_config::{LbMetadata, LbSubsetConfig, LbSubsetFallbackPolicy};

#[derive(Debug)]
pub(crate) struct SubsetIndex {
    fallback: LbSubsetFallbackPolicy,
    default_subset: Option<BTreeMap<String, String>>,
    // one map per selector: key-set -> (value-tuple -> endpoint indices)
    selectors: Vec<SelectorIndex>,
    n: usize, // endpoint count (for ANY_ENDPOINT / disabled-layer = all)
}

#[derive(Debug)]
struct SelectorIndex {
    keys: BTreeSet<String>,
    subsets: BTreeMap<Vec<String>, Vec<usize>>, // value-tuple (in `keys` order) -> indices
}

/// The resolved eligible set for one request.
pub(crate) enum Eligible {
    All,             // no lb_subset_config, or ANY_ENDPOINT/empty-selectors fallthrough
    Some(Vec<usize>),
    None,            // NO_FALLBACK with no matching subset -> 503
}

impl SubsetIndex {
    /// Build over `endpoint_metadata[i]` = the `envoy.lb` map of endpoint i (empty if absent).
    pub(crate) fn build(cfg: &LbSubsetConfig, endpoint_metadata: &[BTreeMap<String, String>]) -> SubsetIndex { /* group per selector */ }

    /// Resolve the eligible endpoint set for a route `metadata_match` (None = no match config).
    pub(crate) fn resolve(&self, metadata_match: Option<&BTreeMap<String, String>>) -> Eligible {
        // 1. if metadata_match present: find the selector whose key-set == match keys;
        //    look up the value-tuple -> Some(indices) (superset already encoded at build:
        //    an endpoint is in the subset iff its metadata is a superset of the tuple).
        // 2. no selector / no match / no metadata_match -> fallback:
        //    NO_FALLBACK -> Eligible::None; ANY_ENDPOINT -> Eligible::All;
        //    DEFAULT_SUBSET -> resolve(default_subset) (empty default = All).
        // 3. empty subset_selectors -> the layer is disabled -> Eligible::All (§A edge).
    }
}
```
> **Superset semantics:** an endpoint belongs to a selector's subset for value-tuple T iff, for each key in the selector, the endpoint's `envoy.lb` value equals T's value. Because the subset is keyed ONLY on the selector's keys, an endpoint with EXTRA metadata keys still matches (superset) — extra keys are simply not part of the tuple. So grouping by the selector-key-tuple naturally yields superset matching. Verify against the §A oracle.

Add `mod subset;` to `lib.rs` (mirror the `maglev.rs` `#![allow(dead_code)]` precedent until Task 5 consumes it).
- [ ] **Step 4: Run, verify pass** — oracle PASS.
- [ ] **Step 5: Commit** — `phase 30: Task 4 — subset.rs index build + resolve (§6.2-LOCKED oracle) [ADR-0074]`.

## Task 5: `pick()` integration — narrow to the eligible set

**Files:** Modify `crates/envoy-cluster/src/cluster.rs` (`subset` field near `hash_lb` :213; `pick()` :346 + `pick_endpoint()` :646 signatures; `from_bootstrap` :990/:1387 build; struct literals).

- [ ] **Step 1:** Add the field + build. Next to `hash_lb`:
```rust
pub(crate) subset: Option<crate::subset::SubsetIndex>,
```
In `from_bootstrap`, build it when `cfg.lb_subset_config.is_some()`; `None` otherwise. **ALIGNMENT HAZARD (I-2):** `endpoints: Vec<SocketAddr>` is NOT 1:1 with config `lb_endpoints` — for `StrictDns` one `LbEndpoint` fans out to MULTIPLE `SocketAddr`s. Collect the per-endpoint `envoy.lb` map INSIDE THE SAME nested `for locality { for lbe { … } }` loop that builds `endpoints` (`cluster.rs:~1010-1055`), pushing `lbe.metadata`'s `envoy.lb` map ONCE PER RESOLVED `SocketAddr` (once per DNS result for StrictDns; once per endpoint for Static/Eds), so `endpoint_metadata[i]` stays index-aligned with `endpoints[i]`. The subset index stores `Vec<usize>` indices into `endpoints` — misalignment → wrong host. Add `subset: None` / `subset: Some(..)` to ALL struct literals — grep `hash_lb:` / `Cluster {` (6+ sites: the `from_bootstrap` literal + the 3 `mk_*` test builders + ≥3 test literals; the line numbers drift, so grep, don't trust a fixed list).
- [ ] **Step 2:** Extend `pick`/`pick_endpoint` with a 2nd arg — the route `metadata_match` map:
```rust
pub fn pick_endpoint(&self, key_hash: Option<u64>, subset_match: Option<&BTreeMap<String,String>>) -> Option<SocketAddr> {
    self.inner.pick(key_hash, subset_match)
}
```
In `pick()` (before the :378 `hash_lb` dispatch): resolve the eligible set:
```rust
// 30: subset narrowing BEFORE the hash_lb/cursor dispatch.
let eligible: Option<Vec<usize>> = match self.subset.as_ref() {
    None => None,                              // no lb_subset_config -> all (no-op)
    Some(ix) => match ix.resolve(subset_match) {
        Eligible::All => None,                 // treat as "all endpoints"
        Eligible::Some(idxs) => Some(idxs),
        Eligible::None => return None,         // NO_FALLBACK no-match -> 503
    },
};
```
Then run the existing dispatch over the eligible set: when `eligible` is `Some(idxs)`, the cursor/round-robin advances within `idxs` (and the `hash_lb` lookup result is accepted only if it lands in `idxs`); when `None`, the existing all-endpoints path runs UNCHANGED (byte-identical — the no-op proof). Keep the `hi < total` guard. **MVP inner LB within a subset = the cursor (ROUND_ROBIN); a single-host subset → deterministic.**

> **SLOW-PATH COMPOSITION (I-1):** `pick()` has a fast round-robin path AND a slow HC/OD-eligibility path (`cluster.rs:~436-444`, which builds its own `eligible_idx` from `is_eligible(i)` over `0..total`). The subset `idxs` must compose with BOTH: restrict the fast path to `idxs`, AND for the slow path intersect — `eligible_idx.retain(|i| idxs.contains(i))` — so a subset cluster that ALSO has HC/OD never returns an out-of-subset host. Since subset+HC/OD is a §2.2-deferred DIFFERENTIAL non-goal and the MVP fixture is a PLAIN cluster (no HC/OD → only the fast path runs), the minimum correct implementation may instead `debug_assert!(self.endpoint_health.is_none() && self.outlier_detection.is_none())` on the subset branch and document that subset+HC/OD composition is out of scope — BUT pick one and WRITE IT DOWN; do not leave the intersection to the implementer's guess.
- [ ] **Step 3:** Update the two pre-existing internal `pick(...)` call site(s) + the doc-comments (the dispatch now narrows first).
- [ ] **Step 4: Run** — `cargo test -p envoy-cluster` → ALL existing tests PASS (round-robin + RING_HASH + MAGLEV byte-identical with `subset: None` and `subset_match: None` — the no-op proof). `cargo clippy -p envoy-cluster --all-targets -- -D warnings` clean.
- [ ] **Step 5: Commit** — `phase 30: Task 5 — pick() subset narrowing (eligible-set, no-op when absent)`.

## Task 6: HCM threading — route `metadata_match` → `pick_endpoint`

**Files:** Modify `crates/envoy-http1/src/hcm.rs` (:419 call site; the route-match that yields the cluster) + `crates/envoy-http2/src/hcm.rs` (:185). Mirror the phase-28 `request_hash_key` threading.

- [ ] **Step 1:** At the route-match step (where the matched `RouteAction_Route` is resolved — the same place `hash_policy` is read), surface `route.metadata_match.as_ref().map(|m| &m.envoy_lb)` and thread it as a new `subset_match: Option<&BTreeMap<String,String>>` parameter alongside `request_hash_key`, to `cluster.pick_endpoint(request_hash_key, subset_match)`. The metadata_match is STATIC route config (no request data) — simpler than the hash key. Do this for BOTH H1 (:419) and H2 (:185) call sites.
> **(M-2) H1/H2 pick-none asymmetry:** H1 (`hcm.rs:~419`) returns the 503 `no healthy upstream` on `pick()→None`; H2 (`hcm.rs:~185`) returns the pre-existing `synth_h2_502()`. Fixture 0038 is **H1-only**, so the NO_FALLBACK→503 probe (Task 7) asserts on the H1 path only; leave the H2 502-on-pick-none UNCHANGED (not asserted). A future H2 subset probe must assert 502, not 503.

- [ ] **Step 2: Run** — `cargo test -p envoy-http1 -p envoy-http2` → green; `cargo build -p envoy-bin` ok.
- [ ] **Step 3: Commit** — `phase 30: Task 6 — HCM route metadata_match threading`.

## Task 7: Differential fixture `0038-lb-subset` (STRONG)

**Files:** Create `tests/fixtures/0038-lb-subset/{envoy.yaml, envoy-rust.yaml, README.md, inputs/, expectations.yaml}` (clone `0037-lb-maglev`'s harness shape — the `{{BACKEND_IP}}`/`discover_host_lan_ip` mechanism + two `--body-marker` backends — but DROP `lb_policy: MAGLEV`/`maglev_lb_config`/the `x-hash-key` hash_policy; ADD endpoint `metadata` + `lb_subset_config` + route `metadata_match`). Use the §A minimal YAML as the template. Modify the differential harness with a NEW route-selection driver (NOT `Http1HashSweep`).

- [ ] **Step 1:** Create the fixture. Endpoints A/B with `envoy.lb` `{stage:prod,version:v2}` / `{stage:canary,version:v1}`; selector `keys:[stage]`; `fallback_policy: NO_FALLBACK`. Routes: `/prod`→`metadata_match {stage:prod}`, `/canary`→`{stage:canary}`, `/nope`→`{stage:nonexistent}` (the 503 fallback probe). README notes the §A algorithm + the STRONG target + the ip:port-independence (subset selects on METADATA, not ip — but keep `{{BACKEND_IP}}` shared-IP so both proxies build identical endpoints; the body marker is the discriminator).
- [ ] **Step 2:** Add a differential driver (clone the 0037 driver's harness scaffolding, NEW selection logic): GET `/prod` → assert body marker == Envoy's (prod); `/canary` → canary; `/nope` → assert **both proxies return 503 with body `no healthy upstream`** (the NO_FALLBACK probe — a single-deterministic-outcome disposition per SPEC §2.1.6, NOT ANY_ENDPOINT). `expectations.yaml`: IGNORE-LIST `cluster.subset_cluster.lb_subsets_*` (§A divergence #2 — subset stats not emitted by envoy-rust).
- [ ] **Step 3: Run** the differential LOCALLY (this Docker-Desktop host — subset LB has no reload trigger). → GREEN; all `0001`–`0037` still green.
- [ ] **Step 4: Commit** — `phase 30: Task 7 — fixture 0038-lb-subset differential (STRONG, metadata-match selection + NO_FALLBACK 503 probe)`.

## Task 8: In-process backstop tests

**Files:** Test in `crates/envoy-cluster/src/subset.rs` + `crates/envoy-cluster/src/cluster.rs` `mod tests`.

Cover what the differential cannot exercise deterministically:
- [ ] subset build determinism (build twice → identical); superset match; the value-tuple → subset mapping.
- [ ] the 3 fallback dispositions via `pick`: NO_FALLBACK no-match → `None` (503); ANY_ENDPOINT no-match → a host (round-robin all); DEFAULT_SUBSET no-match → the default subset's host; the no-`metadata_match` request per policy.
- [ ] the **no-`lb_subset_config` no-op regression witness**: a cluster with `subset: None` + `subset_match: Some(..)` picks EXACTLY as before (round-robin / RING_HASH / MAGLEV unchanged).
- [ ] the empty-`subset_selectors` edge (§A): layer disabled → round-robin all even under NO_FALLBACK.
- [ ] single-host subset (deterministic) + the superset endpoint case.
- [ ] **(I-3) missing-key exclusion:** an endpoint MISSING a selector key (e.g. `{}` or `{version:v1}` only) is EXCLUDED from the `[stage]` selector's subsets → a `{stage:prod}` route never selects it.
- [ ] **Commit** — `phase 30: Task 8 — subset + no-op backstop tests`.

## Task 9: Fuzz seed + BEHAVIOR_CONTRACT

**Files:** Create a `parse_bootstrap` fuzz seed under `crates/envoy-config/fuzz/corpus/parse_bootstrap/` (a MAGLEV-seed-style minimal bootstrap with `lb_subset_config` + endpoint `metadata` + route `metadata_match`; register it in the corpus parse test + `.gitignore`; NO new fuzz target). Modify `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — extend the "LB selection" section with a subset row (the §A algorithm summary + the superset/selector semantics + the 3 fallback dispositions + the validation-not-fatal + stats-deferred notes).

- [ ] **Step 1:** Add the fuzz seed; validate it parses via the corpus test (cargo-fuzz not installed locally → the authoritative short-budget run is the state-4 CI gate).
- [ ] **Step 2:** Extend BEHAVIOR_CONTRACT.md (subset row). Note M29-1/M29-2 are NOT folded (fixture 0038 uses a non-hash-sweep driver) → they remain phase-31 carry-forwards.
- [ ] **Step 3: Commit** — `phase 30: Task 9 — parse_bootstrap subset fuzz seed + BEHAVIOR_CONTRACT subset row`.

---

## §7.5 phase-done gate (verified at state-4)

(a) fixture `0038` green + (b) all `0001`–`0037` green + (c) h2spec ≥95% (unchanged) + (d) the `parse_bootstrap` subset fuzz seed clean + (e) `cargo build --workspace --all-targets` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds (D-3.8).

_Scope locked by ADR-0073; the §6.2 reconciliation locked by ADR-0074 (validation-not-fatal + stats-deferred corrections). ADR-0075 (split) did NOT fire. Next state-3 skill: `superpowers:subagent-driven-development`._
