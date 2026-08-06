# Sub-phase 108.1 — `layered_runtime` config surface + the runtime snapshot store

> **Created by the §6.1 SPLIT of phase 108, executed at the §5 state-2 PLAN-write
> on 2026-08-05 and recorded in ADR-0172.** The parent SPEC is
> `docs/envoy-rust/phases/108-runtime-static-layer/SPEC.md` (935 lines,
> findings R-1…R-18) and remains the authoritative record of the state-0/1
> recon. This document is self-contained for 108.1's slice (doctrine D-3.4) and
> carries the NEW measurements taken at the PLAN-write, which materially
> correct the parent in five places.

## §0. What this sub-phase is, in one paragraph

envoy-rust today has **no runtime subsystem at all**: a config carrying
`layered_runtime:` is rejected at boot, there is no `envoy-runtime` crate, and
`git grep` finds zero hits for `layered_runtime`, `static_layer`,
`LayeredRuntime` or `runtime_fraction` across the tracked tree. This sub-phase
builds the **producer half**: the config schema for `layered_runtime` with a
`static_layer` arm, its reject-direction validators, and the in-memory snapshot
store that turns parsed layers into the flattened, stringified key/value view
upstream Envoy exposes. It is witnessed **entirely in-process**. It adds **no
differential fixture** and **no admin endpoint** — those are sibling `108.2`.

This is the same foundation-slice shape as `05.1`, `07.1`, `12.1`, `14.1`,
`23.1`, `25.1` and `76.1`: land the config surface and the data structure with
unit-level witnesses and full regression-equivalence, then let the sibling
observe it differentially.

## §1. Scope — what 108.1 builds

### D1 — `layered_runtime` config schema (`crates/envoy-config`)

- `Bootstrap.layered_runtime: Option<LayeredRuntime>`. **The `Option` is
  load-bearing** — see N-8: absent and present-but-empty are NOT equivalent
  upstream, and an `Option` model that treats `Some(empty)` and `None` alike
  mints a divergence.
- `LayeredRuntime { #[serde(default)] layers: Vec<RuntimeLayer> }`.
- `RuntimeLayer` with `#[serde(deny_unknown_fields)]`, carrying `name: String`
  and the four oneof arms modelled so that exactly one may be set. Only
  `static_layer` is implemented; `disk_layer`, `rtds_layer` and `admin_layer`
  are **parsed and then loudly rejected** (D2).
- A recursive scalar-or-map value type for static-layer values
  (bool / integer / float / string / nested map). **This type is new to the
  codebase — nothing in `bootstrap.rs` currently models a recursive
  YAML value — and it is the single largest piece of D1.**
- `Serialize` arms, to keep the `/config_dump` cascade whole.

### D2 — validators + `ConfigError` variants (`crates/envoy-config`)

Reject-direction parity for the rules measured against the pinned image
(parent R-3, re-confirmed at N-12 below):

1. empty or absent layer `name` (upstream: PGV `min_len 1`);
2. **duplicate** layer `name` (upstream: the bare string `Duplicate layer name: <n>`,
   raised at a POST-PGV stage);
3. no oneof arm set (upstream: `field: "layer_specifier", reason: is required`);
4. more than one oneof arm set.

Plus the **fail-loud rejection** of the three out-of-scope arms `disk_layer`,
`rtds_layer` and `admin_layer`, per the ADR-0049 all-fatal posture. This is a
deliberate, **recorded reject-direction divergence** — upstream accepts all
three and envoy-rust will not — banked as **CF-108-1**, not vaguely deferred
(§6.3 anti-pattern). It is the same disposition ADR-0168 DECISION 3 took for
`regex_rewrite` inside `redirect`.

Error **text** is not part of the equivalence contract (§7.2) — only the verdict.

### D3 — the runtime snapshot store

A small, self-contained store turning parsed layers into upstream's observable
shape. Every rule below is MEASURED at §2:

- **flatten nested maps to dotted keys, at arbitrary depth** (N-4);
- **stringify every scalar** (N-3, and mind the float rule);
- **one `layer_values` slot per configured layer**, `""` where the key is absent
  from that layer (N-6);
- **`final_value` = the last NON-EMPTY slot** — an explicitly-set empty string
  does not override a lower layer (N-7).

