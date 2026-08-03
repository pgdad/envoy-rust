# Phase 108 — Runtime + hot-restart family OPENER: `layered_runtime` `static_layer` + admin `GET /runtime` + the `runtime.*` stat frame

> **Pick + scope locked by ADR-0171** (§5 state-0/1 next-phase pick, 2026-08-03).
> **ADR-0172 is RESERVED-UNFIRED** for the §6.1 split and/or the §6.2 empirical
> reconciliation at the state-2 PLAN-write.

## §0. How to read this document

This SPEC is written for a stranger with zero prior context (doctrine D-3.4).
Every claim below is either (a) **MEASURED** — with the command and the observed
output recorded at §1 — or (b) explicitly flagged **NOT MEASURED** at §8. Nothing
is asserted from documentation or from memory of upstream Envoy's source; per
doctrine D-3.3 the contract is the contract, and upstream C++ is never read to
decide what equivalence means.

Two categories of measurement appear here:

- **LIVE-ENVOY** — driven against the pinned reference image
  `envoyproxy/envoy:v1.33.0`, digest
  `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`
  (`docs/envoy-rust/ENVOY_TARGET.md`), port-mapped with `docker run -p` and
  probed with `curl`, plus wire-shape probes via upstream's `--mode validate`.
- **TREE** — a read-only census of the envoy-rust working tree at
  `b3a89d33b7bcf30e1b917aed6f2af62abe03b4ad`.

**Line numbers drift.** Every anchor below is given with enough surrounding text
to be re-found by grep. The state-2 PLAN-write MUST re-derive every anchor and
every count rather than inherit it (§7 PLAN-VERIFY).

---

## §1. State-0 recon — the evidence this pick rests on

### R-1 — TREE: the Runtime + hot-restart family has ZERO ROADMAP rows, and so does the heading it must go under

`docs/envoy-rust/ROADMAP.md:183` is the bare heading `### Runtime + hot restart
family`. It carries **no descriptive prose line, no `| id | title | … |` table
header, and no rows**; line 184 is blank and line 185 is the next heading
(`### WASM host family`). Measured by slicing each `### ` heading to the next and
listing the row ids beneath it:

```
  HTTP filters family              [10]  09 10 11 22 24 23 25 25.1 25.2 31
  Network filters family           [ 5]  66 67 67.1 67.2 67.3
  Load balancing family            [ 3]  28 29 30
  Upstream robustness family       [14]  12 12.1 12.2 13 13.1 13.2 14 14.1 15 14.2 16 68 69 17
  HTTP/3 + QUIC family             [ 0]
  gRPC family                      [ 0]
  xDS / dynamic config family      [ 6]  18 19 20 21 26 27
  Observability family             [29]  32 … 58 61 63
  Runtime + hot restart family     [ 0]
  WASM host family                 [ 0]
  Deprecated / edge features       [13]  64 65 70 71 72 73 74 75 75.1 75.2 76 76.1 76.2
```

**FOUR of the eleven family headings carry zero rows.** This phase opens one of
them, which is the single clearest way to advance the mission's stop condition
(see §2.1).

### R-2 — TREE: the "13 rows under `### Deprecated / edge features`" is a FILE-LAYOUT ARTIFACT, not a classification — and it changes where this row must go

**NEWLY MEASURED AT THIS SESSION, and it corrects a reading the standing ledger
invites.** None of the 13 rows listed under `### Deprecated / edge features`
(line 189) is a deprecated or edge feature. They are Observability rows (`64`,
`65`, `70`–`74`), the cross-cutting `HeaderMatcher` rows (`75`, `75.1`, `75.2`)
and the redirect rows (`76`, `76.1`, `76.2`). They sit there because
`### Deprecated / edge features` is the **last heading in the file** and every
row since `64` has been appended at EOF regardless of family.

Two consequences, both binding on this phase:

1. **The de-facto practice of the last 13 rows is EOF-append, and following it
   here would defeat the point of the pick.** A row appended at EOF lands under
   `### Deprecated / edge features` and leaves
   `### Runtime + hot restart family` at zero rows — so the heading census that
   drives stop-condition leg (iii) would not move. **Row 108 is therefore
   INSERTED under the `### Runtime + hot restart family` heading, not appended
   at EOF.** This is a deliberate deviation from recent practice and is recorded
   in ADR-0171.
2. **The insertion must also add the two table-header lines**, because that
   heading has none. Six headings carry a `|---|---|---|---|---|---|` rule
   (HTTP filters, Network filters, Load balancing, Upstream robustness, xDS,
   Observability); five do not (HTTP/3, gRPC, Runtime, WASM, Deprecated). The
   edit is a pure insertion of **four lines** (blank + header + rule + row), so
   `git diff --numstat` on `ROADMAP.md` must be exactly `4 0` — nothing removed,
   no existing row touched.

The seven rows carrying unescaped `|` characters (`36`/`38`/`39`/`52`/`54`/`66`/
`70`) are **NOT** repaired — `ROADMAP.md` is append-only history.

### R-3 — LIVE-ENVOY: `layered_runtime` with a `static_layer` is accepted, and the layer grammar is fully measured

Against the pinned image under `--mode validate`, driving one construct per
probe with a fresh config filename per revision (this host's Docker bind mounts
are stale-cached):

| construct | verdict | evidence |
|---|---|---|
| `static_layer` with bool / int / string / nested-map values | **ACCEPT** (exit 0, `configuration '…' OK`) | R-4 shows the resulting snapshot |
| `admin_layer: {}` | ACCEPT | — |
| `disk_layer` with a real mounted directory | ACCEPT | — |
| `disk_layer` with an absent path | REJECT | `unable to add filesystem watch for file /srv/runtime/current: No such file or directory` |
| `rtds_layer` with an undefined cluster | REJECT | `envoy.config.core.v3.ApiConfigSource must have a statically defined non-EDS cluster: 'xds_cluster' does not exist…` |
| layer with no `name` | **REJECT** | `RuntimeLayerValidationError.Name: value length must be at least 1 characters` — `name` is PGV-required, min length 1 |
| layer with **no** oneof arm | **REJECT** | `field: "layer_specifier", reason: is required` |
| layer with **two** oneof arms | **REJECT** | `'admin_layer' has already been set (either directly or as part of a oneof)` |
| **duplicate** layer `name` | **REJECT** | `Duplicate layer name: dup` — a bare string error at a POST-PGV stage, not a PGV wrapper |
| unknown arm (`bogus_layer`) | REJECT | `no such field: 'bogus_layer'` |
| `layers: []` | **ACCEPT** | but see R-5 |
| `layered_runtime: {}` | **ACCEPT** | but see R-5 |

