# Phase 29 — `29-lb-maglev` — SPEC

> **Lifecycle state 1 (brainstorm output).** Authored by `superpowers:brainstorming`.
> Scope locked by **ADR-0071** (the phase-29 pick + scope decision). This SPEC is the
> requirements contract; `PLAN.md` (the next session's state-2 step) turns it into tasks
> after running the §6.2 empirical reconnaissance. Read this top-to-bottom with zero prior
> context (D-3.4).

## §0 — One-paragraph summary

Continue the **Load-balancing family** (ROADMAP §9 heading
`least_request, random, ring_hash, maglev, subset LB, locality-weighted LB, priority load
balancing, panic thresholds`) with the **`MAGLEV` consistent-hashing load balancer** — the
SECOND (and last) deterministic, byte-exact-differentiable LB policy, after phase-28's
`RING_HASH`. A cluster configured `lb_policy: MAGLEV` builds Google's Maglev lookup table
(a fixed prime-sized permutation table) over its endpoints; a request carrying the SAME
route-configured **header `hash_policy`** that phase 28 introduced is routed to the endpoint
the hashed header value maps to in the table — so the SAME header value always selects the
SAME backend, byte-identical to upstream Envoy v1.33.0. The **request side is identical to
phase 28** (route `hash_policy` header → xxHash64 → an `Option<u64>` key threaded through
`pick()`); only the **table-build strategy** differs from the ring. Like `RING_HASH` the
differential is a **normal request/response — fully observable on this Docker-Desktop dev
host** (no file-watch/reload trigger), so iteration pays no Linux-CI-only tax. The phase
consumes the phase-28 **M28-3** carry-forward: with a SECOND consistent-hash policy the
`ring.is_some()` LB discriminator is no longer sufficient, so it is replaced by an explicit
hash-LB dispatch. See ADR-0071 for the pick rationale and the rejected alternatives (CDS/LDS
hot-reload, least_request/random, subset/locality/priority LB).

## §1 — Goal & differential surface

**Goal.** Implement Envoy's `MAGLEV` cluster load-balancing policy (Maglev permutation-table
construction + table lookup) keyed by a route-level **header** `hash_policy`, behaviorally
equivalent to upstream Envoy v1.33.0 under the differential contract (§7.2 of
`BOOTSTRAP_PROMPT.md`).

**Differential surface at phase end (the new/changed green fixtures):**
- **Fixture `0037-lb-maglev`** (next free number; baseline is `0001`…`0036`): one cluster with
  `lb_policy: MAGLEV` and **two distinguishable real backends** (the phase-27 harness seed —
  `http1-echo-server` instances with per-backend `--body-marker`), an H1 listener whose route
  carries a header `hash_policy` (`{ header: { header_name: "x-hash-key" } }`). A direct clone
  of fixture-0036's shape, including the **`{{BACKEND_IP}}` / `discover_host_lan_ip`
  shared-address mechanism** (the Maglev table is IP-string-sensitive — both proxies MUST build
  the table from identical endpoint strings, exactly the ring-hash constraint from memory
  `consistent-hash-lb-differential-needs-identical-endpoint-strings`). The driver sends a
  **sweep of distinct `x-hash-key` values** and asserts, per value, that envoy-rust routes to
  the **same backend as Envoy** (proven by the response body marker) — cross-proxy identical
  selection (the **STRONG** differential target; see §6.2 + ADR-0071 for the same-key-stability
  fallback). It also asserts **same-key→same-backend stability** (repeat a value → same backend)
  and that the key set spreads across both backends (the table actually distributes).
- **All 36 pre-existing fixtures `0001`–`0036` stay green simultaneously** — `lb_policy: MAGLEV`
  is opt-in per cluster; every existing cluster keeps `ROUND_ROBIN` (or, for fixture 0036,
  `RING_HASH`), and the `pick()` dispatch must be a no-op for them (regression-equivalence; the
  load-bearing proof the M28-3 discriminator refactor is behavior-preserving for BOTH the
  round-robin path AND the existing RING_HASH path).

**Conformance:** h2spec pass-rate ≥95% (unchanged — no HTTP/2 codec change). No new conformance
suite. A new `parse_bootstrap` fuzz seed exercises the new config surface (`maglev_lb_config`);
NO new fuzz target (the Maglev table is covered by unit tests + the differential, reusing the
phase-28 xxHash64 fuzz coverage).

## §2 — Scope (minimum-viable)

Per §6.3 (no vague deferral): every capability is either IN this phase and tested, or an
explicit deferred non-goal with its own future home.

### §2.1 IN scope