**Home — the default is a module inside `envoy-config`, and V-3 is hereby
DECIDED: no new crate.** Measured: the workspace has **14** crates under
`crates/` (22 workspace members including test/helper crates) and no
`envoy-runtime`. `envoy-config` is already a dependency of both `envoy-http1`
(the eventual `runtime_fraction` consumer) and `envoy-admin` (the `108.2`
`/runtime` renderer), so hosting the store there adds **no new dependency edge
and creates no cycle**. A new leaf crate would need edges into config, http1,
admin and bin for no measured benefit, and would have to respect the ADR-0150
seam discipline. Recorded so `108.2` can depend on it without re-litigating.

### D7 (config-side) — in-process backstops

The absent-vs-empty distinction (N-8), the four reject-direction rules (D2), the
three loudly-rejected arms, arbitrary-depth flattening, every stringification
rule including the float and YAML-1.1 rules, last-non-empty-wins precedence, and
the `""`-slot rule. **The two-static-layer construction (N-6) makes multi-layer
precedence testable inside this sub-phase** — the parent SPEC could only measure
it via the out-of-scope `admin_layer`.

### D8 — fuzz disposition (§7.4)

**No new fuzz target.** `layered_runtime` is parsed by the existing
`parse_bootstrap` target, which already covers the whole `Bootstrap` surface, so
gate (d) is satisfied by a pre-existing target — the phase-66/67/76 disposition.
A corpus seed is added instead, under
`crates/envoy-config/fuzz/corpus/parse_bootstrap/`. **It needs an explicit
`!`-un-ignore line in `crates/envoy-config/fuzz/.gitignore` or it is silently
untracked and invisible to CI**; the state-3 session must prove it tracked with
`git ls-files`. **`ci.yml` needs no new step**, and the state-4 session must
RECORD that explicitly rather than skip gate (d) silently.

## §2. NEW measurements taken at the state-2 PLAN-write

All driven against the pinned image `envoyproxy/envoy:v1.33.0`, digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`
(`docs/envoy-rust/ENVOY_TARGET.md`), port-mapped with `docker run -p` and probed
with `curl`. Each corrects or extends the parent SPEC.

### N-1 — V-2 CONFIRMED BY EXPERIMENT (the load-bearing claim of the whole pick)

`GET /runtime` is byte-nondeterministic: **8 consecutive GETs against one
unchanged process produced 8 DISTINCT md5 sums**, every response exactly 1126
bytes — a pure key/field-order shuffle (stronger than the parent's 5-of-6).

The parent asserted this is neutralized because `serde_json::Map` is a
`BTreeMap`. **That was re-confirmed by experiment, not by reading**, using the
workspace's exact pinned `serde_json 1.0.149`:

```
raw bytes identical? false
to_string identical?  true
subtree  entries: to_string identical? true
subtree   layers: to_string identical? true
```

— for both shuffled-vs-shuffled and shuffled-vs-deliberately-key-sorted inputs.
Mechanical corroboration: `git grep 'preserve_order'` over `*.toml`/`*.lock`/`*.rs`
returns **zero** hits, and `serde_json`'s own dependency block in `Cargo.lock`
lists `itoa, memchr, serde, serde_core, zmij` — **no `indexmap`**, which is the
crate `preserve_order` would pull in. **The fixture design in sibling 108.2
HOLDS.**

### N-2 — DECISIVE for D1's value model: upstream's YAML is **YAML 1.1**, `serde_yaml` is not

An error dump from `--mode validate` exposed Envoy's YAML→JSON conversion
verbatim. Measured, on the same file:

| YAML written | upstream JSON | `/runtime` `final_value` |
|---|---|---|
| `key: y` | `true` | `"true"` |
| `key: n` | `false` | `"false"` |
| `key: on` | `true` | `"true"` |
| `key: off` | `false` | `"false"` |
| `key: "y"` | `"y"` | `"y"` |

**`serde_yaml` implements the YAML 1.2 core schema, where unquoted `y` is the
STRING `"y"`.** So the identical fixture file would yield `"true"` on upstream
and `"y"` on envoy-rust. **This is a live divergence that D1 must decide
explicitly** — either normalise at parse time to match YAML 1.1, or document it
as a recorded divergence and keep such values out of fixtures. **The state-3
session MUST NOT leave this implicit.** The parent SPEC flagged it as a hazard
to probe (V-4); it is now measured and it is real.

### N-3 — floats stringify at the YAML→JSON boundary; integers do not

From the same dump: `my.numeric.key: 42` → JSON `42`, `my.negative.key: -7` →
JSON `-7`, but **`my.float.key: 1.5` → JSON `"1.5"`, a STRING**. All three land
in `/runtime` as `"42"`, `"-7"`, `"1.5"`, so the snapshot output is uniform — but
the value model must not assume floats arrive as JSON numbers.

### N-4 — flattening is ARBITRARY-DEPTH, not one level

The parent measured one level. Measured here at two:

```yaml
my.nested:
  sub_key: v
  deeper:
    leaf: w