**No validate-vs-boot divergence was found.** Every construct accepted by
`--mode validate` also booted, and every construct rejected by validate failed at
boot with the identical message text (only the log-prefix source location
differs). Note that for this subsystem `--mode validate` is **not** a pure schema
check: it touches the filesystem for `disk_layer` and resolves the `rtds_layer`
cluster reference.

### R-4 — LIVE-ENVOY: the exact `GET /runtime` response

Booted for real (port-mapped, not validate mode) with a static layer carrying a
bool, an integer, a string and a nested map:

```
$ curl -si http://127.0.0.1:<admin>/runtime
HTTP/1.1 200 OK
content-type: application/json
cache-control: no-cache, max-age=0
x-content-type-options: nosniff
date: …
server: envoy
transfer-encoding: chunked

{
 "entries": {
  "envoy.reloadable_features.some_flag": { "layer_values": ["true"],  "final_value": "true"  },
  "my.numeric.key":                      { "layer_values": ["42"],    "final_value": "42"    },
  "my.nested.sub_key":                   { "layer_values": ["v"],     "final_value": "v"     },
  "my.string.key":                       { "final_value": "hello",    "layer_values": ["hello"] }
 },
 "layers": [ "static_layer_0" ]
}
```

The derived rules the implementation must encode:

1. Top-level shape is exactly two keys: **`entries`** (a JSON object) and
   **`layers`** (a JSON array of layer NAME strings, in config order).
2. Each `entries` value is an object with **`final_value`** (string) and
   **`layer_values`** (array of strings, **one slot per configured layer**, `""`
   where the key is absent from that layer — measured directly at R-7).
3. **All values are stringified.** `true` → `"true"`, `42` → `"42"`, `hello` →
   `"hello"`.
4. **Nested maps FLATTEN to dotted keys.** `my.nested: { sub_key: v }` appears
   as `my.nested.sub_key`; the intermediate `my.nested` does **not** appear.
5. `content-type` is `application/json`. There is **no `format` query
   parameter** — `?format=text` and any unknown parameter are silently ignored
   and still return the same JSON (200).

### R-5 — LIVE-ENVOY: the empty-string layer name, an edge with no analogue elsewhere in the project

- **No `layered_runtime` block at all** → `{"entries": {}, "layers": []}`, and
  `runtime.num_layers: 0`.
- **`layered_runtime: {}` or `layers: []`** → `{"entries": {}, "layers": [""]}`,
  and `runtime.num_layers: 1`. Envoy synthesizes **one** layer whose name is the
  **empty string**, and logs it as `runtime: layers: - admin_layer: {}`.

So an absent block and an empty block are **not** equivalent. This is a genuine
trap for a naive `Option<LayeredRuntime>` model that treats `Some(empty)` and
`None` alike, and it is directly witnessable.

### R-6 — LIVE-ENVOY, DECISIVE: Envoy does **NOT** pre-populate its own feature flags into the snapshot

This is the single measurement that makes the phase tractable, and it was
probed specifically because the opposite result would have killed the pick.

The v1.33.0 binary contains **89** distinct `envoy.reloadable_features.*`
identifiers:

```
$ docker run --rm --entrypoint /bin/sh envoyproxy/envoy:v1.33.0 -c \
    "grep -a -oE 'envoy\.reloadable_features\.[a-z0-9_]+' /usr/local/bin/envoy | sort -u | wc -l"
89
```

**None of them appears in `/runtime`.** Measured entry counts: the
no-`layered_runtime` baseline returns literally `{"entries":{},"layers":[]}`
(**0** entries, **0** layers); the four-key static layer returns **4** entries
and **1** layer. **The snapshot contains only what the config declares, plus
admin overrides.** envoy-rust therefore has to reproduce a bounded,
config-derived set — not Envoy's internal feature-flag registry.

One caveat measured alongside: setting a key under the
`envoy.reloadable_features.` prefix that the binary does not recognise emits a
non-fatal `envoy_bug` line on **stderr** (`Using a removed guard …`) at both
validate and boot, while still exiting 0. It is a log-only artifact and is not
part of the equivalence contract (§7.2 does not compare stderr). The fixture
avoids the prefix entirely.

### R-7 — LIVE-ENVOY: `layer_values` is one slot per layer, and `/runtime_modify` needs an `admin_layer`

With `static_layer_0` **plus** `admin_layer_0` configured, every entry's
`layer_values` becomes length **2**, with `""` in the slot where the key is
absent:

```json
"my.string.key": { "layer_values": ["hello", ""], "final_value": "hello" }
```

`POST /runtime_modify?my.string.key=changed&brand.new.key=7` then returns
`200` / `OK` (`text/plain`) and yields:

```json
"my.string.key":  { "layer_values": ["hello", "changed"], "final_value": "changed" }
"brand.new.key":  { "layer_values": ["", "7"],            "final_value": "7"       }
```

Without an `admin_layer` the same POST returns **503** with the body
`No admin layer specified`; a `GET` returns **405** `Method GET not allowed,
POST required.`. Each successful modify re-runs the whole runtime load, so
`runtime.load_success` and `runtime.override_dir_not_exists` **increment on
every POST**. All of this is measured and recorded here so a later slice need
not re-derive it — **`admin_layer` and `/runtime_modify` are OUT OF SCOPE for
this phase** (§5).

### R-8 — LIVE-ENVOY: the nine `runtime.*` stats, present even with no config

```
runtime.admin_overrides_active: 0
runtime.deprecated_feature_seen_since_process_start: 0
runtime.deprecated_feature_use: 0
runtime.load_error: 0
runtime.load_success: 1
runtime.num_keys: 4          # 0 on the no-layered_runtime baseline
runtime.num_layers: 1        # 0 on the no-layered_runtime baseline
runtime.override_dir_exists: 0
runtime.override_dir_not_exists: 1
```

**All nine exist unconditionally**, including on a config with no
`layered_runtime` block at all. Only `num_keys` and `num_layers` track the
config; `load_success: 1` and `override_dir_not_exists: 1` fire even on the
baseline.