1. **Config — cluster LB policy.** Extend `LbPolicy` (`crates/envoy-config/src/bootstrap.rs:297`,
   today `RoundRobin` + `RingHash`) with `Maglev`. Add `maglev_lb_config: Option<MaglevLbConfig>`
   to the cluster config (Envoy `Cluster.maglev_lb_config`), with `table_size` (Envoy default
   **65537**; must be a PRIME; Envoy max **5000011**). Validators: a `maglev_lb_config` on a
   non-`MAGLEV` cluster, a non-prime `table_size`, or a `table_size` over Envoy's max → fatal
   parse error (exact dispositions §6.2-verified at PLAN-write, mirroring the phase-28
   `RingSizeInversion`/`UnsupportedHashFunction` all-fatal posture per ADR-0049).
2. **The Maglev table + lookup.** A new `crates/envoy-cluster/src/maglev.rs` (sibling to
   phase-28's `ring_hash.rs`) implementing Envoy's Maglev permutation-table construction:
   for each host derive a per-host permutation from two host-key hashes (`offset = h1 %
   table_size`, `skip = h2 % (table_size − 1) + 1`; `permutation[j] = (offset + j·skip) %
   table_size`), then populate the `table[0..table_size]` of host indices by the round-robin
   claim loop until full. Lookup = `table[request_hash % table_size]` → host index. **The EXACT
   host-key string(s), which hash seeds drive `offset` vs `skip`, the prime-`table_size`
   handling, and the populate order are §6.2-locked at PLAN-write to match Envoy byte-for-byte**
   (the phase-28 verify-at-PLAN-write discipline). Uses the phase-28 **xxHash64 (seed 0) from
   `crates/envoy-cluster/src/xxhash.rs`** for the host-key hashes (reused, not rebuilt).
3. **The M28-3 discriminator refactor (load-bearing).** (Naming note: phase-28 `REVIEW.md`
   *defines* M28-3 as the now-dead `host_index < total` ring-guard observation; the
   discriminator refactor is REVIEW.md's *recommendation that closes* M28-3 — ADR-0071 +
   ROADMAP row 29 adopt this "M28-3 discriminator refactor" shorthand, and so does this SPEC.)
   Replace the phase-28 `ring:
   Option<HashRing>` discriminator on `Cluster` (`cluster.rs:209`, where `ring.is_some()` ==
   "this is RING_HASH") with an explicit hash-LB dispatch — the design point §3.1: e.g.
   `hash_lb: Option<HashLb>` where `HashLb { Ring(HashRing), Maglev(MaglevTable) }`, built in
   `from_bootstrap` from `lb_policy`. `pick()` dispatches on the `HashLb` variant (`None` →
   the phase-02 cursor path; `Some(Ring)` → ring lookup; `Some(Maglev)` → table lookup). This
   is the explicit instruction in the phase-28 Maglev-footgun guard comment at the
   `from_bootstrap` build site, and the M28-3 carry-forward. The RING_HASH path stays
   behaviorally identical (fixture 0036 green); the round-robin path stays a no-op (key inert).
4. **Reused as-is (no behavioral change — the request side is identical to phase 28):** the
   route `hash_policy: Vec<HashPolicy>` config + `validate_hash_policy` (header source;
   `bootstrap.rs:1369`); the `HashFunction` enum (XX_HASH only; `bootstrap.rs:338`); the
   `pick()` / `pick_endpoint()` `Option<u64>` request-hash-key threading; the H1/H2 HCM
   hash-key extraction with the load-bearing present-empty-vs-absent `.map()` distinction
   (`crates/envoy-http1/src/hcm.rs`, `crates/envoy-http2/src/hcm.rs`). A MAGLEV cluster keys on
   the same `x-hash-key` header path as a RING_HASH cluster.
5. **Hash-policy-absent fallback.** A `MAGLEV` cluster whose matched route has NO `hash_policy`,
   or where the named header is absent → the same no-hash fallback the phase-28 path already
   uses (the cursor path; §6.2-verified the fallback matches Envoy's documented random-host
   behavior closely enough — recorded as the phase-28 **M28-2** characterization, NOT a
   differential assertion). The fixture always supplies the header (this is a backstop concern).
6. **Health / outlier composition.** As in phase 28: the MVP fixture uses a PLAIN cluster (no
   HC/OD), so MAGLEV+HC/OD compose as a *differential* is a deferred non-goal (§2.2); the
   table host the lookup selects is returned directly. If §6.2 shows Envoy's skip-and-retry for
   an unhealthy table host is cheap to match, it MAY be a backstop item (a PLAN-write call).
7. **Stats.** Any per-cluster LB stat Envoy emits for maglev (e.g. a table-size gauge) — emitted
   only if §6.2 confirms a portable namespace; otherwise none (the phase-21/24/28 discipline).
   §6.2-verified at PLAN-write.
8. **Tests.** Fixture `0037` (the differential above) + an in-process backstop (table
   determinism; same-key→same-host; spread across hosts; the no-hash-key fallback; single-host
   table; the cursor/round-robin path unchanged; the RING_HASH path unchanged — the M28-3
   refactor regression witness) + a `parse_bootstrap` fuzz seed + a BEHAVIOR_CONTRACT
   "LB selection" extension (a MAGLEV row; **M28-1 folded** — a sentence that
   `RingHashLbConfig.maximum_ring_size` is parse-validation-only / the ring build is
   `minimum_ring_size`-governed).

### §2.2 DEFERRED non-goals (explicit; each names its future home)

- **`least_request` / `random`** (non-deterministic → not byte-exact differentiable; they need a
  contract-relaxation ADR before they can be a differential phase — a separate future phase).
- **`MURMUR_HASH_2`** (XX_HASH only this phase, mirroring phase 28).
- **Non-header hash sources** — cookie, connection source-IP, query-parameter, filter-state; plus
  `hash_policy.terminal`, multi-policy hash combination, and `regex_rewrite` on the header value
  (all deferred at phase 28; unchanged here).
- **Weighted Maglev** — `LbEndpoint.load_balancing_weight` → unequal table shares. Phase-27
  endpoints are weight-1; weighted hosts defer.
- **`MAGLEV` + EDS-hot-reload composition** (re-building the table when the phase-27 endpoint set
  hot-swaps) — the fixture uses a static MAGLEV cluster; deferred (a future EDS/LB cross-phase).
- **`MAGLEV` + active-HC / outlier-detection composition** as a *differential* — exercised only
  by the backstop unless §6.2 shows it cheap (PLAN-write call), mirroring phase 28.
- **subset LB / locality-weighted LB / priority load balancing / panic-threshold-for-hashing**
  (the rest of the LB family — later phases).
- **CDS/LDS hot-reload** (the deferred xDS layers — ADR-0065/0067); the **gRPC/ADS transport**
  (still ADR-0014/H2-trailers-blocked).

## §3 — Open PLAN-write design calls (resolved at state-2, §6.2-informed)

These are decisions the state-2 PLAN-write makes after the §6.2 reconnaissance; the brainstorm
deliberately leaves them open:

1. **The M28-3 discriminator shape** — the exact `Cluster` field/type that replaces `ring:
   Option<HashRing>` (a `hash_lb: Option<HashLb>` enum vs a `lb_policy` discriminant field + two
   `Option` tables vs a boxed trait object). The constraint: `pick()` stays allocation-free on
   the round-robin fast path; the RING_HASH path stays byte-identical (fixture 0036 green).
2. **The exact Maglev table construction** — Envoy's per-host hash-key string(s), which hash
   seeds derive `offset` vs `skip`, the prime-`table_size` enforcement (does Envoy reject a
   non-prime `table_size`, round it, or accept it?), and the populate order. §6.2
   reverse-engineers this from observed key→backend mappings.
3. **The differential equivalence target** — STRONG (cross-proxy identical selection) if exact
   table replication is achievable, else the **same-key-stability** fallback (per-proxy
   consistency + distribution sanity) defined by a reconciliation ADR (ADR-0072).
4. **The hash-policy-absent fallback** behavior — confirm it matches the phase-28 path (M28-2).
5. **Health/outlier skip-and-retry** precise semantics for maglev, and whether MAGLEV+HC/OD is
   in differential scope or backstop-only.
6. **LB stat namespace** (if any) — §6.2-verified.
7. **The §6.1 split decision** — see §6.1.

## §4 — Reuse map (what exists; do not rebuild)

- **xxHash64 (seed 0) from scratch** (`crates/envoy-cluster/src/xxhash.rs`) — phase 28; the
  Maglev table-build host-key hashes reuse it. NO re-implementation.
- **The route `hash_policy` config + validator + the request-hash plumbing** — phase 28: the
  `hash_policy: Vec<HashPolicy>` on `RouteAction_Route` (`bootstrap.rs:1369`),
  `validate_hash_policy` (`:1419`), the `HashFunction` enum (`:338`), the `pick()`/
  `pick_endpoint()` `Option<u64>` threading (`cluster.rs:342`), and the H1/H2 HCM extraction
  (the present-empty-vs-absent `.map()` distinction). A MAGLEV cluster keys identically.
- **The `LbPolicy` enum + `RingHashLbConfig` pattern** (`bootstrap.rs:297`, `:313`) — extend with
  `Maglev` + `MaglevLbConfig` following the `RingHashLbConfig` serde-default + validator pattern.
- **The `ring_hash.rs` `HashRing`** (`crates/envoy-cluster/src/ring_hash.rs`) — the structural
  sibling for `maglev.rs` (build-once-at-construction over the endpoint snapshot; a `lookup`
  returning a host index aligned with the `endpoints` Vec).
- **The LB `pick()` + health/outlier slow path + panic threshold** (`cluster.rs:342-432`) —
  extend the dispatch (M28-3 refactor); the round-robin + RING_HASH paths stay behavior-preserving.
- **Fixture `0036-lb-ring-hash`** (`tests/fixtures/0036-lb-ring-hash/` + the
  `tests/differential/src/lib.rs` key-sweep driver + `discover_host_lan_ip` / `{{BACKEND_IP}}`)
  — the template for fixture 0037; the two-distinguishable-backend harness is already built.
- **The H1/H2 HCM request paths** that already extract the hash-key before `pick_endpoint()`.

## §5 — Behavioral contract notes

- **Determinism:** same `x-hash-key` value → same backend, on each proxy, across requests
  (consistency); and (strong target) the same backend on BOTH proxies (cross-proxy identity).
- **Distribution:** distinct keys spread across the table (both backends receive traffic).
- **Regression-equivalence:** every `ROUND_ROBIN` cluster AND the existing `RING_HASH` cluster
  (fixture 0036) behave exactly as before (the M28-3 discriminator-refactor no-op proof — all 36
  existing fixtures green).
- **Config validity:** invalid `maglev_lb_config` is a startup-fatal parse error (ADR-0049
  all-fatal; no reload path this phase).
- **Differential locality:** the maglev selection is observable WITHOUT a file-watch/reload
  trigger → the fixture-0037 differential runs and is authoritative on this Docker-Desktop host
  (NOT Linux-CI-only, unlike phases 26/27).

## §6 — Process

### §6.1 — Split projection (§6.1 gate)

A split is projected **NOT to fire** — the surface is materially SMALLER than phase 28 because
xxHash64, the entire route-`hash_policy` request-hash plumbing, the `LbPolicy`/`MaglevLbConfig`
config pattern, and the two-distinguishable-backend fixture harness ALL already exist. The new
surface = `MaglevLbConfig` + validators + `maglev.rs` (the table build + lookup) + the M28-3
discriminator refactor + fixture 0037 + backstop + a fuzz seed + BEHAVIOR_CONTRACT (~700–1000
LoC / ~8 tasks), well under the ~1500-LoC / ~25-task §6.1 gate. The decision is confirmed at the
state-2 PLAN-write. **ADR-0073 is reserved** for the split (fires only if it happens).

### §6.2 — Empirical reconnaissance (run at the state-2 PLAN-write, LOCALLY)

Like phase 28 (and unlike phases 26/27), this phase's behavior is **locally observable** (no
reload trigger). At the state-2 PLAN-write, stand up `envoyproxy/envoy:v1.33.0` with one
`MAGLEV` cluster + two distinguishable backends + a header `hash_policy`, and:
1. Sweep a set of `x-hash-key` values; RECORD which backend each maps to (the ground truth the
   differential will assert against).
