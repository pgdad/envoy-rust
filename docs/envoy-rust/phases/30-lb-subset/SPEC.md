# Phase 30 — `30-lb-subset` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0073** (the phase-30 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

Continue the **Load-balancing family** (ROADMAP §9 heading
`least_request, random, ring_hash, maglev, subset LB, locality-weighted LB, priority load
balancing, panic thresholds`) with **`subset LB`** — Envoy's metadata-based endpoint-subset
load balancer (`Cluster.lb_subset_config`). Unlike phases 28 (`RING_HASH`) and 29 (`MAGLEV`),
subset LB is **NOT a `LbPolicy` value**: it is an orthogonal layer that, BEFORE the cluster's
existing `lb_policy` picks a host, NARROWS the candidate endpoint set to those whose `envoy.lb`
metadata matches the matched route's `metadata_match`. A cluster carries
`lb_subset_config { subset_selectors, fallback_policy, default_subset }`; each endpoint carries
`envoy.lb` metadata (e.g. `{stage: prod}`); a route carries `metadata_match` (e.g.
`{stage: prod}`); the LB selects only endpoints whose metadata is a SUPERSET of the route's
`metadata_match`, then runs the cluster's `lb_policy` (MVP: `ROUND_ROBIN`) within that subset. The
**differential is byte-exact and DETERMINISTIC** when each metadata value maps to a single
distinguishable backend (one backend per `stage` value in the fixture — NOT Envoy's deferred
`single_host_per_subset` config flag, §2.2) — the SAME `metadata_match` always selects the
SAME backend, byte-identical to upstream Envoy v1.33.0, observable as a **normal request/response
on this Docker-Desktop dev host** (no file-watch/reload trigger, like phases 28/29 and unlike
26/27). The phase consumes the open **M29-1/M29-2** carry-forwards (the shared `Http1HashSweep`
differential driver's RING_HASH-worded diagnostics) if it touches that driver. See ADR-0073 for
the pick rationale and the rejected alternatives (least_request/random — non-deterministic;
priority/panic/locality-weighted — need active-HC health state).

## §1 — Goal & differential surface

**Goal.** Implement Envoy's `LbSubsetConfig` metadata-based subset load balancing (endpoint
`envoy.lb` metadata → route `metadata_match` → narrowed candidate set → inner `lb_policy`),
behaviorally equivalent to upstream Envoy v1.33.0 under the differential contract (§7.2 of
`BOOTSTRAP_PROMPT.md`).

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0038-lb-subset`** (next free number; baseline is `0001`…`0037`): one cluster with
  `lb_subset_config` (a single `subset_selectors` entry keyed on `["stage"]`) and **two
  distinguishable real backends** (the phase-27 harness seed — `http1-echo-server` instances with
  per-backend `--body-marker`), each carrying distinct `envoy.lb` metadata (`stage: prod` vs
  `stage: canary`). The H1 listener's routes carry `metadata_match` (`{ stage: prod }` on one
  route, `{ stage: canary }` on another). A direct structural sibling of fixture-0037, including
  the **`{{BACKEND_IP}}` / `discover_host_lan_ip` shared-address mechanism** (so both proxies build
  identical endpoint identities). The driver sends requests at each route and asserts, per route,
  that envoy-rust selects the **same backend as Envoy** (proven by the response body marker) —
  cross-proxy identical selection (the **STRONG** differential target). It also asserts a
  **fallback probe** (a route whose `metadata_match` matches NO endpoint → the configured
  `fallback_policy` disposition, §6.2-verified — e.g. NO_FALLBACK → 503 no-healthy-upstream, or
  DEFAULT_SUBSET → the default subset's backend).
- **All 37 pre-existing fixtures `0001`–`0037` stay green simultaneously** — `lb_subset_config`
  is opt-in per cluster; every existing cluster has no subset config, so the subset layer must be
  a no-op for them (regression-equivalence — the load-bearing proof that the `pick()` subset
  narrowing is inert when `lb_subset_config` is absent, for BOTH the round-robin path AND the
  existing RING_HASH/MAGLEV hash paths).

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance
suite. A new `parse_bootstrap` fuzz seed exercises the new config surface (`lb_subset_config` +
endpoint `metadata` + route `metadata_match`); NO new fuzz target (the subset engine is covered by
unit tests + the differential).

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an
explicit deferred non-goal with its own future home. Exact dispositions marked **§6.2-VERIFY** are
empirically locked at the state-2 PLAN-write (the phase-27/28/29 verify-at-PLAN-write discipline);
this SPEC states the projected shape.

### §2.1 IN scope

1. **Config — endpoint metadata.** Add `metadata: Option<Metadata>` to the endpoint (`LbEndpoint`)
   config struct (`crates/envoy-config/src/bootstrap.rs`; today an endpoint is address-only). The
   `Metadata` type models Envoy's `core.v3.Metadata` `filter_metadata` map narrowed to the
   **`envoy.lb`** namespace = a map of string-key → scalar value (string; **§6.2-VERIFY** whether
   non-string scalars/`number`/`bool` appear and how they stringify). Other filter-metadata
   namespaces are parsed-and-ignored (Envoy parity — they belong to other consumers).
2. **Config — cluster subset config.** Add `lb_subset_config: Option<LbSubsetConfig>` to `Cluster`
   with: `fallback_policy` (enum `NO_FALLBACK` [default] / `ANY_ENDPOINT` / `DEFAULT_SUBSET`),
   `default_subset` (a `Metadata` map, consulted only for `DEFAULT_SUBSET`), and
   `subset_selectors: Vec<LbSubsetSelector>` where each selector is `{ keys: Vec<String> }` (the
   set of metadata keys that define one family of subsets). MVP locks **per-selector
   `single_host_per_subset = false`** and a **single global `fallback_policy`** (per-selector
   fallback is §2.2-deferred). Validation dispositions (empty `keys`, an empty `subset_selectors`,
   a `default_subset` referencing keys absent from any selector) are **§6.2-VERIFY** (mirroring the
   phase-28/29 all-fatal-vs-accept posture per ADR-0049).
3. **Config — route `metadata_match`.** Add `metadata_match: Option<Metadata>` to the
   `RouteAction_Route` (`crates/envoy-config/src/bootstrap.rs`; today `{ cluster, retry_policy,
   hash_policy }`). It is the `envoy.lb` metadata the matched route requires of a candidate
   endpoint.
4. **The subset engine.** A new `crates/envoy-cluster/src/subset.rs` (sibling to `ring_hash.rs` /
   `maglev.rs`) building, at cluster construction, the subset index: for each `subset_selectors`
   entry, group the cluster's endpoints by the tuple of that selector's `keys` → values, into a
   map `(key-tuple-values) → Vec<endpoint-index>`. At request time, given the route's
   `metadata_match`, find the selector whose `keys` set equals the `metadata_match` key set, look
   up the value-tuple → the matching endpoint subset, and return that subset (an index list) for
   the inner `lb_policy` to pick within. **§6.2-VERIFY** the exact match semantics (Envoy: an
   endpoint matches iff its `envoy.lb` metadata is a SUPERSET of the route `metadata_match`; the
   selector chosen is the one whose key set exactly equals the metadata_match's keys).
5. **Inner LB + integration.** The narrowed subset feeds the cluster's existing `lb_policy`
   dispatch (`pick()` — MVP `ROUND_ROBIN` cursor path within the subset; the hash paths
   ring/maglev within a subset are §2.2-deferred). The route's `metadata_match` is threaded to
   `pick_endpoint()`/`pick()` analogously to phase-28's `Option<u64>` hash key — a new
   `Option<&SubsetKey>` (or resolved subset-index list) argument; a cluster with NO
   `lb_subset_config` ignores it (the no-op regression proof). The H1/H2 HCM passes the matched
   route's `metadata_match` to `pick()`.
6. **Fallback.** When no subset matches the route's `metadata_match` (or the route has no
   `metadata_match` on a subset cluster), apply `fallback_policy`: `NO_FALLBACK` → no host selected
   → the existing no-healthy-upstream path (**§6.2-VERIFY** the exact status — Envoy's 503
   `no_healthy_upstream`); `ANY_ENDPOINT` → the full endpoint set (the normal `lb_policy` over all
   hosts); `DEFAULT_SUBSET` → the subset matching `default_subset`'s metadata. **§6.2-VERIFY** the
   precise disposition for each, and the no-`metadata_match`-on-a-subset-cluster case. **The
   fixture-0038 fallback PROBE uses a single-deterministic-outcome disposition (NO_FALLBACK → the
   503 `no_healthy_upstream`, or DEFAULT_SUBSET → a single-host default subset); `ANY_ENDPOINT`
   over MULTIPLE hosts runs the ROUND_ROBIN cursor and is NOT asserted byte-exact cross-proxy** (the
   cursor phase need not align across the two proxies — the phase-28/29 "round-robin path inert"
   discipline).
7. **Health / outlier composition.** As in phases 28/29: the MVP fixture uses a PLAIN cluster (no
   HC/OD), so subset-LB + HC/OD compose as a *differential* is a deferred non-goal (§2.2); the
   selected subset host is returned directly.
8. **Stats.** Any per-cluster subset LB stat Envoy emits (e.g.
   `cluster.<name>.lb_subsets_{active,fallback,selected}`; the `created`/`removed` gauges are
   EDS-churn counters that stay constant under the static fixture) — emitted only if §6.2
   confirms a portable namespace + deterministic values; otherwise none (the phase-21/24/28/29
   discipline). **§6.2-VERIFY**.
9. **Tests.** Fixture `0038` (the differential above) + an in-process backstop (subset build
   determinism; metadata_match → subset selection; superset-match semantics; the three
   fallback_policy paths; the no-`lb_subset_config` no-op regression witness; single-host and
   multi-host subset) + a `parse_bootstrap` fuzz seed + a BEHAVIOR_CONTRACT "LB selection"
   extension (a subset row). **Fold M29-1/M29-2** (the shared `Http1HashSweep` driver's
   RING_HASH-worded `bail!` messages/comments → policy-agnostic wording) IF the new fixture-0038
   driver reuses/extends that shared driver; else carry forward.

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **`least_request` / `random`** (non-deterministic → not byte-exact differentiable; need a
  contract-relaxation ADR — a separate future phase).
- **`priority load balancing` / `panic thresholds` / `locality-weighted LB`** — each needs active
  health-checking / outlier-detection health state (deferred) or weighted-random distribution to
  be EXERCISED; future phases after the upstream-robustness family lands HC.
- **Subset + consistent-hash inner LB** (`RING_HASH`/`MAGLEV` within a subset) — MVP inner LB is
  `ROUND_ROBIN`; the hash-within-subset composition defers (the `pick()` dispatch is built to
  compose, but it is not differentially exercised this phase).
- **Multiple overlapping `subset_selectors` with selection precedence**, per-selector
  `fallback_policy` / `fallback_keys_subset` (the SUBSET fallback), `single_host_per_subset`,
  `list_as_any`, `metadata_fallback_policy`, `scale_locality_weight`, `allow_redundant_keys` — MVP
  is a single selector + a single global `fallback_policy`.
- **Locality-aware / locality-weighted subset** — needs locality config (deferred with
  locality-weighted LB).
- **Non-`envoy.lb` metadata consumers** (other `filter_metadata` namespaces); weighted endpoints
  WITHIN a subset.
- **Subset + EDS-hot-reload re-subset** (re-building subsets when the endpoint set hot-swaps) — the
  fixture uses a static cluster; deferred (a future EDS/LB cross-phase).
- **Subset + active-HC / outlier-detection composition** as a *differential* — exercised only by
  the backstop unless §6.2 shows it cheap (a PLAN-write call), mirroring phases 28/29.
- **CDS/LDS hot-reload** (the deferred xDS layers); the **gRPC/ADS transport** (still
  ADR-0014/H2-trailers-blocked).

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The `envoy.lb` metadata wire shape + match semantics** — the exact YAML shape of
   `LbEndpoint.metadata.filter_metadata["envoy.lb"]` + `RouteAction.metadata_match`; the
   superset-match rule (endpoint metadata ⊇ route `metadata_match`); how the selector whose `keys`
   set matches the `metadata_match` keys is chosen; non-string scalar handling.
2. **The fallback_policy dispositions** — NO_FALLBACK exact status/body (503 `no_healthy_upstream`?);
   ANY_ENDPOINT and DEFAULT_SUBSET selection; the no-`metadata_match`-on-a-subset-cluster case.
3. **The config validity dispositions** — empty `keys` / empty `subset_selectors` / a
   `default_subset` not covered by a selector / a `metadata_match` whose key set matches no
   selector → fatal vs accept-and-fallback (the ADR-0049 posture).
4. **The `metadata_match` threading shape** into `pick()` (a borrowed `&SubsetKey` vs a resolved
   subset index-list; allocation-free on the no-subset fast path; the RING_HASH/MAGLEV/round-robin
   paths stay byte-identical).
5. **Subset LB stat namespace** (if any) — §6.2-verified.
6. **Subset + HC/OD** precise semantics, and whether it is in differential scope or backstop-only.
7. **The §6.1 split decision** — see §6.1.

## §4 — Reuse map (what exists; do not rebuild)

- **The `LbPolicy` enum + `pick()`/`pick_endpoint()` dispatch** (`crates/envoy-cluster/src/cluster.rs`)
  — phase-29 left a `hash_lb: Option<HashLb>` dispatch + the `ROUND_ROBIN` cursor path; the subset
  layer narrows the endpoint set BEFORE this dispatch runs. Extend, do not rebuild.
- **The route config + the H1/H2 HCM request paths** — phase-28's `hash_policy` threading
  (`Option<u64>` through `pick()`; the H1 `hcm.rs` / H2 `hcm.rs` per-request call sites) is the
  structural template for threading the route `metadata_match` to `pick()`. A subset cluster keys
  off the matched route's static `metadata_match`.
- **The two-distinguishable-backend fixture harness** (`tests/fixtures/0037-lb-maglev/` +
  `0036-lb-ring-hash/` + the `tests/differential/src/lib.rs` driver + `discover_host_lan_ip` /
  `{{BACKEND_IP}}`) — the template for fixture 0038; the per-backend `--body-marker` backends are
  already built.
- **The `*LbConfig` config pattern** (`RingHashLbConfig` / `MaglevLbConfig` serde-default +
  validator pattern in `bootstrap.rs`) — extend with `LbSubsetConfig` following the same shape.
- **The `ring_hash.rs` / `maglev.rs` build-once-at-construction structural sibling** — `subset.rs`
  builds its subset index once over the endpoint snapshot, returning endpoint indices aligned with
  the `endpoints` Vec.
- **The no-host / no-healthy-upstream 503 path** (the existing `pick()` returns `None` → the HCM
  emits Envoy's local 503) — reused for `NO_FALLBACK`.

## §5 — Behavioral contract notes

- **Determinism:** same route `metadata_match` → same backend, on each proxy, across requests
  (consistency); and (strong target) the same backend on BOTH proxies (cross-proxy identity) when
  each metadata value maps to a single distinguishable backend.
- **Superset match (projected; §6.2-VERIFY — stated as one projection, not settled fact):** an
  endpoint is a candidate iff its `envoy.lb` metadata is a SUPERSET of the route `metadata_match`
  AND a `subset_selectors` entry's `keys` set equals the `metadata_match` key set (Envoy's real
  rule interacts with the §2.2-deferred `allow_redundant_keys` — confirmed at the §6.2 recon).
- **Fallback:** a route `metadata_match` matching no subset → the cluster `fallback_policy`
  (§6.2-VERIFY each disposition).
- **Regression-equivalence:** every cluster with NO `lb_subset_config` behaves exactly as before
  (the subset-narrowing no-op proof — all 37 existing fixtures green; the round-robin AND
  RING_HASH/MAGLEV paths unchanged).
- **Config validity:** invalid subset config is a startup-fatal parse error where §6.2 shows Envoy
  rejects (ADR-0049 all-fatal; no reload path this phase).
- **Differential locality:** the subset selection is observable WITHOUT a file-watch/reload trigger
  → the fixture-0038 differential runs and is authoritative on this Docker-Desktop host (NOT
  Linux-CI-only, unlike phases 26/27).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split is projected **NOT to fire**, but this is the **closest call since phase 22** — subset LB
adds MORE genuinely-new config surface than maglev (endpoint metadata + `lb_subset_config` + route
`metadata_match` + the subset engine + 3 fallback policies), though it reuses the
two-distinguishable-backend harness, the `*LbConfig` pattern, and the `pick()` threading template.
Estimate ~1100–1500 LoC / ~12–15 tasks — at the upper edge of the ~1500-LoC / ~25-task gate. The
decision is confirmed at the state-2 PLAN-write. **ADR-0075 is reserved** for the split (fires only
if it happens); the natural seam is **30.1** (endpoint `metadata` + `LbSubsetConfig`/route
`metadata_match` config + validators + the `subset.rs` engine + backstop) / **30.2** (the `pick()`
threading + fallback + fixture 0038 + stats + fuzz + BEHAVIOR_CONTRACT + close).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Like phases 28/29 (and unlike phases 26/27), this phase's behavior is **locally observable** (no
reload trigger). At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0` with one cluster
carrying `lb_subset_config` + two metadata'd backends + routes with `metadata_match`, and:
1. RECORD which backend each `metadata_match` route selects (the ground truth the differential
   asserts against).