**TREE corroboration, and a mis-filing worth correcting.** Those exact nine
names are ALREADY enumerated in the tree — as *envoy-only*. Fixture `0011`'s
`expectations.yaml:234-242` lists `envoy_runtime_admin_overrides_active`,
`…_deprecated_feature_seen_since_process_start`, `…_deprecated_feature_use`,
`…_load_error`, `…_load_success`, `…_num_keys`, `…_num_layers`,
`…_override_dir_exists`, `…_override_dir_not_exists` under
`allowlist_envoy_only`, and its `README.md:55` explains them as
*"`runtime.*` (9) — RTDS runtime layer. Deferred to the xDS family."* Two
consequences:

- The live measurement at R-8 and a two-year-old fixture allow-list agree on the
  name set exactly — independent corroboration of the nine.
- **They were mis-filed.** These belong to the Runtime family, not the xDS
  family; D5 retires the deferral.

**Fixture `0011` needs NO edit when D5 lands**, and this was verified by reading
the rule rather than assumed: `BodyRule::PrometheusExposition`
(`tests/differential/src/lib.rs:7068-7095`) computes
`envoy_names.difference(&rust_names)` and filters that difference by the
allow-list. Once envoy-rust emits the nine they leave the difference entirely
and land in the intersection, so the nine allow-list entries simply stop
matching anything. An unused entry is never an error. Deleting them is optional
tightening for a later session, not an obligation of this phase — and `0011` is
a landed fixture that this phase does not touch.

### R-9 — TREE, DECISIVE: the measured JSON key-order nondeterminism is NEUTRALIZED by the existing harness

Six consecutive `GET /runtime` requests against a single unchanged Envoy process
produced **five distinct md5 sums** — both the key order inside `entries` and
the field order inside each entry object shuffle per request. A byte-exact body
compare is therefore impossible.

**It does not matter, and this was verified in the harness source rather than
assumed.** `BodyRule::JsonShape` (`tests/differential/src/lib.rs:818`) is
evaluated at `lib.rs:7153-7215`: it `serde_json::from_slice`s **both** bodies
into `serde_json::Value`, walks a dotted path on each side, and compares
`serde_json::to_string(sub)` against `expected` — asserting `envoy == expected`
**and** `rust == expected`, so cross-side equality follows by transitivity. It
then diffs the top-level key sets modulo per-side allow-lists.

The ordering is canonical because **`preserve_order` is not enabled anywhere**:

```
$ grep -rn 'serde_json' --include=Cargo.toml .   # 5 hits, all bare `serde_json = "1"`
$ grep -rn 'preserve_order' --include=Cargo.* .  # 0 hits
```

Without that feature `serde_json::Map` is a `BTreeMap`, so parsing and
re-serialising both bodies sorts the keys identically on both sides.
**A shuffled `/runtime` response and a sorted one produce the same
`to_string`.** The nondeterminism is fully absorbed.

### R-10 — TREE: zero new harness machinery is required

- `Driver::AdminScrape { scrapes: Vec<AdminScrapeCase>, … }` already exists
  (`tests/differential/src/lib.rs:346`), and `AdminScrapeCase` (`:555`) carries
  exactly `{ path, expected_status, expected_content_type, expected_body_rule }`
  — everything a `/runtime` scrape needs, with **multiple cases per fixture**.
- **Seven** existing fixtures already drive `admin_scrape` + `json_shape`
  (`0011`, `0014`, `0015`, `0026`, `0027`, `0028`, `0029`), so the pattern is
  proven, not projected.
- Because only ONE `required_subtree` is permitted per rule, the fixture uses
  **two scrapes of the same `/runtime` path** — one anchoring the whole
  `entries` object, one anchoring `layers`. No new rule variant, no new driver,
  no new expectation kind.

### R-11 — TREE: envoy-rust has NO runtime subsystem, and `layered_runtime` is boot-fatal today

```
$ git grep -c 'layered_runtime|static_layer|LayeredRuntime|runtime_fraction' -- ':(exclude).claude'
0 files
```

Zero hits for `layered_runtime`, `static_layer`, `LayeredRuntime` and
`runtime_fraction` across the entire tracked tree. There is no `envoy-runtime`
crate. A config carrying `layered_runtime:` is rejected at boot today.

### R-12 — TREE: two existing config types already carry an INERT or REJECTED `runtime_key`, and both name this phase's absence as the reason

- **`RuntimeUInt32`** (`crates/envoy-config/src/bootstrap.rs:849`), used by the
  access-log `status_code_filter`. Its own doc comment states the comparison
  *"always uses `default_value`"* — `runtime_key` is **REQUIRED, parsed, and
  silently INERT**. ADR-0141 PV-4 records the reasoning and notes there was
  *"NO RTDS subsystem, and NO runtime-override consumer anywhere."*
- **`RuntimeFractionalPercent`** (`bootstrap.rs:1375`), used by the CSRF
  filter's required `filter_enabled`. Its doc comment states *"a present
  `runtime_key` is rejected (no RTDS runtime layer — ADR-0061 L6)."*

**This phase is the named unblocker for both**, but it does **not** wire either
of them (§5) — that is the consumer slice, and mixing it in would put a
behaviour change to two shipped filters inside a foundation phase.

### R-13 — TREE: the admin endpoint seam is a 10-arm string match

`AdminEndpoint::from_path` (`crates/envoy-admin/src/endpoint.rs:102-117`) strips
the query string and matches exactly ten paths — `/ready`, `/stats`,
`/stats/prometheus`, `/config_dump`, `/server_info`, `/clusters`, `/listeners`,
`/drain_listeners`, `/healthcheck/fail`, `/healthcheck/ok` — returning `None`
(rendered as a 404 by `render_404()` at `:982`) for anything else. Adding
`/runtime` is one enum variant, one match arm, one method declaration and one
handler. `envoy-admin` already depends on `serde_json`
(`crates/envoy-admin/src/…`, `Cargo.toml:16`).

### R-14 — TREE: stat registration is a one-liner

`StatsRegistry::register_counter(&str)` and `register_gauge(&str)`
(`crates/envoy-stats/src/registry.rs:45` and `:69`) take a dotted name string
directly; `crates/envoy-bin/src/network_rbac.rs:58-61` is the smallest existing
example. The nine `runtime.*` stats cost nine calls plus their wiring.

