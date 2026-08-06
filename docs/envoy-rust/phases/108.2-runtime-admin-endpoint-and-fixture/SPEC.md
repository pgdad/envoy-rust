# Sub-phase 108.2 — admin `GET /runtime` + the nine `runtime.*` stats + differential fixture `0087`

> **Created by the §6.1 SPLIT of phase 108, executed at the §5 state-2 PLAN-write
> on 2026-08-05 and recorded in ADR-0172.** The parent SPEC is
> `docs/envoy-rust/phases/108-runtime-static-layer/SPEC.md` (935 lines,
> findings R-1…R-18). The sibling that must land FIRST is
> `docs/envoy-rust/phases/108.1-runtime-config-and-snapshot/SPEC.md`, which
> builds the config schema and the snapshot store this slice renders.
> This document is self-contained for 108.2's slice (doctrine D-3.4).

## §0. What this sub-phase is, in one paragraph

Sibling `108.1` builds the **producer**: the `layered_runtime` / `static_layer`
config schema and an in-memory snapshot store that flattens, stringifies and
layers runtime key/values. Nothing observes it. This sub-phase builds the
**observer**: the eleventh admin endpoint `GET /runtime`, the nine `runtime.*`
stats, and the differential fixture `0087` that proves envoy-rust's whole
runtime snapshot is equivalent to upstream Envoy's on the same config. It closes
parent phase `108`.

**`108.2` depends on `108.1` being `done`.** It renders a store that does not
exist until `108.1` lands.

## §1. Scope — what 108.2 builds

### D4 — admin `GET /runtime` (`crates/envoy-admin`)

The **eleventh** endpoint. Five mechanical edits in
`crates/envoy-admin/src/endpoint.rs`, each anchor RE-DERIVED BY TEXT at the
split (line numbers drift — `endpoint.rs` is ~3091 lines):

| edit | current anchor | note |
|---|---|---|
| a variant on `enum AdminEndpoint` | declared at **:9**, spans **9-79**, **10** variants today | |
| an arm in `from_path` | `fn` at **:96**, `match` **102-116**, fn closes **:117** | **exactly ten** paths today, `_ => None` at **:115** |
| an arm in `allowed_method` | `fn` at **:122**, closes **:136** | **exhaustive, NO wildcard arm** |
| an arm in `render_with` | `fn` at **:163**, match **164-187**, closes **:188** | **exhaustive, NO wildcard arm** |
| the renderer itself | — | a `#[derive(Serialize)]` struct + `json_pretty_200` |

**TWO compile-forcing sites, not one.** The parent SPEC named `render_with`;
**`allowed_method` is also a wildcard-free exhaustive match**, re-verified
arm-by-arm at the split. Adding the variant is therefore a hard compile error
until both are handled — a genuine forcing function that should be relied on
rather than duplicated by a grep.

The body is handed to the existing `json_pretty_200` helper (`fn` at **:255**,
closes **:264**), which already emits `("content-type", "application/json")` at
**:261** via `serde_json::to_vec_pretty` at **:256** and — deliberately, per its
own doc — sets **no `content-length`**. Upstream's `/runtime` is likewise
pretty-printed and `transfer-encoding: chunked` (§2). **Byte-equality of
whitespace is NOT relied on** — `BodyRule::JsonShape` parses both sides (N-1).
`crates/envoy-admin/Cargo.toml:16` already declares `serde_json = "1"`, so there
is **zero new response plumbing and no manifest edit**.

**Two convention-only tests want a new row** — neither is compile-forcing, so
both must be updated deliberately:
- `get_known_path_returns_endpoint` (`#[test]` at **:2331**, `fn` **2332-2366**),
  whose own comment states its purpose is to *"guard against any future variant
  being added to `from_path` without a corresponding dispatch-test row."*
- `each_endpoint_declares_its_allowed_method` (`#[test]` at **:2414**, `fn`
  **2415-2425**). Note it asserts only the **7 GET** variants; the 3 POST
  variants live in `each_drain_endpoint_declares_post_allowed_method`
  (**2536-2539**).

