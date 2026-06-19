# Phase 28 — `28-lb-ring-hash` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0069** (the phase-28 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

Open the **Load-balancing family** (the first concrete row under the ROADMAP §9 `Load balancing`
heading; the project has shipped only `ROUND_ROBIN` since phase 02) with the **`RING_HASH`
consistent-hashing load balancer**. A cluster configured `lb_policy: RING_HASH` builds a hash
ring over its endpoints; a request carrying a route-configured **header `hash_policy`** is routed
to the endpoint the hashed header value maps to on the ring — so the SAME header value always
selects the SAME backend (session affinity / cache locality), and changing the value re-targets
deterministically. This is the only LB-family opener that is **both foundational and byte-exact
differentiable**: with two distinguishable backends in one cluster, the chosen backend is
observable on the wire, and `RING_HASH` (unlike `random`/`least_request`) is deterministic, so
both upstream Envoy and envoy-rust must select the SAME backend for the same key. Crucially the
differential is a **normal request/response — fully observable on this Docker-Desktop dev host**
(no file-watch/reload trigger, unlike phases 26/27), so iteration does not pay the Linux-CI-only
tax. See ADR-0069 for the pick rationale and the rejected alternatives (CDS/LDS hot-reload,
maglev, random/least_request, cookie/source-IP hash).

## §1 — Goal & differential surface

