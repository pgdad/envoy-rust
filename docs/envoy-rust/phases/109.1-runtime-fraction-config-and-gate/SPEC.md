# Sub-phase 109.1 — Runtime CONSUMER slice 1a: the `RouteMatch.runtime_fraction` config surface, the three boot-fatal validators, the store's first typed lookup, and the LIVE gate at both `route_matches` call sites — in-process witnessed, NO new differential fixture

> Split child of phase 109 (`109-runtime-fraction-route-gating`), fired at the
> §5 state-2 PLAN-write per §6.1 (**ADR-0176**; parent pick ADR-0175). Written
> for a reader with ZERO prior context (D-3.4). Every inherited figure here is
> a CLAIM the 109.1 state-2 PLAN-write must re-derive on disk; every upstream
> behaviour cited as MEASURED was probed against the pinned
> `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…70c2`, verified by
> `docker image inspect` before probing) — 13 cells at the phase-109 pick
> (parent `SPEC.md` §1.1) and 10 MORE at the state-2 split session (§1.2
> below, closing parent PLAN-VERIFY V-8).
>
> **The cut line (ADR-0176):** 109.1 lands everything through the WORKING gate
> — config surface, validators, typed lookup, threading, both call sites, H2
> pass-through, RDS-reload parity — witnessed entirely in-process (the 108.1
> foundation-slice precedent). Sibling 109.2 lands differential fixture `0088`,
> the `BEHAVIOR_CONTRACT.md` `## Runtime` consumer subsection, the decided-in
> M-1 correction, and the parent close. This cut deviates from ADR-0175's
> sketch (threading in the second slice) DELIBERATELY: the sketched cut would
> have left a landed state where a `runtime_fraction`-bearing config parses,
> validates, and is then SILENTLY IGNORED by routing — exactly the divergence
> class parent SPEC §3 D5 / ADR-0049 exists to prevent. At the end of 109.1
> the gate WORKS or the config is boot-rejected; there is no silent middle.

## §1. The measured upstream contract

### §1.1 The 13-cell pick matrix (parent SPEC §1.1, re-confirmed at the split)

Probe harness: one HCM listener; route 1 = `prefix: "/"` +
`runtime_fraction { default_value { numerator: N, denominator: D }, runtime_key: "probe.frac" }`
→ `direct_response` 200 body `GATED`; route 2 = bare `prefix: "/"` →
`direct_response` 200 body `FALLBACK`. The four load-bearing cells were
RE-RUN FRESH at the split session (parent V-1), 40/40 each:

| cell | default_value | static `probe.frac` | result |
|---|---|---|---|
| 1 | 100/HUNDRED | (no `layered_runtime`) | GATED 40/40 — absent key → `default_value`, deterministic |
| 3 | 100/HUNDRED | `0` (int) | FALLBACK 40/40 — key overrides default |
| 9 | 0/**MILLION** | `100` (int) | GATED 40/40 — an integer value is numerator over **HUNDRED**, NOT over the default's denominator |
| 13 | 100/HUNDRED | two layers: base `100`, override `0` | FALLBACK 40/40 — consumer honors last-layer-wins `final_value` |

The other nine pick cells (2, 4-8, 10-12) are banked in the parent SPEC §1.1:
default-0 honored on absent key; `100`/`200` always match; `50` is per-request
NONDETERMINISTIC (27/33 over 60); quoted numeric strings parse like integers;
map-shaped `{numerator, denominator}` values are HONORED for routing upstream;
`"abc"` falls back to `default_value` in BOTH directions.

### §1.2 The 10-cell V-8 closure matrix (MEASURED at the state-2 split session)

Same harness, 40 probes per cell. These cells close parent PLAN-VERIFY V-8
(bool/float consulted keys were UNMEASURED at the pick) and they REFUTE the
parent's provisional D2 rule for floats:

| cell | default_value | static `probe.frac` | result | reading |
|---|---|---|---|---|
| B1 | 100/HUNDRED | `true` (bool) | GATED 40/40 | bool → `default_value` |
| B2 | 0/HUNDRED | `true` (bool) | FALLBACK 40/40 | bool → default, BOTH directions |
| B3 | 100/HUNDRED | `false` (bool) | GATED 40/40 | `false` is NOT parsed as 0 — default used |
| F1 | 100/HUNDRED | `0.0` (float) | FALLBACK 40/40 | **float PARSES as 0 — NOT default** |
| F2 | 0/HUNDRED | `100.0` (float) | GATED 40/40 | float parses as 100 |
| F3 | 100/HUNDRED | `0.5` (float) | FALLBACK 40/40 | parsed (not default); 0.5% sampling and truncate-to-0 are indistinguishable at n=40 — either way NOT default |
| F4 | 0/HUNDRED | `1.5` (float) | GATED **1**/40 | **non-integral floats are per-request NONDETERMINISTIC** (a single GATED under a 0-default proves parse + sampling) |
| N1 | 100/HUNDRED | `-7` (int) | GATED 40/40 | negative → `default_value` |
| N2 | 0/HUNDRED | `-7` (int) | FALLBACK 40/40 | negative → default, BOTH directions |
| S1 | 100/HUNDRED | `"0.5"` (quoted string) | FALLBACK 40/40 | numeric STRINGS parse like their float counterparts — NOT default |