### D5 — the nine `runtime.*` stats

```
runtime.admin_overrides_active
runtime.deprecated_feature_seen_since_process_start
runtime.deprecated_feature_use
runtime.load_error
runtime.load_success
runtime.num_keys
runtime.num_layers
runtime.override_dir_exists
runtime.override_dir_not_exists
```

Registered **unconditionally** — upstream emits all nine even on a config with
no `layered_runtime` block at all (§2). Nine
`register_counter` / `register_gauge` calls against the flat `BTreeMap`-backed
registry: `register_counter` at `crates/envoy-stats/src/registry.rs:45`,
`register_gauge` at **:69**, both taking a dotted name `&str` directly.
`is_valid_name` (**104-120**) permits `.` explicitly at **:114**
(`c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '-')`) — **but note
the FIRST character does not permit `.`** (**:109**), which is fine for a
`runtime.`-prefixed name. The smallest existing wiring example is
`crates/envoy-bin/src/network_rbac.rs:58-61`.

**MEASURED semantics (§2):** only `num_keys` and `num_layers` track config;
`load_success: 1` and `override_dir_not_exists: 1` fire unconditionally; the
other five are `0` on any in-scope config.

### D6 — differential fixture `0087-runtime-static-layer`

`Driver::AdminScrape`, `pre_requests: []`, **zero clusters, no backend**. The
harness types, re-derived at the split (`tests/differential/src/lib.rs`, ~10 880
lines):

- `Driver::AdminScrape` at **:346**, closing **:364** — **four** fields:
  `pre_admin_actions` (:352), `pre_requests` (:354), `scrapes` (:355, the only
  one without a serde default), `post_admin_assertions` (:363).
- `AdminScrapeCase` at **:555**, closing **:560**, `#[serde(deny_unknown_fields)]`
  at :554 — exactly four fields, **all REQUIRED**: `path: String`,
  `expected_status: u16`, `expected_content_type: String`,
  `expected_body_rule: BodyRule`.
- `BodyRule::JsonShape` at **:818**, closing **:830** — five fields, **all**
  `#[serde(default)]`: `required_keys`, `required_subtree`,
  `allowlist_envoy_only_keys`, `allowlist_envoy_rust_only_keys`,
  `value_may_differ_keys`.

**Because only ONE `required_subtree` is permitted per rule, the fixture uses
TWO scrapes of the same `/runtime` path** — one anchoring the whole `entries`
object, one anchoring `layers` — both with `required_keys: ["entries","layers"]`
and **empty per-side allow-lists**. The intent is that **nothing** needs
allow-listing; if something does, that is a finding, not a knob to turn.

The static layer must witness every measured rule: a bool, an integer, a
negative integer, a float, a string, a quoted numeric string, an empty-string
value, and a **two-level** nested map. **It should use TWO static layers** so
`layer_values` slot ordering, the `""`-absent slot and last-non-empty-wins
precedence are all witnessed (§2, N-6/N-7) — the parent SPEC could only measure
those via the out-of-scope `admin_layer`. It deliberately does **not** use the
`envoy.reloadable_features.` prefix, and — per the YAML-1.1 finding at §2 — it
must not carry an unquoted `y`/`n`/`on`/`off` value unless `108.1` resolved that
divergence and the fixture is deliberately witnessing the resolution.

### D9 — `BEHAVIOR_CONTRACT.md`