**Goal.** Implement Envoy's `RING_HASH` cluster load-balancing policy (ketama-style hash ring +
next-clockwise lookup) keyed by a route-level **header** `hash_policy`, behaviorally equivalent
to upstream Envoy v1.33.0 under the differential contract (§7.2 of `BOOTSTRAP_PROMPT.md`).

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0036-lb-ring-hash`** (next free number; baseline is `0001`…`0035`): one cluster with
  `lb_policy: RING_HASH` and **two distinguishable real backends** (the phase-27 harness seed —
  `http1-echo-server` instances with per-backend `--body-marker`, reached by
  `{{HTTP1_BACKEND_1_PORT}}` / `{{HTTP1_BACKEND_2_PORT}}`), an H1 listener whose route carries a
  header `hash_policy` (`{ header: { header_name: "x-hash-key" } }`). The driver sends a **sweep
  of distinct `x-hash-key` values** and asserts, per value, that envoy-rust routes to the **same
  backend as Envoy** (proven by the response body marker) — i.e. cross-proxy identical selection
  (the **strong** differential target; see §6.2 + ADR-0069 for the same-key-stability fallback).
  It also asserts **same-key→same-backend stability** (repeat a value → same backend) and that
  the key set spreads across both backends (the ring actually distributes).
- **All 35 pre-existing fixtures `0001`–`0035` stay green simultaneously** — `lb_policy:
  RING_HASH` is opt-in per cluster; every existing cluster keeps `ROUND_ROBIN`, and the `pick()`
  dispatch must be a no-op for them (regression-equivalence; the load-bearing proof the
  policy-dispatch + the `pick_endpoint()` signature change are behavior-preserving for the
  round-robin path).

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance
suite. A new `parse_bootstrap` fuzz seed exercises the new config surface (`ring_hash_lb_config`
+ route `hash_policy`); NO new fuzz target unless §6.2/PLAN-write shows a new parser worth one
(the xxHash implementation is covered by unit tests + the differential, not a fuzzer, unless the
PLAN-write decides otherwise).

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an
explicit deferred non-goal with its own future home.

### §2.1 IN scope

1. **Config — cluster LB policy.** Extend `LbPolicy` (`crates/envoy-config/src/bootstrap.rs:290`,
   today the lone `RoundRobin`) with `RingHash`. Add `ring_hash_lb_config: Option<RingHashLbConfig>`
   to the cluster config (Envoy `Cluster.ring_hash_lb_config`), with `minimum_ring_size`
   (default 1024), `maximum_ring_size` (default 8M), and `hash_function` (accept **XX_HASH only**;
   MURMUR_HASH_2 and any other value → all-fatal parse error per ADR-0049). Validators: a
   `ring_hash_lb_config` on a non-`RING_HASH` cluster, `minimum_ring_size > maximum_ring_size`, or
   sizes out of Envoy's bounds → fatal parse error (exact dispositions §6.2-verified at PLAN-write).
2. **Config — route header `hash_policy`.** Add `hash_policy: Vec<HashPolicy>` to
   `RouteAction_Route` (`bootstrap.rs:1303`, today `{ cluster, retry_policy }`), where `HashPolicy`
   carries a **header** source `{ header: { header_name: String } }`. A single header policy is the
   MVP; multiple-policy combination, `terminal`, and non-header sources are deferred (§2.2).
3. **xxHash64 (XX_HASH) from scratch.** Implement Envoy's default ring_hash hash function
   (xxHash64, seed 0) from scratch (D-3.2: LB algorithms are written from scratch; no hashing crate
   exists in-tree or is permitted). Used for BOTH the ring-key hashes and the request header-value
   hash (the exact hashed-string formats are §6.2-locked at PLAN-write). Unit-tested against known
   xxHash64 vectors.
4. **The ring + lookup.** Build a sorted hash ring over the cluster's endpoints (replica count per
   host derived from `minimum_ring_size`; the EXACT per-host ring-key string format + replica math
   §6.2-locked to match Envoy). Lookup = binary-search for the first ring entry ≥ the request hash,
   wrapping to the ring start. Built once at cluster construction over the (phase-27) endpoint
   handle's current set; **the ring's interaction with the phase-27 swappable endpoint set is a
   §6.2/PLAN-write design point** — for this phase, EDS-hot-reload + RING_HASH composition is a
   deferred non-goal (§2.2); the fixture uses a static (non-EDS) RING_HASH cluster.
5. **Request hash plumbing.** Thread an optional request hash key into the LB selection: extract
   the `hash_policy` header value at request time, compute its hash, and pass it to the LB
   selection. `pick_endpoint()` (`cluster.rs:549`) is the **public** wrapper both HCMs call;
   `pick()` (`cluster.rs:322`, today takes no per-request context) is the **private** cursor /
   health / outlier core — the optional hash key AND the `lb_policy` dispatch thread through BOTH.
   The two per-request call sites — H1 `crates/envoy-http1/src/hcm.rs:392`, H2
   `crates/envoy-http2/src/hcm.rs:184` — change to pass the key. The dispatch: `RoundRobin` → the
   existing cursor path (unchanged; key ignored); `RingHash` → the ring lookup.
6. **Hash-policy-absent fallback.** A `RING_HASH` cluster whose matched route has NO `hash_policy`,
   or where the named header is absent on the request → Envoy falls back to a random host. Implement
   envoy-rust's faithful equivalent (§6.2-verified; likely the existing no-hash `pick()` path). The
   fixture always supplies the header (so this is a backstop concern, not a differential one).
   **NOTE — the existing no-host `pick() → None` path is protocol-asymmetric** (H1 → 503
   `hcm.rs:392`, H2 → 502 `hcm.rs:184`); fixture 0036 is H1-only so the differential is unaffected,
   but the §2.1.9 backstop must account for the per-protocol no-host outcome (this also covers the
   empty-ring / single-host edge cases).
7. **Health / outlier composition.** `RING_HASH` must compose with the existing health-filtering /
   outlier-ejection slow path (`cluster.rs:340-373`): an unhealthy/ejected host the ring would
   select is skipped per Envoy's documented ring_hash behavior (retry the next ring entry).
   The MVP fixture uses a PLAIN cluster (no HC/OD) so this is exercised by the backstop; the exact
   Envoy skip-and-retry semantics are §6.2-noted (if matching them precisely is heavy, an HC/OD +
   RING_HASH composition may be a deferred non-goal — a PLAN-write call).
8. **Stats.** Any per-cluster LB stat Envoy emits for ring_hash (e.g. a ring-size gauge) — emitted
   only if §6.2 confirms a portable namespace; otherwise none (the phase-21/24 "stays Envoy-only"
   discipline). §6.2-verified at PLAN-write.
9. **Tests.** Fixture `0036` (the differential above) + an in-process backstop (ring determinism;
   same-key→same-host; spread across hosts; the no-hash-key fallback; single-host ring; the
   health/outlier skip; cursor/round-robin path unchanged) + a `parse_bootstrap` fuzz seed +
   BEHAVIOR_CONTRACT "LB selection" extension.

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **`maglev`** (the other consistent-hash policy — more algorithm to match byte-for-byte; the LB
  family's next LB-opener candidate).
- **`least_request` / `random`** (non-deterministic → not byte-exact differentiable; they need a
  contract-relaxation ADR before they can be a differential phase — a separate future phase).
- **`MURMUR_HASH_2`** (XX_HASH only this phase).
- **Non-header hash sources** — cookie, connection source-IP, query-parameter, filter-state; plus
  `hash_policy.terminal`, multi-policy hash combination, and `regex_rewrite` on the header value.
- **Weighted ring** — `LbEndpoint.load_balancing_weight` → unequal ring replicas. Phase-27 endpoints
  are weight-1; weighted hosts defer.
- **`RingHashLbConfig.use_hostname_for_hashing`** and other ring_hash sub-options beyond min/max
  size + hash_function.
- **`RING_HASH` + EDS-hot-reload composition** (re-building the ring when the phase-27 endpoint set
  hot-swaps) — the fixture uses a static RING_HASH cluster; the EDS+RING_HASH interaction is a
  deferred non-goal (a future EDS/LB cross-phase).
- **`RING_HASH` + active-HC / outlier-detection composition** as a *differential* — exercised only
  by the backstop unless §6.2 shows it cheap (PLAN-write call).
- **subset LB / locality-weighted LB / priority load balancing / panic-threshold-for-hashing** (the
  rest of the LB family — later phases).
- **CDS/LDS hot-reload** (the deferred xDS layers — ADR-0065/0067); the **gRPC/ADS transport** (still
  ADR-0014/H2-trailers-blocked).

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The exact ring construction** — Envoy's per-host ring-key string format (e.g.
   `"<address>_<i>"` vs `"<address>:<port>_<i>"` vs hostname-based), the replica count math from
   `minimum_ring_size` / weights, and whether the request header-hash uses the same xxHash64 path
   as the ring keys. §6.2 reverse-engineers this from observed key→backend mappings.
2. **The differential equivalence target** — STRONG (cross-proxy identical selection) if exact ring
   replication is achievable, else the **same-key-stability** fallback (per-proxy consistency +
   distribution sanity) defined by a reconciliation ADR (ADR-0070). ADR-0069 §"differential-
   equivalence question" specifies both.
3. **The `pick()` / `pick_endpoint()` signature** — how the optional hash key is threaded (a
   `Option<u64>` precomputed hash vs a `&[u8]` key hashed inside `pick`); where the header
   extraction + hash lives (HCM request path vs cluster); keeping the round-robin path allocation-
   free.
4. **The hash-policy-absent fallback** behavior (random host vs round-robin path) — §6.2-verified.
5. **Health/outlier skip-and-retry** precise semantics for ring_hash, and whether HC/OD+RING_HASH
   is in differential scope or backstop-only.
6. **LB stat namespace** (if any) — §6.2-verified.
7. **The §6.1 split decision** — see §6.1.

## §4 — Reuse map (what exists; do not rebuild)

- **The two-distinguishable-backend harness** (phase 27): `Http1EchoBackend::spawn_with_marker`,
  the `{{HTTP1_BACKEND_1_PORT}}` / `{{HTTP1_BACKEND_2_PORT}}` template substitution + dual-backend
  spawn (`tests/differential/src/lib.rs:3019-3041`), per-backend `--body-marker` (response body
  begins `backend: <marker>`). A single cluster with two endpoints pointing at the two backends is
  already expressible.
- **The LB `pick()` + health/outlier slow path + panic threshold** (`crates/envoy-cluster/src/cluster.rs:322-373`)
  — extend, don't replace; the round-robin fast path stays the `RoundRobin` arm.
- **The phase-27 endpoint handle** (`endpoints: RwLock<Arc<Vec<SocketAddr>>>`, read-once per
  selection) — the ring is built over the current endpoint snapshot (EDS-hot-reload re-ring is a
  non-goal, §2.2).
- **The cluster config + `LbPolicy` enum + validators** (`crates/envoy-config/src/bootstrap.rs`),
  the route config (`RouteAction_Route`), and the existing all-fatal config-error machinery
  (ADR-0049).
- **The H1/H2 HCM request paths** that already have the request headers in hand before the
  `pick_endpoint()` call (the hash-key extraction site).

## §5 — Behavioral contract notes

- **Determinism:** same `x-hash-key` value → same backend, on each proxy, across requests
  (consistency); and (strong target) the same backend on BOTH proxies (cross-proxy identity).
- **Distribution:** distinct keys spread across the ring (both backends receive traffic over a key
  sweep).
- **Regression-equivalence:** every `ROUND_ROBIN` cluster behaves exactly as before (the dispatch
  no-op proof — all 35 existing fixtures green).
- **Config validity:** invalid `ring_hash_lb_config` / `hash_policy` is a startup-fatal parse error
  (ADR-0049 all-fatal; no reload path this phase).
- **Differential locality:** the ring_hash selection is observable WITHOUT a file-watch/reload
  trigger → the fixture-0036 differential runs and is authoritative on this Docker-Desktop host
  (NOT Linux-CI-only, unlike phases 26/27).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split into **28.1** (config: `LbPolicy::RingHash` + `RingHashLbConfig` + route `hash_policy` +
validators + xxHash64-from-scratch + the `pick()`/`pick_endpoint()` hash-key threading — a
foundation slice, no new fixture, regression-equivalent) / **28.2** (the ring construction + lookup
+ health/outlier compose + fixture 0036 + backstop + BEHAVIOR_CONTRACT + close) is held in nominal
reserve and is **PLAUSIBLE** — the surface (xxHash + ring + new cluster config + new route config +
the request-path threading) is meaty. The decision is made at the state-2 PLAN-write against the
§6.1 thresholds (~25 tasks / ~1500 LoC). **ADR-0071 is reserved** for the split (fires only if it
happens).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Unlike phases 26/27, this phase's behavior is **locally observable** (no reload trigger). At the
state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0` with one `RING_HASH` cluster + two
distinguishable backends + a header `hash_policy`, and:
1. Sweep a set of `x-hash-key` values; RECORD which backend each maps to (the ground truth the
   differential will assert against).