**The refutation that matters:** parent SPEC §3 D2 provisionally treated
"does not parse as u64" as → `default_value`. MEASURED FALSE for floats and
float-shaped strings: upstream PARSES them (F1-F4, S1). Two mitigating facts
make the implementation clean anyway:

1. envoy-rust's store stringifies YAML floats through `f64` Display
   (`RuntimeValue::stringify`, `crates/envoy-config/src/bootstrap.rs` — landed
   108.1), so YAML `0.0`/`100.0` arrive in `final_value` as `"0"`/`"100"` and
   the numeric path handles them identically to upstream (F1/F2 agree).
2. Only NON-INTEGRAL spellings survive Display as non-integers (`"0.5"`,
   `"1.5"`) — and those are the per-request-nondeterministic class (F4), which
   is boot-fatal here under CF-109-1 exactly like integer `50`.

### §1.3 The evaluation cascade (D2, updated — unit-pin EVERY row)

For a route whose `runtime_fraction.runtime_key` resolves to snapshot entry
with `final_value` string `S` (see D3 for when the key is treated as absent),
the effective gate is decided ONCE per lookup, process-lifetime-constant:

1. Parse `S` as `f64`. If it parses AND is finite:
   - `v == 0.0` → the route NEVER matches (cells 3, 6, F1);
   - `v >= 100.0` → the `runtime_fraction` gate ALWAYS passes; prefix/path/
     headers matching applies unchanged (cells 4, 9, 12, F2);
   - `0 < v < 100` → **boot-fatal** (`CF-109-1`; upstream samples per request
     — cells 5, F3, F4, S1);
   - `v < 0` → use `default_value` (cells N1, N2 — MEASURED, both directions).
2. Otherwise (bools `"true"`/`"false"`, non-numeric strings, empty string,
   non-finite spellings) → use `default_value` (cells 10, 11, B1-B3).
3. `default_value` itself must satisfy the existing
   `FractionalPercent::selects_deterministic` discipline
   (`bootstrap.rs:1321-1323`): numerator `0` → never; numerator
   `== denominator.value()` → always; anything else boot-fatal (upstream also
   accepts `>` — the recorded slightly-narrower divergence, parent D2(a)).

A single `f64` parse covers integers exactly (every u64 below 2^53 is exact,
and any value ≥ 100 is "always" regardless of precision). NOT measured and
excluded from fixtures (record in the PLAN, avoid in yaml): `"1e6"`-style
exponent spellings (would gate Always here), `"NaN"`/`"inf"` (non-finite →
default here), `"-0.0"` (== 0.0 in IEEE → Never here).

## §2. The tree census (re-derived at the split session — re-derive again at PLAN time)

- **`RouteMatch`** — `crates/envoy-config/src/bootstrap.rs:2895-2904`:
  `deny_unknown_fields`, exactly `prefix`/`path`/`headers`. Adding the field
  is a workspace-wide `E0063` blast: **101 `RouteMatch { … }` struct-literal
  sites** (57 `crates/envoy-http1/src/hcm.rs`, 36 `crates/envoy-http2/src/hcm.rs`,
  3 `crates/envoy-filter/src/jwt_authn.rs`, 3 `crates/envoy-config/src/bootstrap.rs`,
  1 `crates/envoy-filter/src/instance.rs`, 1 `crates/envoy-filter/src/types.rs`),
  ZERO of them using `..Default::default()` and no `Default` impl exists. The
  parent SPEC §9 priced only the 47 `HCMConfig` sites — this second blast is
  ADR-0176's first split ground. `-p` runs stay green while the workspace
  breaks: gate on `--workspace --all-targets`.