```

yields entries `my.nested.sub_key` and **`my.nested.deeper.leaf`**. No
intermediate `my.nested` or `my.nested.deeper` entry appears. D3 must recurse,
not special-case a single level.

### N-5 — `runtime.num_keys` counts FLATTENED LEAVES

A layer declaring **11** top-level YAML keys (one of them a nested map holding
two leaves) yields `runtime.num_keys: 12` — the count of flattened scalar
entries, matching the `entries` object size exactly. `runtime.num_layers` counts
configured layers.

### N-6 — TWO STATIC LAYERS ARE LEGAL, so multi-layer precedence is IN SCOPE

The parent measured `layer_values` slots only with an `admin_layer`, which is
out of scope — leaving the impression that precedence could not be witnessed
this phase. **It can.** Two `static_layer` entries with distinct names are
accepted, and produce:

```json
"shared.key":        { "layer_values": ["from_base", "from_override"], "final_value": "from_override" }
"only.in.base":      { "layer_values": ["base_val", ""],               "final_value": "base_val"      }
"only.in.override":  { "layer_values": ["", "over_val"],               "final_value": "over_val"      }
```
with `"layers": ["base_layer","override_layer"]`, `num_layers: 2`, `num_keys: 4`.
Slot order follows config order.

### N-7 — `final_value` is the last **NON-EMPTY** slot, and `""` collides with "absent"

In the same two-layer probe, `empty.in.override` was set to `real_value` in the
base layer and to the empty string `""` in the override layer:

```json
"empty.in.override": { "layer_values": ["real_value", ""], "final_value": "real_value" }
```

So an **explicitly-set empty string does NOT override** a lower layer, and it is
**indistinguishable in the wire format from the key being absent from that
layer** — both render `""`. D3 must implement "last non-empty wins", not "last
wins". A single-layer probe confirms an empty value is still a legitimate entry:
`my.empty.string.key: ""` yields `{"final_value": "", "layer_values": [""]}` and
is counted in `num_keys`.

### N-8 — absent vs empty, reproduced exactly

| config | `/runtime` | `num_layers` | `num_keys` |
|---|---|---:|---:|
| no `layered_runtime` block | `{"entries":{},"layers":[]}` | 0 | 0 |
| `layered_runtime: {}` | `{"entries":{},"layers":[""]}` | 1 | 0 |
| `layered_runtime: { layers: [] }` | `{"entries":{},"layers":[""]}` | 1 | 0 |

Upstream synthesizes ONE layer named the **empty string** for both empty
spellings. Parent R-5 confirmed verbatim.

### N-9 — the tree holds **ELEVEN** "no runtime subsystem" assertions, not four

Parent R-17 named four and V-10 asked for their disposition. A full census of
**shipping code and fixtures** (excluding `docs/` and phase PLAN/SPEC copies)
finds **eleven** distinct sites. The four named by the parent:

1. `crates/envoy-http1/src/hcm.rs` — the test `runtime_key_is_rtds_inert`
   (doc 5632-5639, `fn` at 5641, closes 5686).
2. `crates/envoy-config/src/lib.rs:760-765` — `ConfigError::UnsupportedRuntimeKeyedCsrfFilterEnabled`.
3. `crates/envoy-config/src/bootstrap.rs:4657-4678` — `validate_csrf_config`'s
   `if fe.runtime_key.is_some()` reject (doc 4651-4656).
4. `crates/envoy-config/src/bootstrap.rs:843-846` — `RuntimeUInt32`'s doc contract.

The **seven the parent missed**:

5. `crates/envoy-accesslog/src/filter.rs:18` — the compiled-side mirror of (4).
6. `crates/envoy-http1/src/hcm.rs:1773-1774` — the **production** compile path's
   RTDS-inert comment (distinct from the test at (1)).
7. `crates/envoy-config/src/lib.rs:469-474` — `ConfigError::EmptyStatusCodeFilterRuntimeKey`'s doc.
8. `crates/envoy-config/src/lib.rs:752-758` — `ConfigError::UnsupportedNonDeterministicCsrfFilterEnabled`'s doc.
9. `crates/envoy-config/src/bootstrap.rs:1368-1372` — `RuntimeFractionalPercent`'s doc.
10. `crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml:19-20` — a corpus-seed comment.
11. `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml:35` **and**
    `README.md:55` — the nine `runtime.*` stats filed as an xDS-family deferral.

**V-10 DISPOSITION, DECIDED HERE.** Ten of the eleven narrow correctly from
"no runtime subsystem" to "**no runtime CONSUMER for this key**" and **none
becomes false when 108.1 lands**, because 108.1 builds a `static_layer` store
that nothing reads — it is not RTDS, and it wires neither the `RuntimeUInt32`
nor the `RuntimeFractionalPercent` consumer (§3). **108.1 EDITS NONE OF THEM.**
Site (11) is the one that becomes *semantically* wrong, and it becomes wrong
only when the **stats** land — which is sibling **108.2**, not 108.1. It is
therefore 108.2's to dispose of, and is recorded in that SPEC.

`runtime_key_is_rtds_inert` **keeps its name and its wording.** Its own doc
comment already scopes the claim to the comparison always using `default_value`,
which stays exactly true. The state-5 reviewer is directed here rather than left
to read a contradiction.

### N-10 — a standing-ledger claim CORRECTED: `cluster: y` is boot-fatal, and no fixture uses it

The standing traps ledger records *"an unquoted `cluster: y` under `node:` parses
as boolean `true` — every fixture writes it exactly as `y` and it is fine there;
do not 'improve' it."* **The second clause is false in both halves.** Measured:
`node.cluster: y` booleanizes to `true`, and because `node.cluster` is a
protobuf **string** field, upstream rejects the config outright:

```
error initializing configuration: Unable to parse JSON as proto
  (INVALID_ARGUMENT: invalid JSON in envoy.config.bootstrap.v3.Bootstrap
   @ node.cluster: string … unexpected character: 't'; expected '"')