### R-15 — TREE: the standing censuses, RE-DERIVED at this session

**86** fixture directories under `tests/fixtures/` (highest `0086`), **86**
differential test files under `tests/differential/tests/`, a **21**-line
`tests/conformance/h2spec/known-failures.txt`, **5** fuzz targets spanning
**five** crates, a **3**-entry `HEADER_ALLOW_LIST`
(`tests/differential/src/lib.rs:1177-1181`), **107** ROADMAP rows all `done`,
ledger head **ADR-0170** / next free **ADR-0171**. The new fixture is therefore
**`0087`** and the new row is **`108`**.

Also re-derived: **125** `ConfigError` variants, counted over the enum's exact
span `crates/envoy-config/src/lib.rs:74-1011` — **not** by the naive
`grep -c '^\s*[A-Z][A-Za-z]* {'` over the whole file, which returns a believable
but wrong `104`. And **14** workspace crates, with **no** `envoy-runtime` among
them.

### R-16 — TREE: four row-less phase directories already claim numbers 59/60/61/62, and `61` COLLIDES

`docs/envoy-rust/phases/` holds **111** directories. Eighteen have no
`PLAN.md`: eleven are split parents holding `SPEC.md` only (correct, §6.2 step
1), and **four are abandoned proposals from a parallel perf workstream** that
carry a `SPEC.md` plus an unlanded `ADR-NNNN-DRAFT.md` and have **no ROADMAP
row at all** — `59-perf-h1-hot-path-alloc-trims`,
`60-perf-h1-vectored-response-write`, `61-perf-listener-so-reuseport`,
`62-upstream-idle-timeout-config`. Note that `61` is a **directory-name
collision**: `61-perf-listener-so-reuseport` sits alongside the real, landed
`61-accesslog-h2-urx-retry-exhausted`.

Recorded for two reasons: a future session resolving "does phase NN's directory
exist?" by numeric prefix will get two answers for `61`; and these four are
**not** unexpected state under §1 Step E — `STATE.md` is authoritative and names
no active phase, and none of the four has a ROADMAP row, so none is in the §5
state machine. **They are not touched by this phase.**

### R-17 — TREE: FOUR places in the tree currently ASSERT the opposite posture, and the phase must reconcile them explicitly

This is the cost the pick most easily under-counts. The "there is no runtime
layer" stance is not merely an absence; it is written down and, in one case,
**pinned by a passing test**:

1. `crates/envoy-http1/src/hcm.rs:5641` — a test literally named
   **`runtime_key_is_rtds_inert`**, pinning that two bootstraps differing only
   in `runtime_key` behave identically.
2. `crates/envoy-config/src/lib.rs` — `ConfigError::UnsupportedRuntimeKeyedCsrfFilterEnabled`.
3. `crates/envoy-config/src/bootstrap.rs` — `validate_csrf_config`'s
   `if fe.runtime_key.is_some() { return Err(…) }` reject.
4. `crates/envoy-config/src/bootstrap.rs:843-852` — `RuntimeUInt32`'s
   "RTDS-inert here (no runtime subsystem)" doc contract.

**None of the four is wrong after this phase, and none is edited by it** — the
phase builds a `static_layer` store, not RTDS, and it deliberately does not wire
either consumer (§5 items 4-5). But the SPEC must say so out loud, because a
reviewer meeting `runtime_key_is_rtds_inert` in a phase titled "runtime" will
otherwise read it as a contradiction. The state-2 PLAN-write must decide whether
to narrow their wording from "no runtime subsystem" to "no runtime **consumer**
for this key", and record the decision either way.

### R-18 — TREE: the `expected_stats` zero-value vacuous-pass trap

`tests/differential/src/lib.rs:4500-4507` documents that `scrape_admin_stat`
returns `Ok(0)` for an **unregistered** stat name. A `value: 0` expectation
therefore passes even if envoy-rust never registered the stat at all. Any stat
assertion this phase writes must be **non-zero** to be a witness —
`runtime.num_layers`, `runtime.num_keys` and `runtime.load_success` all qualify
on the fixture's config; `admin_overrides_active`, `load_error`,
`override_dir_exists` and the two `deprecated_feature_*` counters are all `0`
and therefore **cannot** be witnessed this way.

---

## §2. Why this surface — the cheapest-strong-differential argument

### 2.1 It opens a zero-row family, which is the one thing that moves the stop condition

All **107** ROADMAP rows are `done`, so stop-condition leg (i) is TRUE for the
first time in the project's history. It is **not** mission-complete: ADR-0167
DECISION 2 settled that a rows-`done` census measures the rows that EXIST, not
the surface that remains, because `ROADMAP.md:58` states a feature family
*"becomes one or more concrete phase rows when it enters `in-progress`."* Leg
(iii) — four zero-row families — is what still decides it. This phase takes
`### Runtime + hot restart family` from **0 rows to 1**, and per R-2 it does so
by inserting under that heading rather than appending at EOF, which is the only
placement that actually moves the census.

Of the four zero-row families, Runtime is the only one reachable at this cost.
HTTP/3 + QUIC needs the `quinn` transport plus an `h3spec` gate; gRPC needs a
real gRPC data path; the WASM host is explicitly *"its own multi-phase
sub-project."* Runtime's opener needs **no new dependency at all**.

### 2.2 Backend-free, and therefore fully verifiable on this development host

The recon config declared **zero clusters** and validated OK; the fixture drives
the admin listener only, with `pre_requests: []`. That matters concretely:
backend-routing fixtures go RED on this host because it routes the backend via
`192.168.65.2` rather than the allow-listed `192.168.65.254`/`172.17.0.1`, so
for those CI is the only authority. A cluster-free fixture is verifiable
locally, which is the same property that made phase 76 land cleanly.

### 2.3 Zero new harness machinery

R-9 and R-10 together: the existing `Driver::AdminScrape` + `BodyRule::JsonShape`
witness the whole snapshot, and the measured response nondeterminism is absorbed
by `serde_json`'s canonical `BTreeMap` ordering rather than papered over with an
allow-list. This is the phase-76 property — a strong differential that costs the
harness nothing.

### 2.4 The differential is strong, not thin

`required_subtree` with `path: "entries"` compares the **entire** runtime
snapshot object on both sides against one expected value. That is not a spot
check on one field; it is the whole feature's output surface, order-insensitively
byte-equivalent. A second scrape anchors `layers`.