2. Reverse-engineer Envoy's ring: the hash function (confirm XX_HASH/xxHash64 seed 0), the per-host
   ring-key string format, the replica count from `minimum_ring_size`, and the header-hash path —
   precisely enough to replicate so envoy-rust reproduces the recorded mapping.
3. Verify the hash-policy-absent fallback, any LB stat namespace, the `ring_hash_lb_config` /
   `hash_policy` config wire shapes, and the invalid-config dispositions.
4. Decide STRONG (cross-proxy identical selection) vs the same-key-stability fallback contract.
**ADR-0070 fires** at the PLAN-write if any of these materially diverge from this SPEC's projection
(notably if exact ring replication is intractable → the fallback equivalence contract). `PLAN.md`
lands with the empirically-locked facts inline (no `[§6.2-PENDING]` projections, the phase-27
verify-at-PLAN-write discipline).

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2 named
non-goal with a future home. The xxHash, the ring, and the dispatch are real and differentially
exercised (or backstop-exercised where §2.2 says so) — no stubs.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0036` green + (b) all of `0001`–`0035` green + (c) h2spec ≥95% + (d) the
`parse_bootstrap` fuzz seed clean (+ any new fuzz target if the PLAN-write adds one) + (e)
`cargo build --workspace --all-targets` / `cargo clippy --workspace --all-targets --all-features
-- -D warnings` / `cargo fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all
clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds (D-3.8).

---

_Scope locked by **ADR-0069**. ADR-0070 reserved (§6.2 reconciliation), ADR-0071 reserved (§6.1
split). The state-2 PLAN-write is the next session (`superpowers:writing-plans`)._