```

And `git grep -c 'cluster: y$' -- 'tests/fixtures/*/envoy.yaml'` matches **zero
files** — every fixture uses a descriptive cluster name such as
`envoy-rust-phase-06.1`. The YAML-1.1 hazard is real (N-2) but it has never
appeared in `node.cluster`. Recorded so the next session does not inherit a
false reassurance.

### N-11 — re-derived censuses (all measured this session)

**86** fixture directories under `tests/fixtures/` (highest `0086`), **86**
differential test files under `tests/differential/tests/`, **5** fuzz targets
spanning **five** crates, a **3**-entry `HEADER_ALLOW_LIST`, **14** crates under
`crates/` with **no `envoy-runtime`**, **112** phase directories, **125**
`ConfigError` variants over the enum span `crates/envoy-config/src/lib.rs:74-1011`
(confirmed twice — by variant lines and by `#[error(...)]` count), ADR head
**ADR-0171** / next free **ADR-0172** (derived from the max, never the count:
`grep -c '^## ADR-'` returns **168** because it also counts the template at
line 10, and the numbers are non-contiguous — **0082, 0116, 0117, 0119** are
missing).

Two ledger figures corrected:

- **`known-failures.txt` is 21 LINES but holds exactly ONE real entry** (`3.5/2`);
  lines 1-19 are a header comment and line 20 is blank. "21" is a line count,
  not a failure count. Never trim it (this host scores `3.5/2` as PASS where CI
  does not).
- **The ROADMAP rows carrying unescaped pipes are NINE, not seven** — the
  standing list `36/38/39/52/54/66/70` omits **`76`** and **`108`**. Harmless to
  a status census because every extra pipe sits in the `summary` column, which
  is after field 4; `awk -F' | '` still yields a clean status for all nine. Do
  not "fix" any of them — `ROADMAP.md` is append-only.

### N-12 — the layer grammar, re-confirmed