### 2.5 It discharges a recorded blocker rather than inventing a new surface

R-12: two shipped config types carry a `runtime_key` that is inert or rejected
*because* there is no runtime layer, and both say so in their own doc comments.
This phase builds the thing they name.

### 2.6 Deterministic, with no timing, concurrency or crypto

Every measured cell is a static config value rendered into a JSON snapshot. No
clock, no ordering across connections, no fractional sampling in scope.

### 2.7 Rejected alternatives — each with the measurement that decided it

**(a) `CF-76-1` — the query-strip-before-route-matching divergence.** The
strongest competitor, and the one the inherited handoff named first. Upstream
strips the query before route path matching (`match: { path: "/exact" }` matches
`/exact?q=1`, but not `/exact%3Fq=1`); envoy-rust compares the raw target. It
was re-censused in full at this session, and the census both **strengthens** and
**re-prices** it:

- The fix site really is one 12-line pure function — `route_matches`
  (`crates/envoy-http1/src/hcm.rs:2182-2193`) — with exactly two production call
  sites, and HTTP/2 genuinely inherits it with **zero** `:path` pre-processing.
- **NEWLY MEASURED, and absent from the CF-76-1 record: there is a SECOND,
  independent matcher with the same bug.** `route_match_matches`
  (`crates/envoy-filter/src/jwt_authn.rs:173-186`) is a hand-copy of
  `route_matches` over the same `RouteMatch` type, driving jwt_authn's
  `rules[]`. Fixing only the HCM would leave two matchers in one request
  disagreeing — strictly worse than the uniform bug.
- **NEWLY MEASURED: three separate query-strip implementations already exist**
  and none is shared — `crates/envoy-filter/src/rbac.rs:73`,
  `crates/envoy-admin/src/endpoint.rs:101`, and `plan_redirect`'s own
  `split_once('?')` at `hcm.rs:2278`. A fix either adds a fourth copy or
  consolidates across four files.
- **NEWLY MEASURED, and it de-risks the phase: `plan_redirect` is already
  query-strip-correct.** It splits on `'?'` itself and computes `matched_len`
  against the stripped path, so the `path:`-route-plus-query composition the fix
  would newly enable is already handled and already unit-pinned. Phase 76
  accidentally pre-paid part of CF-76-1's cost.
- **The "~4 probes" sizing in the record is a material under-count.** The
  measured cells alone are `/exact`, `/exact?q=1`, `/exact?`, `/exact%3Fq=1`;
  a credible fixture must also cover the `prefix:` side and — decisively — the
  two *non-regression* witnesses (the query must still appear in
  `%REQ(:PATH)%` and on the forwarded upstream target), which need an
  access-log-observing driver rather than the backend-free `Http1ProbeList`.
  Realistically 8-12 probes, ≈900-1100 net LoC.

**Rejected, narrowly, on the zero-row-family criterion, not on cost** — the two
are the same size. CF-76-1 lights up no new family and would leave leg (iii) at
four. Its record is materially improved here (the second matcher, the three
strip copies, the `plan_redirect` freebie, the honest probe count), which
strictly improves the next session's position on it, exactly as ADR-0168
DECISION 5 did for `CF-75-2`. **It remains the strongest non-family-opening
candidate.**

**(b) The other three zero-row families.** HTTP/3 + QUIC needs the `quinn`
transport plus an `h3spec` gate; gRPC needs a real gRPC data path (bridge /
gRPC-Web / JSON transcoding); the WASM host is described in `ROADMAP.md:185` as
*"its own multi-phase sub-project."* Each is a multi-phase programme, and none
is a cheapest-strong-differential opener. Runtime is the only one of the four
whose opener needs **no new dependency at all**.

**(c) `CF-75-2` — the duplicate-header comma-join.** Fully measured at ADR-0168
DECISION 5 (single comma, no following space, OWS-trimmed, seen by every value
mode, preceding the numeric parse, `cookie` not special-cased) and single-site
at `crates/envoy-config/src/matcher.rs:40-43`. Rejected for the reason ADR-0168
already gave and which still holds: it lights up no new config surface, and it
would be the fifth phase in a row on the `HeaderMatcher` surface.

**(d) `CF-75-6` + `CF-75-3` — the test-isolation carry-forwards.** Precisely
sized already (four files, 17 `assert_fatal_startup` call sites plus five
same-class sites; a one-line `--no-fail-fast` at `.github/workflows/ci.yml:67`).
Rejected on the same ground as at the phase-76 pick: **it lights up no
differential fixture at all**, so it scores lowest on the cheapest-strong-
differential bar. It remains the strongest repo-health candidate.

**(e) The redirect follow-ups named by `REVIEW-2.md`.** `M2-1` (the
`regex_rewrite`-inside-`redirect` phase, which must extend
`validate_redirect_oneofs`'s doc *before* adding a third `return Err` or it
reproduces Issue I-1 exactly), `M2-2` and `M2-3`. Real and well-scoped, but they
are a fourth consecutive phase on the redirect surface and none opens a family.

**(f) `envoy.filters.network.sni_cluster`.** Still correctly rejected, on the
same measurement that rejected it at the phase-67 and phase-76 picks and that
nothing has changed: it needs a `tls_inspector` **listener**-filter subsystem
envoy-rust wholly lacks (`Listener.listener_filters` is parse-and-ignore).

**(g) Non-deterministic LB (`least_request` / `random`).** Still blocked behind
a contract-relaxation ADR, because §7.2 requires stats "values exact on
deterministic flows" and these flows are not deterministic. That ADR remains a
legitimate small phase.

---

## §3. Scope — what this phase builds

### D1 — `layered_runtime` config schema (`crates/envoy-config`)

- `Bootstrap.layered_runtime: Option<LayeredRuntime>` — **`Option`, and the
  distinction is load-bearing per R-5**: absent ≠ present-but-empty.
- `LayeredRuntime { #[serde(default)] layers: Vec<RuntimeLayer> }`.
- `RuntimeLayer { name: String, static_layer: StaticLayerValues }` with
  `#[serde(deny_unknown_fields)]`, modelled so exactly one arm can be set.
- Static-layer values are a recursive scalar-or-map value type
  (bool / integer / float / string / nested map).
- `Serialize` arms to keep the `/config_dump` cascade whole.

### D2 — validators + `ConfigError` variants (`crates/envoy-config`)