A new `## Runtime` section carrying the layer grammar, the snapshot rules, the
absent-vs-empty edge and the no-pre-population fact; one new row in
`## Admin endpoint body shapes` (at **:1348**, whose preamble already says *"any
later admin endpoints append rows here with the same columns"*); and the nine
stats in `## Stat-name mapping`.

### The parent close

`108.2`'s state-6 close-out flips ROADMAP row `108.2` **and** parent row `108`
to `done`, per the `76.2` precedent (a parent flips only at the close-out of its
last sub-phase; the `76.1` close-out correctly flipped its own row alone).

## §2. The measured upstream behaviour this slice must reproduce

Driven against the pinned image `envoyproxy/envoy:v1.33.0`, digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`,
port-mapped with `docker run -p` and probed with `curl`. **All transcripts below
were taken fresh at the state-2 PLAN-write on 2026-08-05.**

### The response shape

```
HTTP/1.1 200 OK
content-type: application/json
cache-control: no-cache, max-age=0
x-content-type-options: nosniff
date: …
server: envoy
transfer-encoding: chunked
```

Top-level shape is exactly two keys: **`entries`** (object) and **`layers`**
(array of layer NAME strings, in config order). Each `entries` value is an
object with **`final_value`** (string) and **`layer_values`** (array of strings,
one slot per configured layer). **There is no `format` query parameter** —
`?format=text` and any unknown parameter are silently ignored and still return
the same JSON 200.

### N-1 — the byte-nondeterminism is REAL and is fully absorbed (V-2, CONFIRMED BY EXPERIMENT)

**8 consecutive GETs against one unchanged process produced 8 DISTINCT md5
sums**, every response exactly 1126 bytes — a pure key/field-order shuffle,
both between `entries` keys and between the two fields inside each entry.

`BodyRule::JsonShape` absorbs it completely. Its evaluation arm spans
**7153-7271** (the parent's `7153-7215` understates the end). It
`serde_json::from_slice`s both bodies, walks a dotted path per side via
`walk_pointer`, converts the fixture's `expected` (a `serde_yaml::Value`) to
`serde_json::Value`, and asserts `envoy_str == expected_str` (**:7206**) **AND**
`rust_str == expected_str` (**:7211**) — so cross-side equality follows by
transitivity. It then diffs the top-level key sets modulo per-side allow-lists
(**7217-7250**) and compares shared-key values (**7252-7269**).

Confirmed by experiment on the workspace's exact pinned `serde_json 1.0.149`:

```
raw bytes identical? false
to_string identical?  true
subtree  entries: to_string identical? true
subtree   layers: to_string identical? true
```

for both shuffled-vs-shuffled and shuffled-vs-deliberately-key-sorted inputs.
Mechanically: `git grep 'preserve_order'` over `*.toml`/`*.lock`/`*.rs` returns
**zero** hits, and `serde_json`'s dependency block in `Cargo.lock` lists
`itoa, memchr, serde, serde_core, zmij` — **no `indexmap`**. `Map` is a
`BTreeMap` and re-serialisation key-sorts both sides identically at every depth.
**The fixture design HOLDS.**

### The value and flattening rules — a full measured transcript

Config (one static layer, 11 top-level keys, one of them a two-level nested map):

```yaml
layered_runtime:
  layers:
  - name: static_layer_0
    static_layer:
      my.bool.true.key: true
      my.bool.false.key: false
      my.numeric.key: 42
      my.negative.key: -7
      my.float.key: 1.5
      my.string.key: hello
      my.quoted.number.key: "42"
      my.yaml11.y.key: y
      my.quoted.y.key: "y"
      my.empty.string.key: ""
      my.nested:
        sub_key: v
        deeper:
          leaf: w
```

yields exactly **12** entries — every one `{"final_value": S, "layer_values": [S]}`:

| key | `final_value` |
|---|---|
| `my.bool.true.key` | `"true"` |
| `my.bool.false.key` | `"false"` |
| `my.numeric.key` | `"42"` |
| `my.negative.key` | `"-7"` |
| `my.float.key` | `"1.5"` |
| `my.string.key` | `"hello"` |
| `my.quoted.number.key` | `"42"` |
| `my.yaml11.y.key` | **`"true"`** — see the YAML-1.1 note |
| `my.quoted.y.key` | `"y"` |
| `my.empty.string.key` | `""` |
| `my.nested.sub_key` | `"v"` |
| `my.nested.deeper.leaf` | `"w"` |

with `"layers": ["static_layer_0"]`, `runtime.num_keys: 12`,
`runtime.num_layers: 1`.

Four rules follow, all binding on the fixture:

1. **All values stringify.** Bools, ints, floats and strings all render as JSON
   strings.
2. **Nesting flattens to dotted keys at ARBITRARY DEPTH** — `my.nested.deeper.leaf`
   appears; no intermediate `my.nested` or `my.nested.deeper` entry exists. The
   parent measured only one level.
3. **`num_keys` counts FLATTENED LEAVES** (12), not top-level YAML keys (11).
4. **⚠ YAML 1.1.** Upstream's parser booleanizes unquoted `y`/`n`/`on`/`off`
   (`y` → `true` → `"true"`), while `serde_yaml` implements the YAML 1.2 core
   schema where `y` is the string `"y"`. **Sibling `108.1` owns this decision
   (its N-2, carry-forward CF-108-4); `108.2`'s fixture must not stumble into
   it accidentally.**

### N-6 / N-7 — two static layers, slot ordering and precedence

Two `static_layer` entries with distinct names are legal, so multi-layer
behaviour is witnessable **in scope**, without the out-of-scope `admin_layer`:

```json
"shared.key":        { "layer_values": ["from_base", "from_override"], "final_value": "from_override" }
"only.in.base":      { "layer_values": ["base_val", ""],               "final_value": "base_val"      }
"only.in.override":  { "layer_values": ["", "over_val"],               "final_value": "over_val"      }
"empty.in.override": { "layer_values": ["real_value", ""],             "final_value": "real_value"    }
```

with `"layers": ["base_layer","override_layer"]`, `num_layers: 2`, `num_keys: 4`.

- Slot order follows **config order**; `""` marks "absent from that layer".
- `empty.in.override` was set to `real_value` in the base layer and to the empty
  string in the override layer. **`final_value` is the last NON-EMPTY slot** — an
  explicitly-set empty string does **not** override, and it is
  **indistinguishable on the wire from absence**. Both render `""`.

### Absent vs empty — not equivalent

| config | `/runtime` | `num_layers` | `num_keys` |
|---|---|---:|---:|
| no `layered_runtime` block | `{"entries":{},"layers":[]}` | 0 | 0 |
| `layered_runtime: {}` | `{"entries":{},"layers":[""]}` | 1 | 0 |
| `layered_runtime: { layers: [] }` | `{"entries":{},"layers":[""]}` | 1 | 0 |

Upstream synthesizes ONE layer named the **empty string** for both empty
spellings.

### The nine stats, measured

On the 12-key single-layer config above:

```
runtime.admin_overrides_active: 0
runtime.deprecated_feature_seen_since_process_start: 0
runtime.deprecated_feature_use: 0
runtime.load_error: 0
runtime.load_success: 1
runtime.num_keys: 12
runtime.num_layers: 1
runtime.override_dir_exists: 0
runtime.override_dir_not_exists: 1
```

All nine exist **unconditionally**, including on a config with no
`layered_runtime` block (where `num_keys` and `num_layers` are `0` and the other
seven are unchanged).

### ⚠ The `expected_stats` zero-value vacuous-pass trap

`tests/differential/src/lib.rs:4500-4507` documents it, in the doc comment on
`assert_expected_stats_bilaterally` (the sentences run **4504-4507**, restated at
**:4651** and **:4683**):

> `scrape_admin_stat` returns `Ok(0)` for a stat name the proxy never
> registered. A `value: 0` assertion therefore passes vacuously when the name
> is ABSENT; only a non-zero assertion is a real witness. Fixture READMEs must
> say which of their assertions is the witness.

**Only `num_keys`, `num_layers` and `load_success` can be witnessed by value**
on any in-scope config. `override_dir_not_exists` is also non-zero (`1`) and
therefore also a real witness — **a fourth**, which the parent SPEC missed. The
remaining five are `0` and must be witnessed by the `/stats` **name-set**
comparison instead. The fixture README must say exactly this.

## §3. Fixture `0011` needs NO edit — RE-VERIFIED (V-11)

Fixture `0011-admin-stats-prometheus` already allow-lists the nine names as
**envoy-only**, at `expectations.yaml` **lines 234-242** (contiguous, all nine,
exactly the names above with an `envoy_` prefix and `.`→`_`), with the rationale
at `expectations.yaml:35` and `README.md:55`:

> `- \`runtime.*\` (9) — RTDS runtime layer. Deferred to the xDS family.`

**Mechanically it needs no edit, and this was re-verified by reading the rule
rather than assumed.** `BodyRule::PrometheusExposition`'s arm spans **7068-7142**
(the parent's `7068-7095` covers only the name-set portion). At **7081-7085** it
computes:

```rust
let envoy_only: Vec<String> = envoy_names
    .difference(&rust_names)
    .filter(|n| !allow_envoy.contains(*n))
    .cloned()
    .collect();
```

The allow-list **filters** the difference in the permissive direction. Once
envoy-rust emits the nine, they leave the difference entirely and land in the
intersection, so the closure never sees them and `envoy_only` stays empty.
**There is no "unused allow-list entry" check anywhere in the tree** — searched
for `unused`, `never matched`, `stale`, `leftover`, `unmatched` across the
tracked tree; every hit is unrelated. And `0011`'s `value_exact`,
`value_must_be_zero` and `value_present_only` lists are all empty. **`0011`
does NOT go RED when D5 lands.**

**But its PROSE becomes wrong**, and that is this slice's to dispose of. The
line calls the nine an xDS-family deferral; after D5 they are neither deferred
nor xDS-family. This is the **one** of the eleven "no runtime subsystem"
assertions in the tree (censused in sibling `108.1`'s SPEC §2 N-9) that becomes
semantically false, and it becomes false **here**, not in `108.1`.

**DECISION, recorded rather than left implicit:** `0011` is a landed fixture of
a closed phase, and D-3.5 forbids editing landed artifacts. The nine allow-list
entries and the README line therefore **stay**, and this SPEC plus the D9
`BEHAVIOR_CONTRACT.md` section carry the correction — the same disposition the
parent SPEC took. **The state-5 reviewer is directed here** so the stale prose
reads as a known, recorded mis-filing rather than an oversight. Deleting the
nine entries is optional tightening for a later session that legitimately
touches `0011`; it is not an obligation of this slice, and doing it opportunistically
would be a §6.3 scope widening.

## §4. V-7 — nothing existing breaks when `/runtime` turns 404 → 200

Re-verified by full-tree census at the split:

- **Zero** references to `/runtime` as an HTTP path anywhere in `crates/`,
  `tests/` or any fixture. `git grep -- '/runtime' -- ':!docs/'` returns 7 lines
  and **all 7 are the English word-pairs "startup/runtime" or "main/runtime"**.
  All real path references live in `docs/` and are forward-looking deferral notes
  (`phases/06*/SPEC.md`, `phases/08-admin-api-and-drain/SPEC.md:224`) or the
  phase-108 records themselves. None is an executable assertion.
- **No test enumerates "these paths are NOT endpoints"** in a way `/runtime`
  would break. Every negative probe uses `/nope`, `/unknown`, `""` or `/`:
  `from_path_unknown_returns_none` (**1039-1048**) — whose comment shows it has
  already been re-targeted once, when `/listeners` was promoted from unknown —
  `unknown_path_returns_not_found_regardless_of_method` (**2368-2382**),
  `query_string_strips_to_config_dump` (**2705-2721**), and
  `handler_returns_404_for_unknown_path` (`handler.rs:557-560`).
- **No endpoint COUNT assertion exists anywhere.** The only enumerating test is
  `get_known_path_returns_endpoint`, which is positive-only — it will still pass,
  and it is the deliberate place to add a row (§1 D4).
- **No fixture `expectations.yaml` asserts a 404 on an admin path.** All fixture
  404s are data-plane route-miss synth-404s.

**Admin paths exercised by fixtures today:** `/ready` (0002, via `http_get`),
`/stats/prometheus` (0011), `/config_dump` incl. `?include_eds` (0014, 0026,
0027, 0028, 0029), `/server_info` (0014, 0015), `/clusters` (0014),
`/listeners` (0014, 0027), `POST /drain_listeners` (0015). `/stats`,
`/healthcheck/fail` and `/healthcheck/ok` are unwitnessed by any fixture.
**Seven of the 86 fixtures touch the admin listener via scrape machinery** — the
pattern is proven, not projected.

## §5. V-5 — host-cleanliness, discharged as far as it can be and NO FURTHER

The parent asked this slice to prove fixture `0087` runs clean on this
development host. **Half of it is now measured, and the other half is
structurally impossible until the code exists — stated plainly rather than
assumed.**

**Measured:** the upstream side is fully clean here. Every probe in §2 ran
locally against the pinned image, port-mapped, repeatedly, with zero clusters and
zero backends, and returned stable 200s. A cluster-free admin-scrape fixture has
no backend to route to, so it cannot hit the host's backend-routing failure mode.

**Not measurable yet:** the envoy-rust side does not exist until D4/D5 land, so
the cross-proxy run cannot be exercised at the split.

**The specific risk the parent named, and its status.** Fixture `0014`
(`admin_config_dump_server_info`) is a KNOWN deterministic host-flake in the
`192.168.65.2` bridge-IP family. Its failure is in `/clusters`
**backend-endpoint addresses** — a surface `0087` does not have, since `0087`
declares zero clusters and scrapes only `/runtime`. That is a sound structural
argument, and it is **not** a substitute for running it. **The state-4 session
MUST run `0087` locally and record the transcript**, and must not read the
argument above as pre-discharged. Backend-routing fixtures go RED on this host
and CI is authoritative for them; a backend-free fixture is fully verifiable
locally, which is the property that made phase 76 land cleanly.

## §6. Non-goals — do NOT widen into these

1. **The config schema, validators and snapshot store** — sibling `108.1`.
   `108.2` renders that store; it does not build or reshape it.
2. **`disk_layer` / `rtds_layer` / `admin_layer`** — loudly rejected by `108.1`.
   **CF-108-1.**
3. **`POST /runtime_modify`** — upstream serves it; envoy-rust will 404.
   **CF-108-2.** Fully measured in the parent SPEC's R-7 so a later slice
   inherits it: POST-only, **405** on GET, **503** `No admin layer specified`
   without an admin layer, and `load_success` / `override_dir_not_exists`
   incrementing on every POST.
4. **Editing fixture `0011`** — see §3. D-3.5 forbids it; the correction lives
   in D9.
5. **Honoring `runtime_key` in the two existing consumers**, and route-level
   `runtime_fraction` gating — the consumer slice, a separate phase.
6. **FractionalPercent-shaped struct values** — **CF-108-3**: a nested map
   containing `numerator` is kept as ONE key whose value is the protobuf
   **text-format** dump of the Struct with literal `\n`s. The fixture must use no
   such value.
7. **Hot restart** — the other half of the family heading, entirely UNMEASURED.
8. **`/stats`, `/healthcheck/fail`, `/healthcheck/ok`** remain fixture-unwitnessed.
   Noted, not fixed here.

## §7. Differential surface at sub-phase end

- **NEW** fixture `0087-runtime-static-layer` green — the whole `entries` object
  and the `layers` array equal across both proxies, order-insensitively.
- All **86** pre-existing fixtures still green, including `0011` (§3).
- `h2spec` unchanged; `known-failures.txt` untouched at 21 lines (which hold
  exactly **one** real entry, `3.5/2` — the other 20 lines are a header comment
  and a blank. Never trim it: this host scores `3.5/2` as PASS where CI does not).

## §8. Carry-forwards

### OPENED here
None beyond those `108.1` opens.

### ADVANCED, not consumed
**CF-108-1** (three rejected arms), **CF-108-2** (`/runtime_modify` absent),
**CF-108-3** (FractionalPercent text-format) and **CF-108-4** (the YAML-1.1
boolean divergence) all pass through. **CF-76-1** stays OPEN and out of scope —
the parent SPEC §2.7(a) banks four measured improvements to its record (a
SECOND matcher with the same bug at `crates/envoy-filter/src/jwt_authn.rs:173-186`;
three unshared query-strip implementations; `plan_redirect` already being
query-strip-correct; and an honest 8-12 probe / ≈900-1100 LoC sizing) which the
next session that takes it inherits.

### Consumed
Parent phase `108` closes at this slice's state-6. No banked `76.1` / `76.2`
Minor or Nit is fixed here (§6.3).

## §9. Definition of done — the §7.5 gate, instantiated

- **(a)** Fixture `0087-runtime-static-layer` green cross-proxy, run **locally**
  with the transcript recorded (§5).
- **(b)** All **86** pre-existing fixtures still green.
- **(c)** `h2spec` unchanged and above threshold; `known-failures.txt` untouched.
  No H2 codec or framing change in this slice.
- **(d)** No new fuzz target; the pre-existing `parse_bootstrap` target runs
  clean on its short CI budget. **`ci.yml` needs no new step — RECORD that
  explicitly** rather than skip gate (d) silently.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo test --workspace` and `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

Standing constraints: never weaken a fixture; never trim `known-failures.txt`;
`#![forbid(unsafe_code)]` at every crate root (D-3.8); no `ENVOY_TARGET.md` or
`rust-toolchain.toml` change (D-3.7 / D-3.9); ADR-0028 is not lifted; no landed
artifact of any closed phase is edited (D-3.5).

## §10. Size estimate

| Deliverable | non-test | test | total |
|---|---:|---:|---:|
| D4 admin endpoint + renderer | ~90 | ~130 | ~220 |
| D5 nine stats + wiring | ~55 | ~80 | ~135 |
| D6 fixture `0087` (2 YAMLs + expectations + README + test file) | — | ~300 | ~300 |
| **Total** | **~145** | **~510** | **≈655** |

**D6's anchor is MEASURED, and it corrects the parent SPEC's ~180 badly.** Two
comparable landed fixtures, measured on disk at the split:
`0086-route-redirect-action` = **442** net lines across its five files (README
113, `envoy.yaml` 71, `envoy-rust.yaml` 71, `expectations.yaml` 140, test file
47); `0083-headermatcher-absence-parity` = **745** (README 219, two YAMLs 137
each, `expectations.yaml` 214, test file 38). `0087`'s configs are much smaller
(no listeners, no clusters) but its `expectations.yaml` carries the entire
`entries` object as an expected subtree, so ~300 is the honest middle.

**Comfortably under the ~1500 gate.** Note `D9`'s `BEHAVIOR_CONTRACT.md` edits
sit under `docs/` and are therefore outside the metric the gate is measured with
(`git diff --numstat <base> <head> -- . ':(exclude)docs/'`) — they are real work
but do not count toward the split threshold.

## §11. Next state

`108.2` is **BLOCKED on `108.1` reaching `done`.** Its ROADMAP row is `planned`
and its `depends-on` column names `108.1`. When `108.1` closes, the next session
runs `108.2`'s **state-2 PLAN-write** (`superpowers:writing-plans`) and writes
`108.2/PLAN.md`. It must re-derive every `file:line` anchor in this document BY
TEXT before transcribing it — `crates/envoy-admin/src/endpoint.rs` is ~3091
lines and `tests/differential/src/lib.rs` ~10 880, and both drift. It must also
re-measure the exact `expected` JSON against the pinned image using the
fixture's **own** config and record that transcript, rather than transcribing
§2's illustrative one.