Parent R-3's reject-direction table was spot-checked and holds: `name` is
PGV-required with `min_len 1`; exactly one oneof arm is required
(`field: "layer_specifier", reason: is required`); two arms are rejected
(`'admin_layer' has already been set … as part of a oneof`); duplicate names are
rejected post-PGV with the bare string `Duplicate layer name: <n>`; an unknown
arm gives `no such field`. Note `--mode validate` is **not** a pure schema check
for this subsystem — it touches the filesystem for `disk_layer` and resolves the
`rtds_layer` cluster reference.

## §3. Non-goals — do NOT widen into these

1. **The admin `GET /runtime` endpoint** — sibling `108.2`.
2. **The nine `runtime.*` stats** — sibling `108.2`.
3. **Differential fixture `0087`** — sibling `108.2`. 108.1 adds **no fixture**.
4. **`BEHAVIOR_CONTRACT.md`'s `## Runtime` section** — sibling `108.2`, written
   once against the observable surface.
5. **`disk_layer`** — runtime semantics UNMEASURED, and this host has virtiofs
   with no inotify, so a disk-reload path would be CI-authoritative only.
   Rejected loudly (D2). **CF-108-1.**
6. **`rtds_layer`** — needs an xDS cluster; belongs with the xDS family.
   Rejected loudly. **CF-108-1.**
7. **`admin_layer` + `POST /runtime_modify`** — state-MUTATING. Rejected loudly.
   **CF-108-1 / CF-108-2.** Fully measured in the parent SPEC's R-7 so a later
   slice inherits the measurement: POST-only, **405** on GET, **503**
   `No admin layer specified` without an admin layer, and `load_success` /
   `override_dir_not_exists` incrementing on every POST.
8. **Honoring `runtime_key` in the two existing consumers** (`RuntimeUInt32` for
   `status_code_filter`, `RuntimeFractionalPercent` for CSRF). That changes the
   behaviour of two shipped filters and belongs to the consumer slice. **This is
   why all eleven N-9 sites stay true.**
9. **Route-level `runtime_fraction` gating** — a route-matching change; this is
   the producer.
10. **FractionalPercent-shaped struct values.** MEASURED trap (parent §5 item 6,
    **CF-108-3**): a nested map containing `numerator` is **not** flattened like
    every other nested map — it is kept as ONE key whose value is the protobuf
    **text-format** dump of the Struct, complete with literal `\n`s. Matching
    that byte-for-byte means reimplementing protobuf `DebugString`. D2 decides
    at implementation time whether to reject it loudly or leave it unmodelled;
    **either way it is recorded, not silently unhandled.**
11. **Hot restart** (`/hot_restart_version`, `--restart-epoch`) — the other half
    of the family heading, entirely UNMEASURED, a separate phase.

## §4. Differential surface at sub-phase end

**No new fixture.** The witness is regression-equivalence plus in-process
backstops:

- All **86** pre-existing fixtures still green. This slice is additive: no
  existing config carries `layered_runtime`, so the new parse path is inert for
  every one of them.
- `h2spec` unchanged; `known-failures.txt` untouched at 21 lines / 1 entry.
- The behavioural witnesses are D7's in-process backstops, which pin every rule
  at §2 against the measured upstream transcripts recorded there.

This is the deliberate `76.1` shape: a foundation slice earns its keep by being
regression-clean and unit-pinned, and its differential proof arrives with the
sibling.

## §5. Carry-forwards

### OPENED here
- **CF-108-1** — `disk_layer` / `rtds_layer` / `admin_layer` are accepted by
  upstream and boot-fatal here. A recorded reject-direction divergence,
  differentially unobservable (a rejected config never reaches the wire).
  Owner: whichever phase lands each arm.
- **CF-108-4 [NEW]** — the **YAML 1.1 vs YAML 1.2 boolean divergence** (N-2).
  Whatever D1 decides, the residue is a recorded divergence class covering
  `y`/`n`/`on`/`off` and any other YAML-1.1-only scalar spelling.

### ADVANCED, not consumed
- **CF-108-2** (`/runtime_modify` absent) and **CF-108-3** (FractionalPercent
  text-format rendering) pass through to 108.2 and beyond unchanged.
- The two inert/rejected `runtime_key` fields (`RuntimeUInt32`,
  `RuntimeFractionalPercent`). This slice builds the store they need but does
  not read from it. They stay open, with the blocker removed.