Reject-direction parity for the rules measured at R-3:

1. empty or absent layer `name` (upstream: PGV min length 1);
2. **duplicate** layer `name` (upstream: `Duplicate layer name: <n>`);
3. no oneof arm set (upstream: `layer_specifier … is required`);
4. more than one oneof arm set.

Plus the **fail-loud rejection** of the three out-of-scope arms `disk_layer`,
`rtds_layer` and `admin_layer`, per the ADR-0049 posture. This is a deliberate,
**recorded reject-direction gap**: upstream accepts all three and envoy-rust
will not. It is the same disposition ADR-0168 DECISION 3 took for
`regex_rewrite` inside `redirect`, and it is banked as a carry-forward at §6
rather than vaguely deferred (§6.3).

Error **text** is not part of the equivalence contract — only the verdict.

### D3 — the runtime snapshot

A small, self-contained store that turns the parsed layers into the R-4 shape:

- flatten nested maps to dotted keys (`my.nested.sub_key`);
- stringify every scalar (`true`, `42`, `hello`);
- one `layer_values` slot per configured layer, `""` where absent (R-7);
- `final_value` = the last non-empty slot (last layer wins).

**Home — MEASURED, and the answer is "no new crate".** The workspace has 14
crates and **no `envoy-runtime`**. `envoy-config` is already a dependency of
both `envoy-http1` (the eventual route-match consumer) and `envoy-admin` (the
`/runtime` renderer), so hosting the store there adds **no new dependency edge
and creates no cycle**. A new leaf crate is possible but would need edges into
config, http1, admin and bin for no measured benefit, and would have to respect
the ADR-0150 seam discipline. The state-2 PLAN-write owns the final call (V-3),
but the default is a module inside `envoy-config`.

### D4 — admin `GET /runtime` (`crates/envoy-admin`)

The **eleventh** endpoint. Four mechanical edits in
`crates/envoy-admin/src/endpoint.rs` — a variant on `AdminEndpoint` (`:9`), an
arm in `from_path` (`:103-117`), an arm in `allowed_method` (`:124-135`), and an
arm in `render_with` (`:165`) — plus the renderer. **`render_with`'s match has
no wildcard arm, so the compiler forces the new arm**; that is a genuine
forcing function and it should be relied on rather than duplicated by a grep.

The body is a `#[derive(Serialize)]` struct handed to the existing
`json_pretty_200` helper (`endpoint.rs:254-263`), which already emits
`application/json` — **zero new response plumbing**. Note it uses
`serde_json::to_vec_pretty` and emits no `content-length`; upstream's `/runtime`
is likewise pretty-printed and chunked (R-4). Byte-equality of whitespace is
*not* relied on — `JsonShape` parses both sides (R-9).

Two convention-only tests enumerate the endpoint set and want a new row:
`get_known_path_returns_endpoint` (`endpoint.rs:2332-2366`) and
`each_endpoint_declares_its_allowed_method` (`:2413-2422`). Neither is
compile-forcing, so they must be updated deliberately.

### D5 — the nine `runtime.*` stats

All nine from R-8, registered unconditionally: nine
`register_counter`/`register_gauge` calls against the flat
`BTreeMap`-backed registry, whose `is_valid_name`
(`crates/envoy-stats/src/registry.rs:100-116`) explicitly permits dots. This is
the cheapest item in the phase.

`num_keys` and `num_layers` are the two that track config; the rest are the
constant frame Envoy also emits on a bare config. **Per R-18, only the non-zero
ones can be witnessed by `expected_stats`** — a `value: 0` assertion passes
vacuously against an unregistered name, so the seven that are `0` on the
fixture's config must be witnessed by the `/stats` name-set comparison instead,
not by a value assertion.

### D6 — differential fixture `0087-runtime-static-layer`

`Driver::AdminScrape`, `pre_requests: []`, **zero clusters, no backend**. Two
scrapes of `/runtime`:

- one with `required_subtree { path: "entries", expected: <the whole object> }`;
- one with `required_subtree { path: "layers",  expected: ["<name>"] }`;

both with `required_keys: ["entries", "layers"]` and empty per-side allow-lists
(the intent is that **nothing** needs allow-listing — if something does, that is
a finding, not a knob to turn).

The static layer carries a bool, an integer, a string and a nested map, so R-4's
four stringification and flattening rules are each witnessed. It deliberately
does **not** use the `envoy.reloadable_features.` prefix (R-6's `envoy_bug`
stderr artifact).

### D7 — in-process backstops

The absent-vs-empty `layers` distinction (R-5), the four reject-direction rules
(D2), the flattening and stringification rules, last-layer-wins precedence, and
the `""`-slot rule.

### D8 — fuzz (§7.4 disposition)

**No new fuzz target.** `layered_runtime` is parsed by the existing
`parse_bootstrap` target, which already covers the whole `Bootstrap` surface, so
gate (d) is satisfied by a pre-existing target — the phase-66/67/76 disposition.
A corpus seed is added instead, and it **needs an explicit `!`-un-ignore line**
or it is silently untracked; the state-3 session must prove it tracked with
`git ls-files`. **`ci.yml` needs no new step**, and the state-4 session must
RECORD that explicitly rather than skip gate (d) silently.

### D9 — `BEHAVIOR_CONTRACT.md`

A new `## Runtime` section carrying the R-3 layer grammar, the R-4 snapshot
rules, the R-5 absent-vs-empty edge and the R-6 no-pre-population fact; one new
row in `## Admin endpoint body shapes` for `/runtime`; and the nine stats in
`## Stat-name mapping`.

---

## §4. Differential surface at phase end

- **NEW** fixture `0087-runtime-static-layer` green — the whole `entries`
  object and the `layers` array equal across both proxies.
- All **86** pre-existing fixtures still green. This phase is additive: no
  existing config carries `layered_runtime`, so the parse path is inert for
  every one of them, and `/runtime` is a new path that no existing fixture
  scrapes.
- `h2spec` unchanged; `known-failures.txt` untouched at 21 lines.

---

## §5. Non-goals — do NOT widen into these

Each is recorded here rather than left vague (§6.3 anti-pattern).

1. **`disk_layer`** — its runtime semantics are UNMEASURED, and this host has
   virtiofs with no inotify, so a disk-reload path would be CI-authoritative
   only. Rejected loudly (D2).
