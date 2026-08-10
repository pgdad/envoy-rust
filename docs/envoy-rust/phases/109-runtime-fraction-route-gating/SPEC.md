# Phase 109 — Runtime CONSUMER slice 1: route `match.runtime_fraction` gating (deterministic 0/≥100), witnessed by NEW cluster-free fixture `0088`

> Brainstorming output of the §5 state-0/1 next-phase pick session (ADR-0175).
> Written for a reader with ZERO prior context (D-3.4). Every inherited figure
> here is a CLAIM the state-2 PLAN-write must re-derive on disk before relying
> on it (§7); every upstream behaviour cited as MEASURED was probed against the
> pinned image at this session and is tabulated in §1.1.

## §0. How to read this document

- §1 is the recon evidence: the 13-cell upstream probe matrix (§1.1) and the
  tree census (§1.2), both produced at this session — the probes by the main
  session against `envoyproxy/envoy:v1.33.0` (digest verified by
  `docker image inspect` before any probe), the census by a read-only subagent
  and then RE-VERIFIED line-by-line on disk by the main session.
- §2 argues the pick against the cheapest-strong-differential bar.
- §3 is the scope; §5 the non-goals; §6 the carry-forward ledger.
- §7 lists the PLAN-VERIFY items the state-2 session must re-confirm fresh.
- §9 sizes the phase and projects the §6.1 split decision (ADR-0176 is
  RESERVED-UNFIRED for it).
- This phase is the FIRST CONSUMER of the runtime snapshot store that the
  landed `108` family built (producer + observer only). It changes the
  behaviour of route matching — the first time any runtime key influences the
  data plane.

## §1. State-0 recon — the evidence this pick rests on

### §1.1 The upstream probe matrix (MEASURED at this session, pinned image)

Probe harness: one HCM listener; route 1 = `prefix: "/"` +
`runtime_fraction { default_value { numerator: N, denominator: D }, runtime_key: "probe.frac" }`
→ `direct_response` 200 body `GATED`; route 2 = bare `prefix: "/"` →
`direct_response` 200 body `FALLBACK`. Docker port-mapped (`-p`), admin
`/ready` awaited before probing. All probe configs passed `--mode validate`
first. Counts are over n independent requests against one unchanged process.