- **`HCMConfig`** — `crates/envoy-http1/src/hcm.rs:124-177`; **51 raw
  `HCMConfig { … }` grep hits** (45 `envoy-http1/src/hcm.rs`, 2
  `envoy-http1/src/rds_watcher.rs`, 4 `envoy-http2/src/hcm.rs` — the H2 hits
  are the H2 wrapper type whose `inner: Arc<envoy_http1::HCMConfig>`; the PLAN
  disambiguates which of the 4 construct the H1 type).
- **`route_matches`** — `crates/envoy-http1/src/hcm.rs:2182-2193`, private
  free fn over `(r: &Route, path, headers)`; exactly TWO production call
  sites: `hcm.rs:2028` (`resolve_route_in`) and `hcm.rs:2094`
  (`build_response_in`), documented as required-identical at `hcm.rs:1994-1996`
  ("the 30-fixture regression-equivalence guarantee"). Test call sites at
  `hcm.rs:10342-10368` share the signature and move with it.
- **H2 inherits via the shared resolver**: `crates/envoy-http2/src/hcm.rs:475`
  calls `envoy_http1::hcm::resolve_route(&config.inner, &envoy_req)` — keep
  `resolve_route`'s public `(config, req)` signature and H2 needs ZERO edits.
- **The snapshot store** — `crates/envoy-config/src/runtime.rs`:
  `RuntimeSnapshot { layer_names, entries: BTreeMap<String, RuntimeEntry> }`,
  `RuntimeEntry { layer_values, final_value }`, constructors `from_layers` /
  `from_bootstrap`. NO read API yet (`runtime.rs:10-14`: "Nothing reads this
  store yet" — 109.1 falsifies and narrows this doc). `RuntimeSnapshot`
  derives `Default` (empty = every lookup → `default_value`).
- **THREE route-validation paths, all of which must apply the SAME validators
  under the SAME boot snapshot** (parent V-4, widened at the split session):
  1. boot inline: `validate_hcm` (`crates/envoy-config/src/bootstrap.rs:4223`)
     walks `for r in &mut vh.routes` (`:~4306`) calling
     `validate_route_match_cardinality` — the new validators hook there;
  2. post-merge xDS: `load_dynamic_resources`
     (`crates/envoy-config/src/lib.rs:1119`, takes `&mut Bootstrap`)
     re-validates merged route tables (defer-then-revalidate);
  3. RDS hot reload: `reparse_and_select_route_config`
     (`crates/envoy-config/src/rds.rs:101`) — gains a snapshot (or
     equivalent) parameter; its caller `rds_watcher.rs:184` has
     `store: Arc<HCMConfig>` in scope, which carries the snapshot after D4.
- **⚠ THE RELOAD-CLASSIFIER ABORT TRAP (measured precedent, 76.2 REVIEW I-1):**
  `rds_watcher.rs:205-240` matches the SIX `ConfigError` variants
  `reparse_and_select_route_config` can return and `unreachable!()`s on any
  other — `panic = "abort"` in release: **the whole proxy dies on a hot reload
  of a rejected config.** Every NEW variant the three validators can return
  through `reparse` MUST be added to that classifier's `update_rejected` arm,
  and the compiler will NOT flag the omission (`unreachable!()` compiles
  clean). Grep the callers; write the classifier test FIRST.
- **jwt reuse** — `RequirementRule.r#match: RouteMatch`
  (`bootstrap.rs:1386`), matched by the hand-copied `route_match_matches`
  (`crates/envoy-filter/src/jwt_authn.rs:173-186`); jwt validation lives in
  `validate_jwt_authn_config` (`bootstrap.rs:4765`).
- **The CSRF precedent for both reuse and reject**:
  `RuntimeFractionalPercent` (`bootstrap.rs:1504-1508`) is the wire type,
  today used only by `CsrfPolicy.filter_enabled` whose present `runtime_key`
  is boot-rejected (`validate_csrf_config`, `bootstrap.rs:4863-4874`) — that
  reject is UNTOUCHED.

## §3. Scope — design decisions D1-D8

**D1 — wire field.** `RouteMatch` gains
`#[serde(default)] pub runtime_fraction: Option<RuntimeFractionalPercent>`
(reusing `bootstrap.rs:1504`; `deny_unknown_fields` stays; `Route`'s
hand-written impls untouched). All ~101 struct-literal sites gain
`runtime_fraction: None,` (one line each; mechanical).

**D2 — evaluation semantics.** The §1.3 cascade, exactly. The typed lookup
lands in `crates/envoy-config/src/runtime.rs` as the store's FIRST read API —
a resolver over `entries`/`final_value` returning the deterministic gate (or
the validation error), unit-pinned against EVERY §1.1 + §1.2 cell.

**D3 — the CF-109-2 map-shape reject, implemented as the SNAPSHOT-PREFIX
rule.** Upstream honors map-shaped `{numerator, denominator}` consulted
values (pick cells 7-8) but envoy-rust's store FLATTENS maps to dotted keys
(`runtime.rs:163`): a map at consulted key `K` yields entries
`K.numerator`/`K.denominator` and NO entry `K` — a naive lookup would
silently fall back to `default_value`. The reject that closes this WITHOUT
raw-YAML re-walking: **a consulted key `K` is boot-fatal iff any snapshot
entry name starts with `K.`** (string prefix). Analysis, to be restated in
the PLAN: a map at `K` in ANY layer produces `K.`-prefixed entries → caught;
`K` scalar in a later layer + map in an earlier one ALSO leaves `K.`-prefixed
entries → conservatively caught (upstream last-wins would honor the scalar —
a recorded, slightly-conservative reject-direction divergence inside
CF-109-2); a literal dotted SIBLING key (`K.foo` beside scalar `K`) →
conservatively caught, same recording. Consulted key ABSENT entirely (no
entry, no prefix) → `default_value` (cells 1, 2) — NOT fatal.

**D4 — the threading seam, DECIDED: an `HCMConfig` field.** `HCMConfig` gains
`pub runtime: Arc<envoy_config::runtime::RuntimeSnapshot>`;
`HCMConfig::from_config` (`hcm.rs:180-185`) gains the parameter (its three
production call sites in `crates/envoy-bin/src/main.rs` have `Arc<Bootstrap>`
in scope — build the snapshot ONCE via `RuntimeSnapshot::from_bootstrap` and
clone the `Arc`). `resolve_route_in` / `build_response_in` /`route_matches`
gain a `runtime: &RuntimeSnapshot` parameter; the public `resolve_route` /
`build_response` wrappers pass `&config.runtime` (signatures UNCHANGED, so
H2 and every wrapper caller need zero edits); the H1 keep-alive loop's direct
`_in` call sites pass `&config.runtime` alongside their existing snapshot
threading. ~51 `HCMConfig` literal sites gain
`runtime: Arc::new(RuntimeSnapshot::default()),` (or a shared test helper —
PLAN decides the spelling once).

**D5 — jwt posture (CF-109-3).** A present `runtime_fraction` inside
`jwt_authn.rules[].match` is boot-fatal (new `ConfigError` variant, checked in
`validate_jwt_authn_config`); the hand-copied jwt matcher is NOT edited.

**D6 — validator wiring at ALL THREE paths** (§2): boot inline
(`validate_hcm`), post-merge (`load_dynamic_resources`), RDS reload
(`reparse_and_select_route_config` + the rds_watcher classifier extension per
the §2 abort trap). The new `ConfigError` variants (nondeterministic-fraction
CF-109-1, map-shaped-consulted-key CF-109-2, jwt-rule CF-109-3, plus the
non-deterministic `default_value` reject if not already covered by an
existing variant) follow the `UnsupportedNonDeterministic*` naming family.

**D7 — absence-assertion narrowing (the slice that falsifies them fixes
them).** In 109.1, in place: the `runtime.rs:10-14` module doc, the
`crates/envoy-bin/src/runtime_stats.rs` consumer-absence wording, and the
ONE `BEHAVIOR_CONTRACT.md` sentence at `:3168-3171` ("Nothing READS the
runtime store for behavior yet") — each narrows to "the ROUTE
`runtime_fraction` consumer is live (109.1); the `RuntimeUInt32`/CSRF
consumers and RTDS remain unbuilt". The full `## Runtime` consumer
SUBSECTION (probe matrix, fixture pointers) is 109.2's, as is the banked M-1
correction. NOT edited: `runtime_key_is_rtds_inert`
(`crates/envoy-http1/src/hcm.rs:5641` — pins the STATUS-CODE-FILTER
consumer, which stays inert by design), the CSRF reject family, fixtures
`0011`/`0087`, `HEADER_ALLOW_LIST`, any landed phase artifact (D-3.5).

**D8 — fuzz seed.** One new `parse_bootstrap` corpus seed carrying
`runtime_fraction` (plus `layered_runtime`), with the explicit `.gitignore`
`!` line per the standing negation trap; `git ls-files` is the tracking
proof. No new fuzz target, no `ci.yml` change.

Also in scope: unit + mutation-targeted tests for the lookup cascade (every
§1.1/§1.2 cell), the three validators at all three paths, the reload
classifier, and the gate observable at BOTH call sites (resolve and
build_response, plus an H2-inheritance witness through the shared resolver).

## §4. Non-goals

1. Fixture `0088`, `expectations.yaml`, README — sibling **109.2**.
2. The `BEHAVIOR_CONTRACT.md` `## Runtime` consumer subsection and the
   decided-in M-1 correction — sibling **109.2**.
3. Fractional sampling (CF-109-1), map-value honoring (CF-109-2), jwt-rule
   honoring (CF-109-3), `RuntimeUInt32`/CSRF consumers, RTDS/disk/admin
   layers (CF-108-1/2), hot restart, HCM-level/weighted-cluster runtime keys
   — all parent §5 non-goals, unchanged.

## §5. Differential surface at sub-phase end

- NO new fixture (the 108.1 foundation-slice precedent). All 87 pre-existing
  differential fixtures still green; CI identity `2180/0` over 164 binaries
  moves ONLY by this slice's new unit/mutation tests.
- The gate is witnessed IN-PROCESS: lookup-cascade tests over every measured
  cell, gating tests at both call sites, validator reject tests, reload
  classifier tests.

## §6. PLAN-VERIFY items for the 109.1 state-2 session

- **W-1** — re-derive the 101-site `RouteMatch` and 51-site `HCMConfig`
  literal censuses (they drift); disambiguate the 4 `envoy-http2` `HCMConfig`
  hits (H2 wrapper vs inner H1 constructions).
- **W-2** — re-read `rds_watcher.rs:205-240` and enumerate the classifier's
  current variant set; the classifier test is written BEFORE the validators
  widen `reparse`'s returnable set.
- **W-3** — confirm `RuntimeSnapshot::from_bootstrap` is infallible
  post-validation and cheap enough to build once per proxy boot (it is
  rebuilt per-request by admin `/runtime` today — `endpoint.rs:982`).
- **W-4** — decide the lookup API's exact signature + error type against the
  §1.3 cascade before transcribing tests.
- **W-5** — confirm the three `envoy-bin/src/main.rs` `from_config` call
  sites and their `Arc<Bootstrap>` scope; grep for any fourth caller.

## §7. NOT MEASURED (excluded from all yaml; record, don't guess)

Exponent spellings (`1e6`) under a consulted key; `"NaN"`/`"inf"`/`"-0.0"`;
empty-string consulted value (`""` — treated as absent-like by
`final_value`'s last-NON-EMPTY rule, hence → default here; upstream
unmeasured); upstream behaviour under an RDS route-config swap (parent §8);
negative FLOATS (`-0.5` — folds into cascade rule `v < 0` → default here;
upstream unmeasured).

## §8. Size estimate

Non-test ≈ 290-440 (field + validators + wiring 125-185, lookup 60-90,
threading 50-70, narrowing 15, seed 5, classifier 10-15); mechanical literal
touches ≈ 152; tests ≈ 350-550. **≈ 800-1140 net LoC, under the ~1500 gate**
(the parent's un-split total, re-derived bottom-up at the split session with
the previously-unpriced RouteMatch blast, was ≈ 1150-1550 BEFORE the measured
+50% test-half overrun calibration — ADR-0176's firing ground).

## §9. Next state

This SPEC is the §6.2 step-3 redistribution output. The NEXT session runs
§5 state-2 for **109.1** (`superpowers:writing-plans`), re-confirming
§6 W-1…W-5 fresh. States 3-6 follow per the state machine; sibling 109.2
starts only after 109.1's state-6 close.