2. **`rtds_layer`** — needs an xDS cluster and belongs with the xDS family.
   Rejected loudly.
3. **`admin_layer` + `POST /runtime_modify`** — fully measured at R-7 so a later
   slice inherits the measurement, but it is a state-**mutating** admin endpoint
   and a second layer's precedence semantics. Rejected loudly.
4. **`runtime_fraction` route gating.** Measured working upstream: a route
   `match.runtime_fraction { default_value, runtime_key }` over a
   `direct_response` flips between two bodies with no cluster and no backend.
   It is an attractive **consumer** slice — but it is a route-matching change,
   and this phase is the producer.
5. **Honoring `runtime_key` in the two existing consumers** (`RuntimeUInt32` for
   `status_code_filter`, `RuntimeFractionalPercent` for CSRF, R-12). That
   changes the behaviour of two shipped filters and belongs with the consumer
   slice.
6. **FractionalPercent-shaped struct values.** MEASURED trap: a nested map
   containing `numerator` is **not** flattened like every other nested map — it
   is kept as a single key whose value is the protobuf **text-format** dump of
   the Struct, complete with literal `\n`s. Matching that byte-for-byte means
   reimplementing protobuf `DebugString`. Out of scope; the fixture uses no such
   value, and D2 should decide at PLAN-write whether to reject it loudly or
   leave it unmodelled.
7. **Hot restart** (`/hot_restart_version`, `--restart-epoch`) — the other half
   of the family heading, entirely UNMEASURED, and a separate phase.
8. **The `runtime.deprecated_feature_use` counters actually incrementing** — the
   frame is emitted (D5) but no deprecated-field detector is built.

---

## §6. Carry-forwards

### 6.1 OPENED by this phase

- **CF-108-1 — the three loudly-rejected layer arms.** `disk_layer`,
  `rtds_layer` and `admin_layer` are accepted by upstream and boot-fatal here: a
  recorded reject-direction divergence, differentially unobservable (a rejected
  config never reaches the wire). Owner: whichever phase lands each arm.
- **CF-108-2 — `/runtime_modify` is absent.** Upstream serves it (R-7);
  envoy-rust will 404. Owner: the `admin_layer` slice.
- **CF-108-3 — the FractionalPercent text-format rendering** (§5 item 6).

### 6.2 ADVANCED, not consumed

**R-12's two inert/rejected `runtime_key` fields.** This phase builds the store
they need but does not read from it. They stay open, with the blocker removed.

**`CF-76-1` — measurement debt discharged without touching the carry-forward.**
`CF-76-1` stays **OPEN and out of scope**, but §2.7(a) banks four things its
record did not contain, all measured here: the **second matcher**
(`route_match_matches`, `crates/envoy-filter/src/jwt_authn.rs:173-186`); the
**three pre-existing query-strip implementations** (`rbac.rs:73`,
`endpoint.rs:101`, `hcm.rs:2278`); that **`plan_redirect` is already
query-strip-correct**, so phase 76 pre-paid part of the fix; and that the
record's "**~4 probes**" sizing is an under-count — 8-12 probes and ≈900-1100
net LoC, because the two *non-regression* witnesses (query still logged, query
still forwarded) need an access-log-observing driver. This is the ADR-0168
DECISION 5 pattern: improve the next session's position on a candidate you did
not pick.

Two further measured facts about `CF-76-1` worth banking, both of which lower
its risk: **no existing test is at risk** — the intersection of {test exercising
route matching} ∩ {target containing `?`} is empty, and all five exact-`path:`
route constructions in the Rust tree are probed with query-free targets — and
**no existing fixture depends on the current behaviour**: exactly six
query-bearing fixture probes exist (`0029` ×1 on the admin listener, `0045` ×1,
`0086` ×4) and **none lands on an exact-`path:` route**.

### 6.3 Consumed

None. This phase consumes no carry-forward, and it fixes none of the banked
Minors or Nits from the `76.1`, `76.2` or `76.2` round-2 reviews (§6.3 — a phase
picks its scope; it does not clear a backlog).

---

## §7. PLAN-VERIFY items — re-confirm FRESH at the state-2 PLAN-write

Every one of these must be re-derived against the live tree and the live pinned
image. Do not inherit a number or a line anchor from this document.

- **V-1.** Re-derive every `file:line` anchor in §1 by TEXT. `bootstrap.rs` is
  ~21 000 lines and `tests/differential/src/lib.rs` ~10 900; both drift.
- **V-2.** Re-confirm R-9 by experiment, not by reading: parse a shuffled and a
  sorted copy of the same `/runtime` body through `serde_json` and assert the
  `to_string` forms are identical. **If this fails, the fixture design in D6
  collapses** — it is the load-bearing claim of the whole pick.
- **V-3.** Decide D3's home (module vs new `envoy-runtime` crate) and record the
  reasoning. A new crate needs `#![forbid(unsafe_code)]` (D-3.8) and a
  `Cargo.toml` workspace-members entry.
- **V-4.** Settle the static-layer value model against `serde_yaml` — YAML 1.1
  parses unquoted `y`/`n`/`on`/`off` as booleans, so a runtime key whose value
  is the string `"y"` is a live hazard. Probe what upstream renders for such a
  value before choosing the model.
- **V-5.** Confirm the fixture is genuinely host-clean. Fixture `0014`
  (`admin_config_dump_server_info`) is a KNOWN deterministic host-flake in the
  `192.168.65.2` bridge-IP family — but its failure is in `/clusters`
  backend-endpoint addresses. A cluster-free `/runtime` fixture should be
  unaffected; **run it locally and prove it**, do not assume.
- **V-6.** Re-derive the §6.1 estimate bottom-up (see §9) and own the split
  decision.
- **V-7.** Re-confirm that no existing fixture or test scrapes a path that would
  now resolve differently, and that adding a 404→200 path for `/runtime` breaks
  no existing admin assertion.
- **V-8.** Re-measure the exact `expected` JSON for D6 against the pinned image
  with the fixture's own config, and record the transcript. Do not transcribe
  §1's illustrative snapshot.
- **V-9.** Confirm `ADR-0171` is still the number this SPEC's pick landed under
  and that `ADR-0172` is still free (re-derive the head with
  `grep -o '^## ADR-[0-9]\{4\}' … | sort -t- -k2 -n | tail -1`; note
  `grep -c '^## ADR-'` over-counts by one because of the template near line 10,
  and the numbers are not contiguous).