| # | default_value | static_layer `probe.frac` | result |
|---|---|---|---|
| 1 | 100/HUNDRED | (no `layered_runtime`) | GATED 30/30 — **absent key → default_value honored, deterministic** |
| 2 | 0/HUNDRED | (no `layered_runtime`) | FALLBACK 30/30 — default 0 honored |
| 3 | 100/HUNDRED | `0` (int) | FALLBACK 30/30 — **key overrides default** |
| 4 | 0/HUNDRED | `100` (int) | GATED 30/30 |
| 5 | 100/HUNDRED | `50` (int) | GATED 27 / FALLBACK 33 over n=60 — **strictly-between values are per-request NONDETERMINISTIC**; excluded from fixtures |
| 6 | 100/HUNDRED | `"0"` (quoted numeric string) | FALLBACK 30/30 — numeric string parses like the integer |
| 7 | 100/HUNDRED | `{numerator: 0, denominator: HUNDRED}` (map) | FALLBACK 30/30 — map-shaped FractionalPercent honored for ROUTING |
| 8 | 0/HUNDRED | `{numerator: 100, denominator: HUNDRED}` (map) | GATED 30/30 |
| 9 | 0/**MILLION** | `100` (int) | **GATED 40/40 — an integer runtime value is numerator over HUNDRED, NOT over the default's denominator** (100/10⁶ would be ~never over 40 tries) |
| 10 | 100/HUNDRED | `"abc"` (unparseable) | GATED 30/30 — unparseable → default_value used |
| 11 | 0/HUNDRED | `"abc"` | FALLBACK 30/30 — **confirms default-used in BOTH directions** |
| 12 | 0/HUNDRED | `200` (int) | GATED 30/30 — **numerator ≥ denominator ⇒ always matches** |
| 13 | 100/HUNDRED | two static layers: base `100`, override `0` | FALLBACK 30/30 — **the consumer honors last-layer-wins `final_value`** |

Measured alongside: `/runtime` for cell 7 renders the map value as the
protobuf **text-format dump** (`"fields {\n  key: \"denominator\"…"`) — the
banked **CF-108-3** divergence, live on this path (see §3 D3 and §6).

Determinism summary the whole phase rests on: effective value `v` — `v == 0`
→ never matches; `v ≥ 100` (integer values are always over HUNDRED) → always
matches; `0 < v < 100` → per-request random (out of scope, boot-fatal here);
absent or unparseable key → `default_value`.

### §1.2 The tree census (subagent recon, RE-VERIFIED on disk by the main session)

- **`RouteMatch`** — `crates/envoy-config/src/bootstrap.rs:2895-2904`:
  `#[serde(deny_unknown_fields)]`, exactly three fields (`prefix`, `path`,
  `headers`). `runtime_fraction` is **NOT modeled: zero grep hits across
  `crates/` and `tests/`** (re-run by the main session; docs-only hits remain).
  A config carrying `match.runtime_fraction:` is boot-fatal today while
  upstream loads and serves it.
- **`RuntimeFractionalPercent` already exists** —
  `crates/envoy-config/src/bootstrap.rs:1504-1508`
  (`default_value: FractionalPercent`, `runtime_key: Option<String>`), used
  today ONLY by `CsrfPolicy.filter_enabled` (`bootstrap.rs:1520`), where a
  present `runtime_key` is **rejected** at boot
  (`validate_csrf_config`, `bootstrap.rs:4863-4874`,
  `ConfigError::UnsupportedRuntimeKeyedCsrfFilterEnabled`). The type is
  reusable as the wire schema for the new field; the CSRF reject is untouched.
- **`FractionalPercent::selects_deterministic`** —
  `bootstrap.rs:1321-1323`: the house 0%/100%-only discipline (fault + CSRF
  precedent, no PRNG anywhere in the tree — the only `rand` hit is a test-only
  `aws_lc_rs::rand` in `envoy-jwt/src/test_support.rs:55`; `fastrand` is
  transitive-dev-only via `tempfile`).
- **The snapshot store has NO lookup API** —
  `crates/envoy-config/src/runtime.rs`: `RuntimeSnapshot { layer_names,
  entries: BTreeMap<String, RuntimeEntry> }` (`:38-44`), `RuntimeEntry
  { layer_values, final_value: String }` (`:26-33`), constructors
  `from_layers` (`:80`) / `from_bootstrap` (`:131`), plus `num_layers`/
  `num_keys`. No `get`, no `get_integer`, no `feature_enabled` — this phase
  adds the first typed lookup.
- **The snapshot is never stored — it is recomputed per use.** Exactly two
  consumers exist: `crates/envoy-bin/src/runtime_stats.rs:34` (startup gauges,
  dropped immediately) and `crates/envoy-admin/src/endpoint.rs:982` (rebuilt
  per `GET /runtime` request from `AdminHandler`'s `Arc<Bootstrap>`). There is
  no `Arc<RuntimeSnapshot>` anywhere. Since NOTHING mutates runtime state
  after boot in this tree (no RTDS, no `/runtime_modify`, no disk layer), the
  effective gate value of every route is **fixed for the process lifetime**.
- **Route matching sees no runtime state.** `route_matches`
  (`crates/envoy-http1/src/hcm.rs:2182-2193`) is a private free function over
  `(r: &Route, path, headers)`; it is called from exactly TWO production
  sites — `hcm.rs:2028` (inside `resolve_route_in`) and `hcm.rs:2094` (inside
  `build_response_in`) — and the doc at `hcm.rs:1994-1996` records that both
  MUST stay identical ("the 30-fixture regression-equivalence guarantee").
  The H2 crate reuses the H1 resolver (`crates/envoy-http2/src/hcm.rs:475`
  calls `envoy_http1::hcm::resolve_route`), so one seam serves both protocols.
  `HCMConfig` (`hcm.rs:124-177`) carries no `Bootstrap` and no snapshot;
  threading one in touches `HCMConfig::from_config` (`hcm.rs:180-185`, all
  three production call sites in `crates/envoy-bin/src/main.rs` have
  `Arc<Bootstrap>` in scope) and **47 `HCMConfig { … }` struct-literal
  construction sites** (re-counted by the main session; nearly all
  `#[cfg(test)]`) — the dominant mechanical cost, and a workspace-wide
  `E0063` blast radius per the standing public-struct-field trap.
- **`RouteMatch` is REUSED by jwt_authn** — `RequirementRule.r#match`
  (`bootstrap.rs:1386`), matched by the hand-copied `route_match_matches`
  (`crates/envoy-filter/src/jwt_authn.rs:173-186`, the CF-76-1 second
  matcher). Adding a field to `RouteMatch` silently widens the JWT wire
  surface; §3 D5 decides the posture.
- **Harness machinery is fully in place**: `Driver::Http1ProbeList`
  (`tests/differential/src/lib.rs:119-121`, probe struct `:1156-1177` with
  `expected_status`/`expected_body`/`expected_headers`), used by 13 fixtures;
  `0083`/`0086` are cluster-free `direct_response` templates; fixture `0087`
  proves both sides accept a two-static-layer `layered_runtime` block.
- **Absence assertions**: the four sites this phase FALSIFIES and must narrow
  are `crates/envoy-config/src/runtime.rs:10-14` ("Nothing reads this store
  yet"), `docs/envoy-rust/BEHAVIOR_CONTRACT.md:3168-3171` ("Nothing READS the
  runtime store for behavior yet — the consumer slice … is future work"),
  `crates/envoy-bin/src/runtime_stats.rs:15-17` (consumer-absence wording),
  and the general "no runtime subsystem" framing wherever it survives. The
  sites that STAY TRUE and are NOT edited: the `RuntimeUInt32`
  (`status_code_filter`) inertness family incl. the test
  `runtime_key_is_rtds_inert` (`crates/envoy-http1/src/hcm.rs:5641`) and the
  CSRF `runtime_key` reject family — this phase wires NEITHER of those
  consumers (§5). A reviewer meeting `runtime_key_is_rtds_inert` inside a
  phase titled "runtime consumer" will read a contradiction unless told: that
  test pins the STATUS-CODE-FILTER consumer, which stays inert by design.

## §2. Why this surface — the cheapest-strong-differential argument

Five properties, each measured:

1. **Zero new dependencies and zero new harness machinery.** The wire type
   (`RuntimeFractionalPercent`), the determinism primitive
   (`selects_deterministic`), the snapshot store, the fixture driver
   (`Http1ProbeList`) and the cluster-free `direct_response` fixture shape all
   exist. No PRNG is needed (§3 D2). Every zero-row family alternative needs
   new machinery: HTTP/3 + QUIC needs `quinn` + an `h3spec` gate, gRPC needs a
   real gRPC data path in the harness, the WASM host is its own multi-phase
   sub-project (`ROADMAP.md:193`).
2. **Backend-free, cluster-free, deterministic.** Every probe cell is a
   static-config value evaluated at the 0/≥100 extremes — no clock, no
   sampling, no backend routing (which is host-RED here; CI-authoritative
   otherwise). Fixture `0088` is fully verifiable on this development host.
3. **It is the mission's named next slice.** Stop-condition leg (ii) names the
   runtime CONSUMER slice explicitly; the `108` family landed producer +
   observer only, and `108`'s own SPEC non-goal 4 (`SPEC.md:713-717`) already
   measured this exact surface working upstream and called it "an attractive
   consumer slice".
4. **One clean seam, compiler-checked.** The gate goes inside `route_matches`
   (or is threaded identically into its two call sites), preserving the
   documented resolve/build_response equivalence; H2 inherits through the
   shared resolver for free.
5. **The strongest competitor does not move the mission legs.** CF-76-1
   (query-strip; re-priced 8-12 probes / ≈900-1100 net LoC at the 108 pick)
   fixes a real divergence but lights up no new config surface and no mission
   leg; it remains banked with its improved record. The zero-row families
   remain correctly rejected as openers on cost (ADR-0171 rejected-alternative
   (b), unchanged). `sni_cluster` still needs the absent `tls_inspector`
   listener-filter subsystem; non-deterministic LB still needs a
   contract-relaxation ADR first.

## §3. Scope — what this phase builds (design decisions D1-D7)

**D1 — the wire surface.** `RouteMatch` gains
`#[serde(default)] pub runtime_fraction: Option<RuntimeFractionalPercent>`,
reusing the existing type at `bootstrap.rs:1504`. Absent field = today's
behaviour (route matches on prefix/path/headers alone). `deny_unknown_fields`
stays. The derived `Deserialize` on `RouteMatch` is edited; `Route`'s
hand-written impls (`bootstrap.rs:2721-2833`) are untouched.

**D2 — evaluation semantics (from §1.1; NO sampling, NO PRNG).** Because
nothing mutates runtime state after boot (§1.2), every route's effective gate
collapses to a process-lifetime constant:

- Resolve the effective value: if `runtime_key` is present AND the key exists
  in the boot `RuntimeSnapshot` AND its `final_value` parses as `u64` → the
  effective numerator is that integer **over HUNDRED regardless of the
  default's denominator** (cell 9). If the key is absent from the snapshot OR
  its `final_value` does not parse → `default_value` (cells 1, 2, 10, 11).
  Lookup reads `final_value` (last-non-empty-wins), which cell 13 proves is
  exactly what upstream's consumer honors.
- Deterministic contract: effective `v == 0` → the route NEVER matches;
  effective `v ≥ 100` (or `default_value` with `numerator ==
  denominator.value()`) → the route ALWAYS matches (cells 3, 4, 12).
- Reject-direction rules (ADR-0049 all-fatal posture, fault/CSRF precedent),
  all validated at boot: (a) `default_value.numerator` must be `0` or
  `== denominator.value()` (the existing `selects_deterministic` discipline;
  upstream also accepts `>` — recorded, slightly-narrower divergence);
  (b) a CONSULTED key whose parsed integer value is strictly between 0 and
  100 is boot-fatal (upstream samples per request — cell 5; banked
  **CF-109-1**); (c) a CONSULTED key whose raw `static_layer` value is a MAP
  is boot-fatal (**CF-109-2**, see D3). New `ConfigError` variants in the
  `UnsupportedNonDeterministic*` family carry each.
- The typed lookup lands in `crates/envoy-config/src/runtime.rs` as the
  store's first read API (e.g. an effective-fraction resolver over
  `entries`/`final_value`), unit-pinned against all 13 §1.1 cells.

**D3 — the CF-108-3 interlock (why a map-shaped consulted value is fatal).**
Upstream honors map-shaped `{numerator, denominator}` values for routing
(cells 7-8) but stores them in `/runtime` as a protobuf text-format dump
(CF-108-3). envoy-rust's store FLATTENS every nested map to dotted keys
(`runtime.rs:163`, `:157-162`), so a map value under key `K` yields entries
`K.numerator`/`K.denominator` and **no entry `K` at all** — a lookup would
silently fall back to `default_value` and mint an unwitnessable divergence.
Boot-fatal reject (validated over the RAW `static_layer` maps, where the
shape is still visible) is the only honest posture short of reworking the
store's flattening, which belongs to a later runtime slice. Fixture `0088`
uses integer and string values only.

**D4 — the threading seam (constraint stated here, choice owned by the PLAN).**
The snapshot (or the precomputed per-route gate) must reach BOTH
`route_matches` call sites (`hcm.rs:2028`, `:2094`) identically, preserving
the documented resolve/build_response equivalence. The census offers two
seams: an `HCMConfig` field (+ `from_config` param; 47 struct literals to
touch, `Arc<Bootstrap>` already in scope at all three production call sites)
or argument-threading through `resolve_route_in`/`build_response_in`. Either
way the RDS reload path must apply the SAME validation under the SAME boot
snapshot: `crates/envoy-config/src/rds.rs:101`
(`reparse_and_select_route_config`) re-runs route validation, and a
runtime-fraction-bearing RDS route config must be checked against the boot
snapshot there too (PLAN-VERIFY V-4).

**D5 — the jwt_authn posture.** Adding the field to `RouteMatch` makes
`runtime_fraction` wire-acceptable inside `jwt_authn.rules[].match`
(`bootstrap.rs:1386`), where the hand-copied matcher
(`jwt_authn.rs:173-186`) would silently ignore it. A present
`runtime_fraction` inside a JWT requirement rule is **boot-fatal** (new
`ConfigError`; recorded reject-direction divergence, banked **CF-109-3**) —
the silent-inert alternative is exactly the divergence class ADR-0049 exists
to prevent, and honoring it there would drag the CF-76-1 second-matcher
surface into scope.

**D6 — the absence-assertion narrowing (the 108-pick lesson).** The phase
edits, in place: the `runtime.rs:10-14` module doc, the
`runtime_stats.rs:15-17` wording, and `BEHAVIOR_CONTRACT.md`'s `## Runtime`
"Nothing READS the runtime store" paragraph (`:3168-3171`) — each narrows to
"the ROUTE gating consumer is live; the `RuntimeUInt32`/CSRF consumers and
RTDS remain unbuilt". It does NOT edit: any landed phase artifact (108's
SPEC "no runtime subsystem" claims are historical, D-3.5), the
`runtime_key_is_rtds_inert` test or any A1-A9/A13-A15 site (§1.2 — all stay
true), fixture `0011`, `0087`, or the `HEADER_ALLOW_LIST`. NOTE: the banked
`108.2` REVIEW M-1 (the measured-false bilateral-405 claim at
`BEHAVIOR_CONTRACT.md:1379`/`:3180-3181`) sits INSIDE the `## Runtime`
section this phase legitimately touches — the state-2/3 sessions must decide
and record whether the M-1 correction rides along (§6.3 permits it now that a
session legitimately touches the surface; it is NOT an obligation of the pick).

**D7 — fixture `0088-runtime-fraction-route-gating`.** Cluster-free,
backend-free, `envoy.yaml` ≡ `envoy-rust.yaml` byte-identical (both sides now
model every construct). One HCM listener; a `layered_runtime` with TWO static
layers (the precedence witness); a route table where each gated route has its
OWN path prefix and its OWN runtime key (the BEHAVIOR_CONTRACT §G
one-path-per-probe attribution rule), each `direct_response`-terminated with
a distinct body; a final bare catch-all route. Probes (≈9, all deterministic,
one per §1.1 equivalence class): default-on (cell 1), default-off (cell 2),
key-0 override (cell 3), key-100 override (cell 4), key-200 ≥denominator
(cell 12), quoted-numeric-string key (cell 6), unparseable-string key →
default (cells 10-11), two-layer last-wins (cell 13), plus the catch-all
itself. `expected_status` + byte-exact `expected_body` per probe. Cells 5
(fractional), 7-8 (map-shaped) are deliberately absent — they are the
boot-fatal classes, witnessed in-process by reject tests instead.

Also in scope: unit + mutation-targeted tests for the lookup API, the three
new validators and the gate evaluation (both call sites); a
`parse_bootstrap` fuzz-corpus seed carrying `runtime_fraction` (with the
explicit `.gitignore` `!` line per the standing trap); the
`BEHAVIOR_CONTRACT.md` `## Runtime` consumer subsection recording §1.1; and
regression-equivalence over all 87 existing fixtures.

## §4. Differential surface at phase end

- NEW fixture `0088-runtime-fraction-route-gating` green cross-proxy
  (`Http1ProbeList`, ≈9 probes, backend-free — locally runnable).
- All 87 pre-existing differential fixtures still green (CI identity
  2180/0 over 164 binaries moves only by this phase's new tests).
- No conformance-suite change (h2spec threshold untouched; route matching
  semantics for non-`runtime_fraction` routes are bit-identical).

## §5. Non-goals — do NOT widen into these

1. **Fractional sampling** (`0 < v < 100`) — needs a PRNG dependency (none in
   tree, `deny.toml` gated) AND a §7.2 contract-relaxation ADR ("values exact
   on deterministic flows"). Boot-fatal here; CF-109-1.
2. **`RuntimeUInt32` (`status_code_filter`) runtime_key honoring** — stays
   RTDS-inert; its test `runtime_key_is_rtds_inert` stays true and untouched.
3. **CSRF `filter_enabled.runtime_key` honoring** — the boot reject stays.
4. **RTDS / `rtds_layer`, `disk_layer`, `admin_layer`, `/runtime_modify`** —
   CF-108-1/CF-108-2 stay banked with the xDS-family/admin slices.
5. **Map-shaped consulted values** — CF-109-2 (D3); requires reworking the
   store's CF-108-3 flattening first.
6. **`runtime_fraction` inside jwt_authn rules** — CF-109-3 (D5).
7. **Hot restart** — the family's other leg, untouched.
8. **Route-level `runtime_fraction` cousins** (e.g. HCM-level or
   per-filter runtime gates, `weighted_clusters.runtime_key_prefix`) — not
   modeled today, stay unknown-field-fatal, unmeasured.

## §6. Carry-forwards

- **OPENS CF-109-1** — fractional runtime_fraction sampling rejected
  boot-fatal (upstream samples per request, cell 5). Unblocks: a PRNG ADR +
  contract-relaxation ADR (shared with the non-deterministic-LB candidate).
- **OPENS CF-109-2** — map-shaped value under a CONSULTED key rejected
  boot-fatal (upstream honors it for routing, cells 7-8); blocked on the
  CF-108-3 flattening rework. References, does not modify, CF-108-3.
- **OPENS CF-109-3** — `runtime_fraction` inside `jwt_authn.rules[].match`
  rejected boot-fatal (upstream honors it there).
- **TOUCHES the surface of banked `108.2` M-1** (D6) — the state-2/3 sessions
  decide whether the contract correction rides along; recorded so the option
  is not silently lost.
- CF-76-1, CF-75-2/3/4/5/6, CF-108-1/2/3, CF-72-2/CF-75-1, M71-6,
  CF-74-1/2/3/4/6, CF-73-1 and the banked Minor/Nit families are unchanged
  by this pick.

## §7. PLAN-VERIFY items — re-confirm FRESH at the state-2 PLAN-write

- **V-1** — re-run the four load-bearing probe cells (1, 3, 9, 13) against
  the pinned image before transcribing them into fixture expectations; the
  full matrix is banked in §1.1 but the fixture's exact YAML must be
  dry-run end-to-end (the 108.2 precedent: the dry-run is a CLAIM state 3
  re-establishes).
- **V-2** — `Http1ProbeList` + `layered_runtime` coexistence: no fixture yet
  combines them (0087 is `admin_scrape`); dry-run the 0088 shape (HCM listener
  + `layered_runtime`) against BOTH proxies before writing the PLAN's
  expectations.
- **V-3** — re-derive the `HCMConfig { … }` literal census (measured 47 at
  this session) and treat the field addition as the workspace-wide `E0063`
  blast the standing trap describes: `-p` runs stay green while the workspace
  breaks; gate on `--workspace --all-targets`.
- **V-4** — the RDS reload path: confirm `reparse_and_select_route_config`
  (`crates/envoy-config/src/rds.rs:101`) can see the boot snapshot (or its
  validation context can), and decide the signature; an RDS-delivered
  `runtime_fraction` must hit the SAME three validators.
- **V-5** — re-derive the `route_matches` call-site census (two production
  sites at `hcm.rs:2028`/`:2094`) and the H2-inheritance claim
  (`envoy-http2/src/hcm.rs:475`); the gate must be observable at both.
- **V-6** — re-derive the §9 size estimate bottom-up and decide the §6.1
  split (ADR-0176 reserved); measure against landed phases, not the SPEC's
  projection (the 76.1 +50% lesson).
- **V-7** — grep-derive the full narrowing list for D6 (the "nothing reads
  the store" family) at PLAN time — line numbers WILL have drifted.
- **V-8** — MEASURE the two unprobed value classes before the fixture is
  frozen: a Bool-valued consulted key (`true`/`false`) and a float-valued
  consulted key (`0.5`, plus a Display-stable `1.5`) against the pinned
  image; D2 provisionally treats both as "does not parse as u64 → default"
  and the PLAN must either confirm by measurement or move them to the
  boot-fatal class. (CF-108-5's closed record governs float SPELLINGS in
  fixtures either way.)
- **V-9** — enumerate every `RouteMatch` construction/consumption site
  (routes + jwt rules today) to confirm D5's reject catches the ONLY other
  consumer; grep, don't trust this SPEC's census.
- **V-10** — confirm fixture `0011` and `0087` need NO edit (both asserted
  here from the census; the PrometheusExposition set-difference argument is
  ADR-0171 DECISION 7's and must be re-read, not re-derived from memory).

## §8. NOT MEASURED — stated explicitly per D-3.4

- Upstream behaviour of `runtime_fraction` under an RDS route-config SWAP
  (the boot snapshot is static here, but upstream re-evaluates per request —
  unobservable difference in this slice's static world; recorded, untested).
- Bool- and float-valued consulted keys (V-8 owns closing this).
- Upstream's exact parse rule for negative integers under a consulted key
  (fixture avoids; validator may fold into the unparseable→default class
  pending V-8's measurement).
- Whether upstream ticks any stat on runtime_fraction evaluation (no stat
  surface is in scope; the nine `runtime.*` stats are startup-set and
  unmoved).
- HCM-level/weighted-cluster runtime keys (§5 item 8).

## §9. Size estimate and the §6.1 split gate

Bottom-up: config field + three validators + typed lookup ≈ 200-300 non-test;
consumer threading (seam + both call sites + H2 pass-through) ≈ 120-200
non-test + 47 one-line test-literal touches; unit/mutation tests ≈ 400-550;
fixture `0088` (two identical YAMLs + expectations + README) ≈ 250-300;
contract section + narrowing edits ≈ 60-90. **Projection ≈ 1100-1450 net
LoC** — under the ~1500 gate on its face, but the calibration precedents cut
both ways (76.1 overran +50%; 108.2 landed ≈905 on a ≈905 pre-flight), so
**the split is PROJECTED POSSIBLE, not projected-to-fire. ADR-0176 is
RESERVED-UNFIRED** for it. The natural cut if it fires: `109.1` = config
surface + validators + typed lookup (in-process witnessed, no fixture, the
foundation-slice precedent); `109.2` = consumer threading + fixture `0088` +
contract + parent close. The state-2 PLAN-write owns the decision (V-6) and
must stop without a `PLAN.md` if it splits (§6.2 step 7).

## §10. Definition of done — the §7.5 gate, instantiated

- (a) Fixture `0088` green cross-proxy on all ≈9 probes.
- (b) All 87 pre-existing fixtures green (CI-authoritative for
  backend-routing ones; local for `0088` itself).
- (c) Conformance unchanged (no new suite; h2spec threshold untouched;
  `known-failures.txt` untouched at 21 lines / ONE real entry).
- (d) The new `parse_bootstrap` corpus seed runs clean in the short-budget
  CI fuzz pass (seed tracked — `git ls-files` is the proof, per the
  `.gitignore` negation trap).
- (e) `cargo build/clippy/fmt/test/deny` clean at workspace scope
  (`--all-targets`; the V-3 blast radius makes `-p` green meaningless).
- (f) `REVIEW.md` APPROVED.

## §11. Next state

This SPEC completes §5 state 0/1 for phase 109 (ROADMAP row added
`in-progress` under `### Runtime + hot restart family`; directory
`docs/envoy-rust/phases/109-runtime-fraction-route-gating/` holds SPEC.md
ONLY — no `PLAN.md` exists yet and none may be written this session). The
NEXT session runs §5 state 2 (`superpowers:writing-plans`), re-confirming
V-1…V-10 fresh — most decisively V-2 (the fixture-shape dry-run) and V-8
(the two unmeasured value classes). ADR-0175 records this pick; ADR-0176 is
reserved for the potential split.