2. Reverse-engineer Envoy's Maglev table: the host-key string(s), the `offset`/`skip` hash
   derivation (confirm XX_HASH/xxHash64 seed 0 and the seeds), the prime `table_size` handling
   (default 65537; reject vs round vs accept a non-prime), and the populate order — precisely
   enough to replicate so envoy-rust reproduces the recorded mapping.
3. Verify the hash-policy-absent fallback (matches M28-2), any LB stat namespace, the
   `maglev_lb_config` wire shape, and the invalid-config dispositions.
4. Decide STRONG (cross-proxy identical selection) vs the same-key-stability fallback contract.
**ADR-0072 FIRES** at the PLAN-write if any of these materially diverge from this SPEC's
projection (notably if exact table replication is intractable → the fallback equivalence
contract, or if the table-build hash differs from xxHash64-seed-0). `PLAN.md` lands with the
empirically-locked facts inline (no `[§6.2-PENDING]` projections — the phase-27/28
verify-at-PLAN-write discipline).

### §6.3 — Anti-deferral

No vague TODOs. Every §2.1 item is implemented + tested this phase; every deferral is a §2.2
named non-goal with a future home. The Maglev table, the dispatch refactor, and the fixture are
real and differentially exercised (or backstop-exercised where §2.2 says so) — no stubs.

## §7 — Acceptance (the §7.5 phase-done gate, previewed)

(a) fixture `0037` green + (b) all of `0001`–`0036` green + (c) h2spec ≥95% + (d) the
`parse_bootstrap` fuzz seed clean + (e) `cargo build --workspace --all-targets` / `cargo clippy
--workspace --all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo
test --workspace` / `cargo deny check` all clean + (f) `REVIEW.md` approved.
`#![forbid(unsafe_code)]` holds (D-3.8).

---

_Scope locked by **ADR-0071**. ADR-0072 reserved (§6.2 reconciliation), ADR-0073 reserved (§6.1
split). The state-2 PLAN-write is the next session (`superpowers:writing-plans`)._