- **V-10.** Decide and record the disposition of R-17's four anti-runtime
  assertions — in particular whether the passing test
  `runtime_key_is_rtds_inert` (`crates/envoy-http1/src/hcm.rs:5641`) keeps its
  name and its wording. Leaving it unmentioned is the failure mode.
- **V-11.** Re-confirm R-8's TREE corroboration: that fixture `0011` still
  allow-lists the nine `runtime.*` names as envoy-only, and re-read
  `BodyRule::PrometheusExposition` to confirm an unused allow-list entry is
  still not an error. **If that changed, `0011` goes RED when D5 lands** and the
  phase acquires a landed-fixture edit it does not currently plan for.

---

## §8. NOT MEASURED — stated explicitly per D-3.4

1. **`disk_layer` runtime semantics.** It validated against a real mounted
   directory but was never booted and `/runtime` never read with disk-sourced
   keys. Precedence between `symlink_root`/`subdirectory`/override, and the
   documented "disk values can be overridden but not deleted" claim, are
   unmeasured.
2. **`rtds_layer` end to end.** Never validated or booted successfully — both
   attempts failed on the cluster reference. Its `/runtime` representation and
   any `runtime.rtds.*` stats are unknown.
3. **`runtime.load_error`** — only ever observed at 0; no config was found that
   increments it.
4. **The `deprecated_feature_*` counters under a real deprecated field.**
5. **Whether setting a REAL reloadable feature flag changes behaviour.** A
   recognised flag appears cleanly in `/runtime`, but no gated code path was
   exercised.
6. **`/runtime` with three or more layers**, and the `layers` array ordering
   under stress — it matched config order in every probe, but only up to two
   layers.
7. **Very large snapshots** — no test of chunking or ordering at hundreds of
   keys.
8. **Hot restart**, entirely.

---

## §9. Size estimate and the §6.1 split gate

Bottom-up, non-test and test halves separately:

| Deliverable | non-test | test |
|---|---:|---:|
| D1 schema | ~90 | ~120 |
| D2 validators + `ConfigError` variants | ~110 | ~180 |
| D3 snapshot / flatten / precedence | ~140 | ~200 |
| D4 admin endpoint | ~90 | ~130 |
| D5 nine stats | ~60 | ~90 |
| D6 fixture `0087` (2 YAMLs + expectations + README + test file) | — | ~180 |
| D7 in-process backstops | — | ~140 |
| D8 corpus seed | ~15 | — |
| **Total** | **~505** | **~1040** |

**≈1545 net LoC** — **over the ~1500 gate**, and the calibration makes that
worse rather than better. Five recent landed phases, each measured at this
session with `git diff --numstat <base> <head> -- . ':(exclude)docs/'` (not
inherited — every figure below was reproduced on disk):

| phase | span | files | added | removed | **net** |
|---|---|---:|---:|---:|---:|
| `70` (access-log `status_code_filter`) | `b362bae..9b27d44` | 19 | 1811 | 91 | **1720** |
| `75.1` (HeaderMatcher engine) | `78c37a3..3f0ec89` | 14 | 1486 | 73 | **1413** |
| `75.2` (HeaderMatcher access-log witness) | `1bf256a..1014524` | 15 | 914 | 17 | **897** |
| `76.1` (redirect config surface) | `cf5cf85..9556b2c` | 5 | 793 | 19 | **774** |
| `76.2` (redirect runtime + fixture 0086) | `0ea2de1..a2ebc8a` | 12 | 1334 | 69 | **1265** |
| `76.2` **including its §5.2 re-entry** | `0ea2de1..a326c4e` | 13 | 1643 | 75 | **1568** |

Two lessons the table makes concrete. **`76.1` overran its PLAN projection of
≈515 by +50%.** And **`76.2` came in 4% UNDER its ≈1312 projection at state-3
close, then crossed the gate at 1568 once the review's fixes landed** — so a
projection must budget for the §5.2 re-entry, not just the happy path.

**The §6.1 split is therefore PROJECTED TO FIRE**, and **ADR-0172 is RESERVED**
for it. The natural cut follows the producer/observer seam and mirrors the
`05.1`/`07.1`/`12.1`/`14.1`/`23.1`/`25.1`/`76.1` foundation-slice precedent:

- **`108.1`** — D1 + D2 + D3 + D7's config-side backstops. The config surface
  and the snapshot, witnessed in-process, with regression-equivalence over all
  86 existing fixtures and **no new fixture**.
- **`108.2`** — D4 + D5 + D6 + D9 + the parent close. The admin endpoint, the
  stats, fixture `0087` and the contract section.

**The state-2 PLAN-write owns the decision and MUST re-derive the estimate**
(V-6). If it splits, §6.2 applies: create both sibling directories, redistribute
this SPEC, make the parent row `in-progress` with its `sub-phases` column
listing `108.1, 108.2`, point `STATE.md` at `108.1`, append the ADR, and **stop
without writing a `PLAN.md`** (§6.2 step 1).

---

## §10. Definition of done — the §7.5 gate, instantiated

- **(a)** fixture `0087-runtime-static-layer` green cross-proxy.
- **(b)** all 86 pre-existing fixtures still green.
- **(c)** `h2spec` unchanged and above threshold; `known-failures.txt` untouched
  at 21 lines. No H2 codec or framing change this phase.
- **(d)** no new fuzz target (D8); the pre-existing `parse_bootstrap` target
  runs clean on its short CI budget, and the new corpus seed is proven tracked
  via `git ls-files`. **Record this explicitly** — do not skip it silently.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo test --workspace` and `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

Standing constraints that bind this phase: never weaken a fixture; never trim
`known-failures.txt`; `#![forbid(unsafe_code)]` holds at every crate root
(D-3.8); no `ENVOY_TARGET.md` or `rust-toolchain.toml` change (D-3.7 / D-3.9);
ADR-0028 is not lifted; and no landed artifact of any closed phase is edited
(D-3.5).

---

## §11. Next state

**State 2 — the PLAN-write** (`superpowers:writing-plans`), a **separate
session** per §5.1 and ADR-0127. It re-confirms V-1…V-9 fresh, runs the §6.2
empirical reconciliation against `envoyproxy/envoy:v1.33.0`, and either writes
`PLAN.md` or executes the §6.1 split. It does **not** write code.