2. Verify the `envoy.lb` metadata + `metadata_match` wire shape; the superset-match + selector-key
   semantics; the three `fallback_policy` dispositions (NO_FALLBACK status/body, ANY_ENDPOINT,
   DEFAULT_SUBSET); the no-`metadata_match` case; any subset LB stat namespace + values; the
   invalid-config dispositions (empty keys/selectors, uncovered default_subset).
3. Decide STRONG (cross-proxy identical selection) — expected, since subset selection is
   deterministic; record a fallback equivalence only if some disposition proves non-deterministic.
**ADR-0074 FIRES** at the PLAN-write if any of these materially diverge from this SPEC's projection
(notably the fallback dispositions or the match semantics). `PLAN.md` lands with the
empirically-locked facts inline (no `[§6.2-PENDING]` projections — the phase-27/28/29
verify-at-PLAN-write discipline).

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2
named non-goal with a future home. The subset engine, the `pick()` integration, the fallback, and
the fixture are real and differentially exercised (or backstop-exercised where §2.2 says so) — no
stubs.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0038` green + (b) all of `0001`–`0037` green + (c) h2spec ≥95% + (d) the
`parse_bootstrap` fuzz seed clean + (e) `cargo build --workspace --all-targets` / `cargo clippy
--workspace --all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo
test --workspace` / `cargo deny check` all clean + (f) `REVIEW.md` approved.
`#![forbid(unsafe_code)]` holds (D-3.8).

---

_Scope locked by **ADR-0073**. ADR-0074 reserved (§6.2 reconciliation), ADR-0075 reserved (§6.1
split). The state-2 PLAN-write is the next session (`superpowers:writing-plans`)._