### Consumed
None. This slice consumes no carry-forward and fixes none of the banked Minors
or Nits from the `76.1` / `76.2` reviews (§6.3 — a phase picks its scope; it
does not clear a backlog).

## §6. Definition of done — the §7.5 gate, instantiated

- **(a)** No new/changed differential fixture — vacuously satisfied, and the
  state-4 session must RECORD that rather than skip it silently.
- **(b)** All **86** pre-existing fixtures still green.
- **(c)** `h2spec` unchanged and above threshold; `known-failures.txt` untouched.
  No H2 codec or framing change in this slice.
- **(d)** No new fuzz target (D8); the pre-existing `parse_bootstrap` target runs
  clean on its short CI budget, and the new corpus seed is proven tracked via
  `git ls-files`. **Record this explicitly.**
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo test --workspace` and `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

Standing constraints that bind this slice: never weaken a fixture; never trim
`known-failures.txt`; `#![forbid(unsafe_code)]` holds at every crate root
(D-3.8); no `ENVOY_TARGET.md` or `rust-toolchain.toml` change (D-3.7 / D-3.9);
ADR-0028 is not lifted; and no landed artifact of any closed phase is edited
(D-3.5).

## §7. Size estimate

Re-derived bottom-up at the split, anchored on landed phases measured on disk
rather than projected:

| Deliverable | non-test | test | total |
|---|---:|---:|---:|
| D1 schema (incl. the recursive value type) | ~150 | ~180 | ~330 |
| D2 validators + `ConfigError` variants | ~110 | ~170 | ~280 |
| D3 snapshot store | ~140 | ~180 | ~320 |
| D7 config-side backstops (incremental) | — | ~130 | ~130 |
| D8 corpus seed + `.gitignore` un-ignore | ~38 | — | ~38 |
| **Total** | **~438** | **~660** | **≈1098** |

Anchors: `76.1`'s `crates/envoy-config/src/bootstrap.rs` landed **+655** for a
comparable config surface, its `lib.rs` **+28** for the `ConfigError` variants,
and its corpus seed **+38**; `75.1`'s `crates/envoy-config/src/matcher.rs`
landed **+241** for a self-contained engine plus its tests.

**Under the ~1500 gate, with headroom for a §5.2 re-entry** — which must be
budgeted, not assumed away: `76.2` grew from **1265** at state-3 close to
**1568** once its review's fixes landed, a **+24%** overrun, and `76.1` overran
its own PLAN projection by **+50%**. At +24% this slice lands near **1361**;
at +50%, near **1647**. **If the state-3 session finds itself crossing ~1500,
that is a §6.1 mid-execution trigger, not something to absorb.**

## §8. NOT MEASURED — stated explicitly per D-3.4

1. **`disk_layer` runtime semantics.** It validated against a real mounted
   directory but was never booted and `/runtime` never read with disk-sourced
   keys.
2. **`rtds_layer` end to end.** Never validated or booted successfully — both
   attempts failed on the cluster reference.
3. **`runtime.load_error`** — only ever observed at 0; no config was found that
   increments it.
4. **The `deprecated_feature_*` counters under a real deprecated field.**
5. **Whether setting a REAL reloadable feature flag changes behaviour.** A
   recognised flag appears cleanly in `/runtime`, but no gated code path was
   exercised. Note the parent's R-6 caveat: an unrecognised key under the
   `envoy.reloadable_features.` prefix emits a non-fatal `envoy_bug` line on
   **stderr** while still exiting 0. It is log-only and §7.2 does not compare
   stderr.
6. **Three or more layers.** Two were measured (N-6); ordering matched config
   order in every probe.
7. **Very large snapshots** — no test of chunking or ordering at hundreds of keys.
8. **Hot restart**, entirely.

## §9. Next state

**State 2 — the PLAN-write for `108.1`** (`superpowers:writing-plans`), a
**separate session** per §5.1 and ADR-0127. Sub-phase directories created by a
split enter the lifecycle at state 1/2 with their `SPEC.md` already written, so
the next session writes `108.1/PLAN.md` (TDD-ordered numbered tasks) and writes
**no code**. It should re-derive every line anchor in §2 by TEXT before
transcribing it — `crates/envoy-config/src/bootstrap.rs` is ~21 000 lines and
`crates/envoy-http1/src/hcm.rs` ~10 900, and both drift.
