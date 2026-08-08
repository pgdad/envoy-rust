# Sub-phase 108.2 — admin `GET /runtime` + the nine `runtime.*` stats + fixture `0087` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the observer half of the phase-108 runtime opener: the eleventh
admin endpoint `GET /runtime` (rendering the 108.1-landed
`RuntimeSnapshot::from_bootstrap` store), the nine `runtime.*` stats, the
backend-free CLUSTER-FREE differential fixture `0087-runtime-static-layer`, and
the `BEHAVIOR_CONTRACT.md` `## Runtime` section — closing parent phase `108` at
this slice's state-6.

**Architecture:** `AdminEndpoint::Runtime` is added to the existing exhaustive
dispatch (TWO compile-forcing sites: `allowed_method` and `render_with`, both
wildcard-free); the renderer computes the snapshot per request from the
handler's cached `Arc<Bootstrap>` via `RuntimeSnapshot::from_bootstrap` (never
`from_layers` — 108.1 REVIEW M-5) and serializes through the shared
`json_pretty_200` helper. The nine stats are registered unconditionally at
`envoy-bin` startup from the SAME entry point. The fixture drives TWO
`/runtime` scrapes through the existing `Driver::AdminScrape` +
`BodyRule::JsonShape`, plus nine bilateral stat assertions through a small
ADDITIVE harness extension (`expected_stats` on `Driver::AdminScrape`,
reusing `KeepAliveExpectedStat` + `assert_expected_stats_bilaterally`
verbatim).

**Tech Stack:** Rust (workspace toolchain pin), `serde`/`serde_json` (already
deps of `envoy-admin`), the existing `envoy-stats` flat registry, the existing
differential harness. **No new dependency, no new crate, no `Cargo.toml` edit.**

## Global Constraints

- Never weaken a fixture; never trim `known-failures.txt` (21 lines, ONE real entry).
- `#![forbid(unsafe_code)]` at every crate root (D-3.8); no `ENVOY_TARGET.md` /
  `rust-toolchain.toml` change (D-3.7/D-3.9); ADR-0028 is NOT lifted.
- No landed artifact of a closed phase is edited (D-3.5) — in particular
  fixture `0011` (its stale `runtime.*` prose is corrected in the D9 contract
  section, Task 5, NOT by editing the fixture), and the eleven "no runtime
  subsystem" assertions incl. the test `runtime_key_is_rtds_inert` keep name
  and wording (ADR-0172 DECISION 5 — still true: nothing READS the store for
  behavior; this slice only renders and counts it).
- `crates/envoy-config/src/runtime.rs` and the 108.1 schema are LANDED and
  REVIEWED — this slice calls them; it does not reshape them.
  `RuntimeSnapshot::from_bootstrap` must not change without updating
  `108.2/SPEC.md`.
- Fixture `0087` carries NO unquoted `y`/`n`/`on`/`off` (CF-108-4, ADR-0173
  DECISION 1), NO float except the Display-stable `1.5` (CF-108-5, ADR-0174),
  NO `numerator`-bearing nested map (CF-108-3), NO
  `envoy.reloadable_features.` prefix.
- Every task boundary: `cargo fmt --all -- --check` clean and
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  exit 0 — gate clippy evidence on the `Checking` LINE COUNT (a cached no-op
  prints ZERO lines and proves nothing).
- Commit after every task (small commits; message prefix `phase 108.2 task N:`).

---

## §0 — Measured evidence this plan rests on (all taken at the state-2 PLAN-write, 2026-08-08)

Every number below was MEASURED by the PLAN-write session — the upstream cells
against the pinned `envoyproxy/envoy:v1.33.0`
(digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`,
verified via `docker image inspect` before probing), the envoy-rust cells in a
scratch pre-flight worktree detached at `ced6802`. **A pre-flight's claim is
still a claim** — the implementing session re-establishes each one through the
task steps below; the point of this section is that no expected value in this
plan is a guess.

**§0.1 — The fixture's exact upstream `/runtime` transcript.** Booting upstream
with fixture `0087`'s exact `layered_runtime` block (Task 3's YAML, concrete
ports) and scraping `GET /runtime` twice: both responses **1508 bytes** with
**distinct md5s** (`3f25d0f0…` / `1c922fb9…`) — the per-request key-order
shuffle `BodyRule::JsonShape`'s canonical re-parse absorbs (ADR-0172 DECISION
3). Headers: `200`, `content-type: application/json`, `transfer-encoding:
chunked`. Canonically sorted, the body is EXACTLY the 14-entry object
transcribed into Task 3's `expectations.yaml` (every value a string;
`layers: ["base_layer","override_layer"]`).

**§0.2 — The nine stats on that same config** (`/stats`, upstream):

```
runtime.admin_overrides_active: 0
runtime.deprecated_feature_seen_since_process_start: 0
runtime.deprecated_feature_use: 0
runtime.load_error: 0
runtime.load_success: 1        <- stays 1 on TWO layers: loads, not layers
runtime.num_keys: 14           <- flattened LEAVES (13 declared + 1 override-only)
runtime.num_layers: 2
runtime.override_dir_exists: 0
runtime.override_dir_not_exists: 1
```

**Kinds** (upstream `/stats/prometheus` `# TYPE` lines): **gauges** =
`admin_overrides_active`, `deprecated_feature_seen_since_process_start`,
`num_keys`, `num_layers`; **counters** = `deprecated_feature_use`,
`load_error`, `load_success`, `override_dir_exists`,
`override_dir_not_exists`. (The harness's `PrometheusExposition` rule compares
NAME sets only, so the kind choice cannot red fixture `0011` — but Task 2
pins the measured kinds anyway; they are the documented surface.)
`GET /runtime_modify` re-confirmed **405** (CF-108-2, unconsumed).

**§0.3 — CF-108-5, ALL THREE AXES MEASURED (ADR-0174).** Upstream preserves
the **raw YAML source text** of float-shaped scalars — it does NOT format a
parsed `f64`:

| YAML source | upstream `/runtime` `final_value` | Rust `f64` `Display` would give |
|---|---|---|
| `1.5` | `"1.5"` | `"1.5"` ✅ agree |
| `0.5` | `"0.5"` | `"0.5"` ✅ agree |
| `1.0` | `"1.0"` | `"1"` ❌ |
| `1e6` | `"1e6"` | `"1000000"` ❌ |
| `-0.0` | `"-0.0"` | `"-0"` ❌ |
| `1e-7` | `"1e-7"` | `"0.0000001"` ❌ |
| `1e300` | `"1e300"` | `"1e300"` ✅ (coincidence of spelling) |
| `1.50` | `"1.50"` | `"1.5"` ❌ |
| `1E6` | `"1E6"` | `"1000000"` ❌ |
| `.5` | `".5"` | `"0.5"` ❌ |
| `5.0` | `"5.0"` | `"5"` ❌ |

The `1.50`/`1E6`/`.5` cells are the discriminator: no numeric formatter
reproduces them — upstream keeps the scalar's SOURCE TEXT. `serde_yaml`
destroys source text before any serde code runs (the same scanner property
that made CF-108-4 normalisation not implementable, ADR-0173 DECISION 1), so
matching upstream on non-Display-stable spellings is NOT IMPLEMENTABLE at the
`serde_yaml::Value` level. **Axis (ii),** `/config_dump`: upstream emits float
cells as SOURCE-TEXT STRINGS (`"1.5"`, `"1e6"`) where our `Serialize` emits
JSON NUMBERS — 108.1 REVIEW M-4 confirmed and now precisely characterized;
fixture `0087` does not scrape `/config_dump`, so it is differentially
unobserved and RECORDED (Task 1 pins our shape; Task 5 records the
divergence). **Axis (iii),** non-finite: `.nan`/`.inf`/`-.inf` boot CLEAN
upstream and render as source text (`".nan"`/`".inf"`/`"-.inf"`); envoy-rust
accepts them into `Float(f64)` and would render `"NaN"`/`"inf"`/`"-inf"`
(divergent — kept OUT of the fixture, recorded in Task 5).
**Consequence:** the fixture's ONLY float is `1.5` (measured-agreeing);
CF-108-5 CLOSES as measured-and-recorded (ADR-0174).

**§0.4 — The pre-flight end-to-end dry-run.** With all of this plan's code
applied in a scratch worktree: `cargo fmt --all -- --check` clean;
`cargo clippy --workspace --all-targets --all-features -- -D warnings` exit 0
(first full run **148 `Checking` lines**, one real error caught — see DD-7);
`cargo test -p envoy-admin` **103 passed** (baseline 97 + 6 new);
`envoy-bin` `runtime_stats` **2 passed**; `differential --lib` fixture-0087
parse test **1 passed**; and envoy-rust booted on the substituted fixture
config served `/runtime` **canonically identical to the §0.1 upstream
transcript** (`json.dumps(sort_keys=True)` equality) with all nine `/stats`
lines value-identical to §0.2. The harness accept-ready wait made the
data-plane listener NECESSARY (DD-3): a listener-free config times out
`run_fixture` before the driver arm runs.

**§0.5 — NOT MEASURED, deliberately (carried forward per 108.1 REVIEW §8
item 3; the fixture excludes all four):** a flattened-key collision inside one
layer (`a: {b: 1}` + `a.b: 2` — M-1: deterministic dotted-spelling-wins by
`BTreeMap` byte order on OUR side, upstream cell unmeasured); an empty nested
map as a VALUE (M-7 — our side yields no entry, upstream unmeasured);
empty/dot-bearing static-layer key SEGMENTS (N-5); an explicit-null
`static_layer:` arm (N-4). These stay in the NOT-MEASURED list (Task 5
records them in the contract section; the fixture README repeats them).

---

## File Structure

| File | Change | Task |
|---|---|---|
| `crates/envoy-admin/src/endpoint.rs` | `Runtime` variant + 3 dispatch arms + `RuntimeBody`/`RuntimeEntryBody` + `render_runtime` + 2 test-support consts + `mod runtime_tests` (6 tests) + 2 convention-test rows | 1 |
| `crates/envoy-bin/src/runtime_stats.rs` | NEW module: `register_runtime_stats` + 2 tests | 2 |
| `crates/envoy-bin/src/main.rs` | `mod runtime_stats;` + one call after `register_rds_stats` | 2 |
| `tests/differential/src/lib.rs` | `expected_stats` field on `Driver::AdminScrape` + arm wiring + STEP 3.5 call + fixture-0087 parse test | 3 |
| `tests/fixtures/0087-runtime-static-layer/{envoy.yaml, envoy-rust.yaml, expectations.yaml, README.md}` | NEW fixture (87th) | 3 (README in 5) |
| `tests/differential/tests/runtime_static_layer.rs` | NEW test binary (87th differential test file, 164th workspace test binary) | 4 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | NEW `## Runtime` section + one `## Admin endpoint body shapes` row + `## Stat-name mapping` entries | 5 |
| `docs/envoy-rust/phases/108.2-runtime-admin-endpoint-and-fixture/PROGRESS.md` | per-task log + gate records | 1-6 |

**No other file changes.** In particular: NO `Cargo.toml` (serde_json is
already an `envoy-admin` dep at `Cargo.toml:16`), NO `ci.yml` (no new fuzz
target — gate (d) is satisfied by the pre-existing `parse_bootstrap`
short-budget run; Task 6 RECORDS this explicitly), NO fuzz corpus change, NO
edit to `crates/envoy-config/` (the producer is landed; the M-6 Serialize pin
lives in `envoy-admin` where the real `/config_dump` cascade runs).

## Design decisions this plan settles (read before Task 1; banked in ADR-0174)

- **DD-1 — CF-108-5 is CLOSED as measured-and-recorded** (§0.3): envoy-rust
  renders floats via `f64 Display`; upstream preserves source text; the
  divergence is recorded, not chased (not implementable at the
  `serde_yaml::Value` level), and the fixture's only float is `1.5`.
- **DD-2 — M-5 disposition: route around.** Both the renderer (Task 1) and the
  stats registration (Task 2) call `RuntimeSnapshot::from_bootstrap` — the
  documented entry point that maintains the names/layers invariant internally
  — and never `from_layers` directly. The unenforced `pub` invariant is not
  consumed and not "fixed" (the producer is a landed, reviewed artifact).
- **DD-3 — the fixture carries a minimal ECHO data-plane listener** (fixture
  `0001`'s spelling), because `run_fixture` unconditionally waits for
  data-plane accept-ready on `{{PORT}}` before dispatching ANY driver arm
  (measured, §0.4). Still zero clusters, zero backends. The envoy-rust side
  uses the NAME-ONLY echo spelling (the `typed_config` `@type` for echo is not
  in envoy-rust's enum — the fixture-0001 precedent divergence, noted in the
  README).
- **DD-4 — the stats witness needs ONE additive harness field** —
  `expected_stats` on `Driver::AdminScrape` (`#[serde(default)]`, so all six
  existing AdminScrape fixtures parse unchanged), asserted by the EXISTING
  `assert_expected_stats_bilaterally` as STEP 3.5 of the arm. This deviates
  from ADR-0171's "zero new harness machinery" claim for the measured reason
  that AdminScrape has NO stat surface at all and SPEC §2's witness obligation
  requires one; the values are ABSOLUTE (not ADR-0131 deltas) because
  `runtime.*` stats are set once at startup and no readiness connect perturbs
  them.
- **DD-5 — M-6 disposition: the positive `Serialize` pin goes through the real
  `/config_dump` cascade** (Task 1's `config_dump_serializes_layered_runtime_positively`
  in `envoy-admin`): a `#[serde(skip)]` on `Bootstrap.layered_runtime` — or a
  broken `RuntimeValue` Serialize arm — now fails a test; the float cell pins
  our JSON-NUMBER shape deliberately as the recorded M-4 divergence; the
  absent direction pins `skip_serializing_if` (what keeps the 86 pre-existing
  `/config_dump` fixtures byte-identical).
- **DD-6 — stat kinds follow the measured upstream `# TYPE` lines** (§0.2):
  4 gauges / 5 counters. `num_keys`/`num_layers` are gauges SET from the
  snapshot; `load_success`/`override_dir_not_exists` are counters `.inc()`ed
  exactly once at registration.
- **DD-7 — the widened `run_admin_scrape_arm` carries
  `#[allow(clippy::too_many_arguments)]`** with a justification comment (the
  `AdminHandler::new` house precedent): the pre-flight's full clippy run
  errored on exactly this (the one defect it caught), and a params struct
  would obscure the field-to-arg correspondence the dispatch destructure makes
  obvious.
- **DD-8 — subtree paths never point INTO `entries`.** `walk_pointer` splits
  its dotted path on `.`, and every runtime key CONTAINS dots — a path like
  `entries.diff.int.key` would mis-walk five segments. The two scrapes anchor
  at the single-segment paths `entries` and `layers` only (the SPEC's design,
  now with the mechanical reason recorded).

### Task ordering note

Tasks 1 and 2 are independent of each other but both precede 3 (the fixture
asserts against both proxies' new surfaces). Task 3 (harness + fixture data)
precedes 4 (the fixture RUN). Task 5 (contract + README) and 6 (records +
sweep) close. The per-task-boundary clippy gate holds at every boundary here
(pre-flight-verified: no task leaves a dead item — Task 1's renderer is
reached via `render_with` immediately, Task 2's module is wired into `main.rs`
in the same task, Task 3's field is consumed by the arm in the same edit).

---

### Task 1: `AdminEndpoint::Runtime` — the eleventh endpoint (envoy-admin)

**Files:**
- Modify: `crates/envoy-admin/src/endpoint.rs` (anchors re-derived BY TEXT
  this session; the file is 3091 lines pre-task)

**Interfaces:**
- Consumes: `envoy_config::runtime::RuntimeSnapshot::from_bootstrap(&Bootstrap)`
  (landed, `crates/envoy-config/src/runtime.rs:131`), `handler.bootstrap()`
  (`pub(crate)`, `handler.rs:145`), `json_pretty_200` (`endpoint.rs:255`),
  test support: `handler_with_bootstrap`, `dump_value`, `TINY_BOOTSTRAP`.
- Produces: `AdminEndpoint::Runtime` (dispatched for `GET /runtime`),
  `render_runtime`, test-support consts `RUNTIME_TWO_LAYER_BOOTSTRAP` /
  `RUNTIME_SCALARS_BOOTSTRAP` (consumed by this task's tests only).

- [ ] **Step 1: Write the failing tests.** Append the two consts to
  `pub(crate) mod test_support`, immediately after `TINY_BOOTSTRAP`
  (whole-block insertion; note `test_support`'s `parse_bootstrap` bypasses the
  validator, so no `admin:` block is needed):

```rust
    /// 108.2 D4: the SPEC §2 N-6/N-7 two-layer transcript, MEASURED against
    /// envoyproxy/envoy:v1.33.0 (the four-cell precedence table also pinned
    /// at `envoy_config::runtime`'s `from_layers_reproduces_the_measured_two_layer_transcript`).
    pub(crate) const RUNTIME_TWO_LAYER_BOOTSTRAP: &str = "\
node:
  id: t
  cluster: c
static_resources:
  listeners: []
  clusters: []
layered_runtime:
  layers:
    - name: base_layer
      static_layer:
        shared.key: from_base
        only.in.base: base_val
        empty.in.override: real_value
    - name: override_layer
      static_layer:
        shared.key: from_override
        only.in.override: over_val
        empty.in.override: \"\"
";

    /// 108.2: one layer carrying every scalar shape, for the POSITIVE
    /// `/config_dump` serialization pin (108.1 REVIEW M-6 closure).
    pub(crate) const RUNTIME_SCALARS_BOOTSTRAP: &str = "\
node:
  id: t
  cluster: c
static_resources:
  listeners: []
  clusters: []
layered_runtime:
  layers:
    - name: scalars
      static_layer:
        k.bool: true
        k.int: 42
        k.float: 1.5
        k.str: hello
        k.nested:
          leaf: v
";
```

  Then append a NEW per-endpoint test module at end-of-file (AFTER
  `test_support`'s closing brace — the file's convention is one module per
  endpoint; do NOT put these inside `test_support`):

```rust
/// 108.2 D4/D5: `GET /runtime` renderer + dispatch coverage, and the
/// 108.1 REVIEW M-6 closure (the positive `/config_dump` serialization pin
/// for `layered_runtime`). Mirrors the per-endpoint test-module convention
/// (`config_dump_tests`, `server_info_tests`, …).
#[cfg(test)]
mod runtime_tests {
    use super::test_support::{
        RUNTIME_SCALARS_BOOTSTRAP, RUNTIME_TWO_LAYER_BOOTSTRAP, TINY_BOOTSTRAP, dump_value,
        handler_with_bootstrap,
    };
    use super::{AdminEndpoint, Dispatch};

    // ------------------------------------------------------------------
    // 108.2 D4: `GET /runtime` renderer tests.
    // ------------------------------------------------------------------

    /// SPEC §2 (MEASURED): a bootstrap with NO `layered_runtime` block
    /// renders exactly `{"entries":{},"layers":[]}` — zero layers, zero
    /// keys — with the shared pretty-JSON response shape (`application/json`,
    /// `reason: None`, deliberately NO `content-length`).
    #[test]
    fn runtime_renders_empty_snapshot_for_absent_block() {
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::Runtime.render_with(&handler);
        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, None);
        assert!(
            resp.headers
                .contains(&("content-type".to_string(), "application/json".to_string())),
            "content-type application/json: {:?}",
            resp.headers
        );
        assert!(
            !resp.headers.iter().any(|(name, _)| name == "content-length"),
            "the pretty-JSON admin shape deliberately carries no content-length"
        );
        let v: serde_json::Value = serde_json::from_slice(&resp.body).expect("valid JSON");
        assert_eq!(v, serde_json::json!({"entries": {}, "layers": []}));
    }

    /// SPEC §2 N-8 (MEASURED): `layered_runtime: {}` synthesizes ONE layer
    /// named the EMPTY STRING — `{"entries":{},"layers":[""]}` — which is
    /// NOT the absent-block shape. Collapsing the two mints a divergence.
    #[test]
    fn runtime_renders_one_empty_string_layer_for_an_empty_block() {
        let yaml = format!("{TINY_BOOTSTRAP}layered_runtime: {{}}\n");
        let handler = handler_with_bootstrap(&yaml);
        let resp = AdminEndpoint::Runtime.render_with(&handler);
        let v: serde_json::Value = serde_json::from_slice(&resp.body).expect("valid JSON");
        assert_eq!(v, serde_json::json!({"entries": {}, "layers": [""]}));
    }

    /// SPEC §2 N-6/N-7 (MEASURED): the four-cell two-layer precedence
    /// transcript, rendered through the WHOLE `/runtime` cascade — slot
    /// order follows config order, `""` marks absence, and `final_value` is
    /// the last NON-EMPTY slot (`empty.in.override` is the cell "last wins"
    /// would get wrong).
    #[test]
    fn runtime_renders_the_measured_two_layer_snapshot() {
        let handler = handler_with_bootstrap(RUNTIME_TWO_LAYER_BOOTSTRAP);
        let resp = AdminEndpoint::Runtime.render_with(&handler);
        let v: serde_json::Value = serde_json::from_slice(&resp.body).expect("valid JSON");
        assert_eq!(
            v,
            serde_json::json!({
                "entries": {
                    "empty.in.override": {
                        "final_value": "real_value",
                        "layer_values": ["real_value", ""]
                    },
                    "only.in.base": {
                        "final_value": "base_val",
                        "layer_values": ["base_val", ""]
                    },
                    "only.in.override": {
                        "final_value": "over_val",
                        "layer_values": ["", "over_val"]
                    },
                    "shared.key": {
                        "final_value": "from_override",
                        "layer_values": ["from_base", "from_override"]
                    }
                },
                "layers": ["base_layer", "override_layer"]
            })
        );
    }

    /// SPEC §2 (MEASURED): `/runtime` has no `format` query parameter —
    /// `?format=text` and any unknown parameter are ignored and the path
    /// still dispatches (the established `from_path` query-strip).
    #[test]
    fn runtime_query_string_still_dispatches() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/runtime?format=text"),
            Dispatch::Endpoint(AdminEndpoint::Runtime)
        ));
    }

    /// CF-108-2 boundary: upstream serves `POST /runtime_modify` (405 on
    /// GET); envoy-rust has no `/runtime_modify` at all (404, unwitnessed
    /// here — recorded divergence). `/runtime` itself is GET-only on BOTH
    /// sides: POST answers 405 with `allow: GET`.
    #[test]
    fn runtime_post_is_method_not_allowed() {
        assert_eq!(
            AdminEndpoint::dispatch("POST", "/runtime"),
            Dispatch::MethodNotAllowed { allow: "GET" }
        );
        assert_eq!(AdminEndpoint::dispatch("GET", "/runtime_modify"), Dispatch::NotFound);
    }

    // ------------------------------------------------------------------
    // 108.1 REVIEW M-6 closure: the POSITIVE `/config_dump` serialization
    // pin for `layered_runtime`.
    // ------------------------------------------------------------------

    /// A `#[serde(skip)]` on `Bootstrap.layered_runtime` — or a broken
    /// `Serialize` arm on `RuntimeValue` — would survive every 108.1 test
    /// (they pin only the ABSENT direction). This test pins the PRESENT
    /// direction through the real `/config_dump` cascade. The float cell
    /// pins our JSON-NUMBER shape deliberately: upstream `/config_dump`
    /// emits float cells as SOURCE-TEXT STRINGS (`"1.5"`), a RECORDED
    /// divergence (108.1 REVIEW M-4; ADR-0174) that fixture 0087 does not
    /// scrape and no fixture witnesses.
    #[test]
    fn config_dump_serializes_layered_runtime_positively() {
        let v = dump_value(&handler_with_bootstrap(RUNTIME_SCALARS_BOOTSTRAP));
        let sl = &v["configs"][0]["bootstrap"]["layered_runtime"]["layers"][0]["static_layer"];
        assert_eq!(sl["k.bool"], serde_json::json!(true));
        assert_eq!(sl["k.int"], serde_json::json!(42));
        assert_eq!(sl["k.float"], serde_json::json!(1.5));
        assert_eq!(sl["k.str"], serde_json::json!("hello"));
        assert_eq!(sl["k.nested"]["leaf"], serde_json::json!("v"));
        assert_eq!(
            v["configs"][0]["bootstrap"]["layered_runtime"]["layers"][0]["name"],
            serde_json::json!("scalars")
        );

        // The ABSENT direction stays absent (`skip_serializing_if`) — the
        // property that keeps the 86 pre-existing `/config_dump` fixtures
        // byte-identical.
        let v = dump_value(&handler_with_bootstrap(TINY_BOOTSTRAP));
        assert!(
            v["configs"][0]["bootstrap"].get("layered_runtime").is_none(),
            "absent layered_runtime must not serialize"
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they FAIL TO COMPILE.**
  `cargo test -p envoy-admin runtime_tests` must fail with
  `E0599: no variant or associated item named 'Runtime' found for enum
  'AdminEndpoint'` — the compile-forcing RED (record it in PROGRESS.md).

- [ ] **Step 3: Add the variant and the three dispatch arms.**
  (a) In `enum AdminEndpoint` (declared `endpoint.rs:9`), insert AFTER the
  `Listeners,` variant and BEFORE the blank line preceding `DrainListeners`'s
  doc comment — check there is NO doc comment directly above the insertion
  point (the doc-orphaning trap):

```rust
    /// 108.2 D4: `GET /runtime` — the runtime snapshot the parsed bootstrap
    /// implies, rendered as pretty JSON with exactly two top-level keys
    /// (`entries`, `layers`) per upstream Envoy v1.33.0's admin runtime
    /// surface (BEHAVIOR_CONTRACT `## Runtime`). Computed per request from
    /// the handler's cached `Arc<Bootstrap>` via
    /// `RuntimeSnapshot::from_bootstrap` — the documented entry point; the
    /// renderer never calls `from_layers` directly, whose slot-count
    /// invariant is unenforced on a `pub` API (108.1 REVIEW M-5).
    Runtime,
```

  (b) In `from_path` (match at `:102`), after the `"/listeners"` arm:

```rust
            // 108.2 D4 — the eleventh endpoint (the eighth GET).
            "/runtime" => Some(AdminEndpoint::Runtime),
```

  (c) In `allowed_method` (the FIRST compile-forcing site — the build is
  broken from (a) until this lands), extend the GET group:

```rust
            | AdminEndpoint::Listeners
            | AdminEndpoint::Runtime => "GET",
```

  (d) In `render_with` (the SECOND compile-forcing site), after the
  `Listeners` arm:

```rust
            // 108.2 D4: the runtime snapshot renderer.
            AdminEndpoint::Runtime => render_runtime(handler),
```

- [ ] **Step 4: Add the body types and renderer**, inserted immediately after
  `render_listeners`'s closing brace (before `render_drain_listeners`'s doc
  block):

```rust
/// 108.2 D4: top-level body for `GET /runtime` — exactly two keys, `entries`
/// then `layers`, mirroring upstream Envoy v1.33.0's measured shape
/// (BEHAVIOR_CONTRACT `## Runtime`). OWNED rather than lifetime-borrowed
/// (contrast [`ConfigDumpBody`]): the snapshot is COMPUTED per request from
/// the cached bootstrap, so there is nothing to borrow from.
#[derive(Serialize)]
struct RuntimeBody {
    entries: std::collections::BTreeMap<String, RuntimeEntryBody>,
    layers: Vec<String>,
}

/// 108.2 D4: one `entries` value — `final_value` (the last NON-EMPTY layer
/// slot) plus `layer_values` (one slot per configured layer, `""` where the
/// key is absent from that layer). Field semantics live on
/// `envoy_config::runtime::RuntimeEntry`; this struct exists only to give the
/// wire shape a `Serialize` derive without imposing serde on `envoy-config`'s
/// engine type.
#[derive(Serialize)]
struct RuntimeEntryBody {
    final_value: String,
    layer_values: Vec<String>,
}

/// 108.2 D4: render `GET /runtime`. Delegates the entire snapshot semantics
/// (arbitrary-depth flattening, stringification, slot layout, last-non-empty
/// precedence, the absent-vs-empty distinction) to
/// `RuntimeSnapshot::from_bootstrap` — the 108.1-landed, reviewed entry
/// point — and hands the result to the shared [`json_pretty_200`] helper, so
/// the response plumbing (pretty JSON, `application/json`, no
/// `content-length`) is byte-identical to `/config_dump`'s and
/// `/server_info`'s. Key order on the wire is `BTreeMap`-canonical; the
/// differential comparison is order-insensitive either way
/// (`BodyRule::JsonShape` re-parses both sides).
fn render_runtime(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    let snapshot = envoy_config::runtime::RuntimeSnapshot::from_bootstrap(handler.bootstrap());
    let body = RuntimeBody {
        entries: snapshot
            .entries
            .into_iter()
            .map(|(key, entry)| {
                (
                    key,
                    RuntimeEntryBody {
                        final_value: entry.final_value,
                        layer_values: entry.layer_values,
                    },
                )
            })
            .collect(),
        layers: snapshot.layer_names,
    };
    json_pretty_200(&body, "RuntimeBody")
}
```

- [ ] **Step 5: Extend the two convention-only tests** (NOT compile-forcing —
  this is the deliberate update the SPEC requires). In
  `get_known_path_returns_endpoint` (fn at `:2332`), append after the
  `/listeners` row:

```rust
        // 108.2 D4 adds the eleventh endpoint (the eighth GET).
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/runtime"),
            Dispatch::Endpoint(AdminEndpoint::Runtime)
        ));
```

  In `each_endpoint_declares_its_allowed_method` (fn at `:2415`), append:

```rust
        // 108.2 D4.
        assert_eq!(AdminEndpoint::Runtime.allowed_method(), "GET");
```

  (`each_drain_endpoint_declares_post_allowed_method` is NOT touched — it
  covers the 3 POST variants only.)

- [ ] **Step 6: Run and verify GREEN.**
  `cargo test -p envoy-admin` — expect **103 passed** (97 baseline + 6 new).
  Then `cargo fmt --all -- --check` (clean) and
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  (exit 0 AND a non-zero `Checking` count).

- [ ] **Step 7: Commit.**
  `git add crates/envoy-admin/src/endpoint.rs && git commit -m "phase 108.2 task 1: admin GET /runtime — the eleventh endpoint"`

---

### Task 2: the nine `runtime.*` stats (envoy-bin)

**Files:**
- Create: `crates/envoy-bin/src/runtime_stats.rs`
- Modify: `crates/envoy-bin/src/main.rs` (module decl + one call)

**Interfaces:**
- Consumes: `RuntimeSnapshot::from_bootstrap`,
  `StatsRegistry::{register_counter, register_gauge}`
  (`crates/envoy-stats/src/registry.rs:45`/`:69` — get-or-create; dotted
  names pass `is_valid_name`, whose FIRST character must not be `.` — fine
  for the `runtime.` prefix), `Counter::inc`, `Gauge::set`,
  `StatsRegistry::snapshot` + `StatHandle` (tests).
- Produces: `runtime_stats::register_runtime_stats(&Bootstrap, &StatsRegistry)
  -> Result<(), StatsError>`, called once from `main.rs`.

- [ ] **Step 1: Write the module WITH its failing tests** — create
  `crates/envoy-bin/src/runtime_stats.rs` with exactly this content (the
  test yamls NEED the `admin:` block: `envoy_config::parse_bootstrap` runs the
  validator, and a config with neither admin nor listener is
  `ConfigError::NoRuntime` — measured in pre-flight):

```rust
//! 108.2 D5: the nine `runtime.*` stats, registered UNCONDITIONALLY at
//! process startup — upstream Envoy v1.33.0 emits all nine even on a config
//! with no `layered_runtime` block at all (SPEC §2, MEASURED).
//!
//! Kinds mirror upstream's `/stats/prometheus` `# TYPE` lines, MEASURED
//! against the pinned image (ADR-0174): four GAUGES
//! (`admin_overrides_active`, `deprecated_feature_seen_since_process_start`,
//! `num_keys`, `num_layers`) and five COUNTERS (`deprecated_feature_use`,
//! `load_error`, `load_success`, `override_dir_exists`,
//! `override_dir_not_exists`).
//!
//! Only `num_keys` and `num_layers` track config; `load_success: 1` and
//! `override_dir_not_exists: 1` fire unconditionally (MEASURED: `load_success`
//! stays `1` on a TWO-layer config — it counts loads, not layers); the other
//! five are `0` on any in-scope config. Values are set ONCE here — nothing
//! mutates the snapshot after startup in this slice (no RTDS, no
//! `/runtime_modify`, no override directory).

use envoy_config::Bootstrap;
use envoy_config::runtime::RuntimeSnapshot;
use envoy_stats::{StatsError, StatsRegistry};

/// Register the nine `runtime.*` stats and bind the two config-tracking
/// gauges to the snapshot the parsed bootstrap implies. Mirrors the
/// `register_lds_stats` / `register_rds_stats` startup cadence in `main.rs`;
/// like them it is called exactly once, before the admin listener serves.
pub fn register_runtime_stats(
    bootstrap: &Bootstrap,
    registry: &StatsRegistry,
) -> Result<(), StatsError> {
    // The same entry point the `/runtime` renderer uses (108.1 REVIEW M-5:
    // never `from_layers` directly), so the stats and the endpoint can never
    // disagree about the snapshot.
    let snapshot = RuntimeSnapshot::from_bootstrap(bootstrap);

    // Gauges (upstream `# TYPE ... gauge`).
    registry.register_gauge("runtime.admin_overrides_active")?;
    registry.register_gauge("runtime.deprecated_feature_seen_since_process_start")?;
    let num_keys = registry.register_gauge("runtime.num_keys")?;
    num_keys.set(i64::try_from(snapshot.num_keys()).unwrap_or(i64::MAX));
    let num_layers = registry.register_gauge("runtime.num_layers")?;
    num_layers.set(i64::try_from(snapshot.num_layers()).unwrap_or(i64::MAX));

    // Counters (upstream `# TYPE ... counter`).
    registry.register_counter("runtime.deprecated_feature_use")?;
    registry.register_counter("runtime.load_error")?;
    registry.register_counter("runtime.load_success")?.inc();
    registry.register_counter("runtime.override_dir_exists")?;
    registry
        .register_counter("runtime.override_dir_not_exists")?
        .inc();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_stats::StatHandle;

    const TWO_LAYER_YAML: &str = "\
node:
  id: t
  cluster: c
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 0
static_resources:
  listeners: []
  clusters: []
layered_runtime:
  layers:
    - name: base_layer
      static_layer:
        shared.key: from_base
        only.in.base: base_val
        nested:
          deep: x
    - name: override_layer
      static_layer:
        shared.key: from_override
        only.in.override: over_val
";

    /// Look a stat up in the registry snapshot; panic with the name on a miss
    /// so an absent registration reads as the failure it is (never `Ok(0)`).
    fn handle_for(registry: &StatsRegistry, name: &str) -> StatHandle {
        registry
            .snapshot()
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("stat {name} not registered"))
            .1
    }

    /// SPEC §2 (MEASURED on the fixture-0087 config): all nine names exist,
    /// with the four non-zero witnesses at their measured values and the
    /// measured counter/gauge kinds (upstream `/stats/prometheus` `# TYPE`
    /// lines — ADR-0174). `nested.deep` proves `num_keys` counts FLATTENED
    /// LEAVES: 3 base leaves + 1 override-only key = 4.
    #[test]
    fn registers_all_nine_runtime_stats_with_measured_values_and_kinds() {
        let registry = StatsRegistry::new();
        let b = envoy_config::parse_bootstrap(TWO_LAYER_YAML).expect("valid bootstrap");
        register_runtime_stats(&b, &registry).expect("register");

        for (name, value) in [
            ("runtime.admin_overrides_active", 0),
            ("runtime.deprecated_feature_seen_since_process_start", 0),
            ("runtime.num_keys", 4),
            ("runtime.num_layers", 2),
        ] {
            match handle_for(&registry, name) {
                StatHandle::Gauge(g) => assert_eq!(g.value(), value, "{name}"),
                StatHandle::Counter(_) => panic!("{name} must be a GAUGE (measured upstream kind)"),
            }
        }
        for (name, value) in [
            ("runtime.deprecated_feature_use", 0),
            ("runtime.load_error", 0),
            ("runtime.load_success", 1),
            ("runtime.override_dir_exists", 0),
            ("runtime.override_dir_not_exists", 1),
        ] {
            match handle_for(&registry, name) {
                StatHandle::Counter(c) => assert_eq!(c.value(), value, "{name}"),
                StatHandle::Gauge(_) => panic!("{name} must be a COUNTER (measured upstream kind)"),
            }
        }
    }

    /// SPEC §2 N-8 (MEASURED): the absent-vs-empty distinction reaches the
    /// stats — no block: `num_layers 0 / num_keys 0`; an empty block (either
    /// spelling): `num_layers 1 / num_keys 0`; and the unconditional pair
    /// (`load_success`, `override_dir_not_exists`) is 1 in every case.
    #[test]
    fn absent_and_empty_blocks_differ_in_num_layers_only() {
        let base = "admin:\n  address:\n    socket_address:\n      address: 127.0.0.1\n      port_value: 0\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        for (spelling, layers) in [
            ("", 0),
            ("layered_runtime: {}\n", 1),
            ("layered_runtime:\n  layers: []\n", 1),
        ] {
            let registry = StatsRegistry::new();
            let b = envoy_config::parse_bootstrap(&format!("{base}{spelling}")).expect("valid");
            register_runtime_stats(&b, &registry).expect("register");
            match handle_for(&registry, "runtime.num_layers") {
                StatHandle::Gauge(g) => assert_eq!(g.value(), layers, "num_layers for {spelling:?}"),
                StatHandle::Counter(_) => panic!("num_layers must be a gauge"),
            }
            match handle_for(&registry, "runtime.num_keys") {
                StatHandle::Gauge(g) => assert_eq!(g.value(), 0, "num_keys for {spelling:?}"),
                StatHandle::Counter(_) => panic!("num_keys must be a gauge"),
            }
            match handle_for(&registry, "runtime.load_success") {
                StatHandle::Counter(c) => assert_eq!(c.value(), 1),
                StatHandle::Gauge(_) => panic!("load_success must be a counter"),
            }
        }
    }
}
```

- [ ] **Step 2: RED.** `cargo test -p envoy-bin --bin envoy-bin runtime_stats`
  fails to compile — the module is not declared in `main.rs`
  (`E0583`/unresolved module). Record the RED.

- [ ] **Step 3: Wire into `main.rs`.** Add `mod runtime_stats;` after
  `mod network_rbac;`, and after the `register_rds_stats` call
  (`main.rs:118-119` region) add:

```rust
    // 108.2 D5: the nine runtime.* stats, registered UNCONDITIONALLY —
    // upstream emits all nine even with no `layered_runtime` block (SPEC §2).
    runtime_stats::register_runtime_stats(&bootstrap, &registry)
        .context("registering runtime.* stats")?;
```

- [ ] **Step 4: GREEN.**
  `cargo test -p envoy-bin --bin envoy-bin runtime_stats` →
  `test result: ok. 2 passed` (assert the COUNT, not the exit code). Then the
  fmt/clippy boundary gate.

- [ ] **Step 5: Commit.**
  `git add crates/envoy-bin/src/runtime_stats.rs crates/envoy-bin/src/main.rs && git commit -m "phase 108.2 task 2: the nine runtime.* stats"`

---

### Task 3: the `expected_stats` harness extension + fixture `0087` data files

**Files:**
- Modify: `tests/differential/src/lib.rs` (4 edits)
- Create: `tests/fixtures/0087-runtime-static-layer/envoy.yaml`,
  `envoy-rust.yaml`, `expectations.yaml`

**Interfaces:**
- Consumes: `KeepAliveExpectedStat` (`lib.rs`, `{name: String, value: u64}`),
  `assert_expected_stats_bilaterally` (`lib.rs:4508`), the
  `run_admin_scrape_arm` locals `upstream_admin_addr`/`subject_admin_addr`.
- Produces: `Driver::AdminScrape.expected_stats` (defaulted — the six
  existing AdminScrape fixtures parse unchanged), fixture `0087`'s three data
  files (the 87th fixture dir; census 86 → 87).

- [ ] **Step 1: Write the failing test** — in `lib.rs`'s main `mod tests`,
  immediately BEFORE `fixture_0001_expectations_parses_as_tcp_echo`'s
  `#[test]` attribute, insert:

```rust
    /// 108.2 D6: fixture 0087's expectations parse into the AdminScrape
    /// shape, INCLUDING the new `expected_stats` field — two /runtime
    /// scrapes (one `entries` subtree, one `layers`) and nine bilateral
    /// stat assertions of which FOUR are non-zero witnesses.
    #[test]
    fn fixture_0087_expectations_parses_as_admin_scrape_with_expected_stats() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests/fixtures/0087-runtime-static-layer/expectations.yaml");
        let e = load_expectations(&path).expect("parses");
        match e.driver {
            Driver::AdminScrape {
                ref pre_admin_actions,
                ref pre_requests,
                ref scrapes,
                ref expected_stats,
                ref post_admin_assertions,
            } => {
                assert!(pre_admin_actions.is_empty());
                assert!(pre_requests.is_empty());
                assert!(post_admin_assertions.is_empty());
                assert_eq!(scrapes.len(), 2, "two /runtime scrapes");
                assert!(scrapes.iter().all(|s| s.path == "/runtime"));
                assert_eq!(expected_stats.len(), 9, "all nine runtime.* stats");
                let non_zero: Vec<&str> = expected_stats
                    .iter()
                    .filter(|s| s.value != 0)
                    .map(|s| s.name.as_str())
                    .collect();
                assert_eq!(
                    non_zero,
                    vec![
                        "runtime.num_keys",
                        "runtime.num_layers",
                        "runtime.load_success",
                        "runtime.override_dir_not_exists"
                    ],
                    "exactly the four real witnesses are non-zero"
                );
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
    }

```

- [ ] **Step 2: Write the three fixture data files.**
  `tests/fixtures/0087-runtime-static-layer/envoy.yaml`:

```yaml
# Sub-phase 108.2 fixture 0087 — the admin GET /runtime differential (D6).
# Backend-free and CLUSTER-FREE: the echo listener exists only because
# run_fixture's data-plane accept-ready wait needs a {{PORT}} listener
# (fixture 0001's spelling); no traffic is ever driven at it.
#
# The layered_runtime block witnesses every measured /runtime rule
# (BEHAVIOR_CONTRACT `## Runtime`): scalar stringification (bool, int,
# negative int, float, string, quoted numeric string, empty string),
# arbitrary-depth flattening to dotted keys, two-layer slot ordering, the
# ""-absent slot marker, and last-NON-EMPTY-wins precedence.
#
# Deliberately ABSENT (all recorded, none witnessed here):
#   - unquoted y/n/on/off values — CF-108-4 (YAML 1.1 vs 1.2 divergence);
#   - floats whose source spelling differs from Rust f64 Display output
#     (`1.0`, `1e6`, `-0.0`, `1e-7`, `.nan`, …) — upstream renders the raw
#     SOURCE TEXT, envoy-rust renders f64 Display (CF-108-5, ADR-0174);
#     `1.5` is spelling-stable and is the one float cell both sides agree on;
#   - nested maps containing `numerator` — CF-108-3 (protobuf text-format);
#   - the envoy.reloadable_features. prefix (non-fatal envoy_bug stderr).
node:
  id: envoy-rust-phase-108.2-fixture-0087
  cluster: envoy-rust-phase-108.2
admin:
  address:
    socket_address:
      address: 0.0.0.0
      port_value: {{ADMIN_PORT}}
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.echo.v3.Echo
  clusters: []
layered_runtime:
  layers:
    - name: base_layer
      static_layer:
        diff.bool.true.key: true
        diff.bool.false.key: false
        diff.int.key: 42
        diff.negative.key: -7
        diff.float.key: 1.5
        diff.string.key: hello
        diff.quoted.number.key: "42"
        diff.empty.string.key: ""
        diff.nested:
          sub_key: v
          deeper:
            leaf: w
        shared.key: from_base
        only.in.base: base_val
        empty.in.override: real_value
    - name: override_layer
      static_layer:
        shared.key: from_override
        only.in.override: over_val
        empty.in.override: ""
```

  `envoy-rust.yaml`: byte-identical EXCEPT the listener's filter block, which
  uses the name-only echo spelling (DD-3; the fixture-0001 precedent
  divergence — envoy-rust's `typed_config` enum does not model the echo
  `@type`; measured boot-fatal in pre-flight otherwise):

```yaml
      filter_chains:
        - filters:
            # Name-only spelling, per the fixture-0001 envoy-rust precedent:
            # envoy-rust's typed_config enum does not model the echo filter's
            # @type; the name-only form selects the same built-in echo filter.
            - name: envoy.filters.network.echo
```

  `expectations.yaml` — the expected values are §0.1/§0.2's MEASURED
  transcript, verbatim:

```yaml
# Sub-phase 108.2 fixture 0087 — Driver::AdminScrape over GET /runtime (D6).
#
# TWO scrapes of the SAME path, because BodyRule::JsonShape permits only ONE
# required_subtree per rule (tests/differential/src/lib.rs, JsonSubtreeRule):
# scrape 1 anchors the whole `entries` object, scrape 2 anchors `layers`.
# Both carry required_keys [entries, layers] and EMPTY per-side allow-lists —
# the intent is that NOTHING needs allow-listing; if something does, that is
# a finding, not a knob to turn (SPEC §1 D6).
#
# The expected values below are the MEASURED upstream response — taken
# against envoyproxy/envoy:v1.33.0 (digest sha256:56da5afd…70c2) with THIS
# fixture's exact layered_runtime block at the 108.2 state-2 PLAN-write
# (2026-08-08; two scrapes, md5-distinct at identical 1508 bytes — the
# per-request key-order shuffle JsonShape's canonical re-parse absorbs).
# Every scalar is a YAML STRING here (quoted) because /runtime stringifies
# every value; an unquoted 42 would YAML-parse as an integer and the
# JsonShape expected-conversion would then compare JSON 42 against "42".
#
# ⚠ subtree paths are WALKED ON DOTS (walk_pointer): "entries" and "layers"
# are single-segment and safe; a path pointing INTO an entry
# ("entries.diff.int.key") would mis-walk the dotted KEY as five segments.
# Never anchor a subtree below `entries`.
driver:
  kind: admin_scrape
  pre_requests: []
  scrapes:
    # -------------------------------------------------------------------
    # Scrape 1 — the whole `entries` object (all 14 flattened keys).
    # -------------------------------------------------------------------
    - path: /runtime
      expected_status: 200
      expected_content_type: "application/json"
      expected_body_rule:
        kind: json_shape
        required_keys: ["entries", "layers"]
        required_subtree:
          path: "entries"
          expected:
            diff.bool.true.key:
              final_value: "true"
              layer_values: ["true", ""]
            diff.bool.false.key:
              final_value: "false"
              layer_values: ["false", ""]
            diff.int.key:
              final_value: "42"
              layer_values: ["42", ""]
            diff.negative.key:
              final_value: "-7"
              layer_values: ["-7", ""]
            diff.float.key:
              final_value: "1.5"
              layer_values: ["1.5", ""]
            diff.string.key:
              final_value: "hello"
              layer_values: ["hello", ""]
            diff.quoted.number.key:
              final_value: "42"
              layer_values: ["42", ""]
            diff.empty.string.key:
              final_value: ""
              layer_values: ["", ""]
            diff.nested.sub_key:
              final_value: "v"
              layer_values: ["v", ""]
            diff.nested.deeper.leaf:
              final_value: "w"
              layer_values: ["w", ""]
            shared.key:
              final_value: "from_override"
              layer_values: ["from_base", "from_override"]
            only.in.base:
              final_value: "base_val"
              layer_values: ["base_val", ""]
            only.in.override:
              final_value: "over_val"
              layer_values: ["", "over_val"]
            empty.in.override:
              final_value: "real_value"
              layer_values: ["real_value", ""]
        allowlist_envoy_only_keys: []
        allowlist_envoy_rust_only_keys: []
        value_may_differ_keys: []
    # -------------------------------------------------------------------
    # Scrape 2 — the `layers` array (names in config order).
    # -------------------------------------------------------------------
    - path: /runtime
      expected_status: 200
      expected_content_type: "application/json"
      expected_body_rule:
        kind: json_shape
        required_keys: ["entries", "layers"]
        required_subtree:
          path: "layers"
          expected: ["base_layer", "override_layer"]
        allowlist_envoy_only_keys: []
        allowlist_envoy_rust_only_keys: []
        value_may_differ_keys: []
  # ---------------------------------------------------------------------
  # STEP 3.5 — the nine runtime.* stats, bilateral ABSOLUTE values
  # (108.2 D5 harness extension; values MEASURED on this exact config).
  #
  # ⚠ WITNESS LEDGER (the scrape_admin_stat Ok(0) vacuous-pass rule,
  # tests/differential/src/lib.rs:4504-4507): only the FOUR non-zero
  # entries are real witnesses — num_keys (14 flattened leaves, NOT 13
  # declared keys), num_layers (2), load_success (1 — loads, not layers),
  # override_dir_not_exists (1, unconditional). The five value-0 entries
  # pass vacuously if the name is absent; their PRESENCE on the envoy-rust
  # side is pinned in-process by
  # envoy-bin::runtime_stats::registers_all_nine_runtime_stats_with_measured_values_and_kinds.
  # ---------------------------------------------------------------------
  expected_stats:
    - { name: "runtime.num_keys", value: 14 }
    - { name: "runtime.num_layers", value: 2 }
    - { name: "runtime.load_success", value: 1 }
    - { name: "runtime.override_dir_not_exists", value: 1 }
    - { name: "runtime.admin_overrides_active", value: 0 }
    - { name: "runtime.deprecated_feature_seen_since_process_start", value: 0 }
    - { name: "runtime.deprecated_feature_use", value: 0 }
    - { name: "runtime.load_error", value: 0 }
    - { name: "runtime.override_dir_exists", value: 0 }
```

- [ ] **Step 3: RED.** `cargo test -p differential --lib fixture_0087` fails:
  the `Driver::AdminScrape` destructure has no `expected_stats` field
  (`E0026`) — the compile-forcing RED for the harness field. Record it.

- [ ] **Step 4: Add the field and wire the arm** — four edits in `lib.rs`:
  (a) in the `Driver::AdminScrape` variant, after `scrapes`:

```rust
        /// 108.2 D5: bilateral ABSOLUTE-value admin-stat assertions, run as
        /// STEP 3.5 (after the scrape loop, before `post_admin_assertions`).
        /// Reuses `KeepAliveExpectedStat` + `assert_expected_stats_bilaterally`
        /// verbatim — each named stat must equal `value` on BOTH proxies.
        /// Absolute (not ADR-0131 deltas) because `runtime.*` stats are set
        /// once at startup and no readiness connect perturbs them. ⚠ The
        /// vacuous-pass trap applies: `scrape_admin_stat` returns `Ok(0)` for
        /// a name the proxy never registered, so a `value: 0` entry passes
        /// even when the stat is ABSENT — only non-zero entries are real
        /// witnesses, and fixture READMEs must say which is which.
        #[serde(default)]
        expected_stats: Vec<KeepAliveExpectedStat>,
```

  (b) in `run_fixture`'s dispatch, destructure `expected_stats` and pass it
  through (between `scrapes` and `post_admin_assertions`, both places);
  (c) on `run_admin_scrape_arm`, add the parameter
  `expected_stats: &[KeepAliveExpectedStat]` between `scrapes` and
  `post_admin_assertions`, and put this attribute + comment directly above
  the `async fn` (the pre-flight's one real clippy error — DD-7):

```rust
// The 108.2 expected_stats widening takes this to 8 args. Mirrors the
// AdminHandler::new precedent: the params are the driver variant's own fields
// threaded verbatim from the dispatch site, and a params struct would obscure
// the field-to-arg correspondence the destructure makes obvious.
#[allow(clippy::too_many_arguments)]
```

  (d) in the arm body, after the scrape-assertion loop
  (`assert_admin_scrape_case`) and BEFORE the `// STEP 4:` comment:

```rust
    // STEP 3.5 (108.2 D5): bilateral absolute-value stat assertions. Runs
    // after the scrape loop so a body-shape failure surfaces first (the
    // richer diagnostic), and before the wire-level post_admin_assertions,
    // preserving lock-in #18's temporal ordering for the existing steps.
    assert_expected_stats_bilaterally(upstream_admin_addr, subject_admin_addr, expected_stats)
        .await?;
```

- [ ] **Step 5: GREEN.** `cargo test -p differential --lib` → **164 passed**
  (163 baseline + the new parse test; assert the count). fmt/clippy boundary
  gate (`Checking` count non-zero).

- [ ] **Step 6: Commit.**
  `git add tests/differential/src/lib.rs tests/fixtures/0087-runtime-static-layer && git commit -m "phase 108.2 task 3: AdminScrape expected_stats + fixture 0087 data"`

---

### Task 4: the differential test binary + the local fixture run

**Files:**
- Create: `tests/differential/tests/runtime_static_layer.rs`

**Interfaces:**
- Consumes: `differential::run_fixture`.
- Produces: the 87th differential test file / 164th workspace test binary,
  named `runtime_static_layer` (fixture→binary derivation:
  `grep -oE 'tests/fixtures/[0-9]{4}-[a-z0-9-]+' tests/differential/tests/runtime_static_layer.rs`).

- [ ] **Step 1: Write the test file:**

```rust
//! Docker-gated differential test for fixture 0087-runtime-static-layer.
//! Sub-phase 108.2 D6 — the runtime family's first differential: the admin
//! `GET /runtime` snapshot (entries + layers) and the four non-zero
//! `runtime.*` stat witnesses, asserted bilaterally against upstream Envoy
//! v1.33.0 over `tests/fixtures/0087-runtime-static-layer/`.

use std::path::PathBuf;

#[tokio::test]
async fn runtime_static_layer() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0087-runtime-static-layer");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Rebuild the debug binary FIRST** —
  `cargo build -p envoy-bin` (the harness runs `target/debug/envoy-bin`; this
  phase's Task 2 changed it, and a stale binary REDs on nothing at all).

- [ ] **Step 3: Run the fixture locally** (backend-free → fully verifiable
  here, expect ~1-3 s):
  `cargo test -p differential --test runtime_static_layer -- --nocapture`
  → `test result: ok. 1 passed`. A mass `client error (Connect)` = Docker
  daemon down (`sudo setfacl -m u:esa:rw /dev/kvm && systemctl --user restart
  docker-desktop`). Record the transcript in PROGRESS.md — SPEC §5 requires
  the local run recorded (and state 4 runs it again; the structural
  no-backend argument is NOT a substitute).

- [ ] **Step 4: Mutation checks — prove the assertions bite** (in-place data
  mutations with a post-revert `git diff --stat` EMPTY check, the 76.2 q02
  precedent; no parallel subagents while mutated):
  (a) in `expectations.yaml` flip `shared.key.final_value` `"from_override"`
  → `"from_base"`; rerun; expect RED naming the `entries` subtree mismatch;
  READ THE FAILURE TEXT (a compile error or a startup failure is NOT a
  mutation RED — gate on the `test result` line existing);
  (b) revert (a), then flip `runtime.num_keys` `value: 14` → `13`; rerun;
  expect RED from `assert_expected_stats_bilaterally` (`expected 13 got 14`)
  — this is the witness that a WRONG flattened-leaf count fails loudly on
  BOTH sides;
  (c) revert, `git diff --stat` must be EMPTY, rerun once more → GREEN.

- [ ] **Step 5: Commit.**
  `git add tests/differential/tests/runtime_static_layer.rs && git commit -m "phase 108.2 task 4: fixture 0087 differential test + local green"`

---

### Task 5: `BEHAVIOR_CONTRACT.md` `## Runtime` + admin row + stat mapping + fixture README

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (three locations, by TEXT)
- Create: `tests/fixtures/0087-runtime-static-layer/README.md`

- [ ] **Step 1: the `## Runtime` section** — insert immediately BEFORE the
  `## xDS wire state machine` heading (locate by text; `:3153` at plan time):

```markdown
## Runtime

> Authored at sub-phase 108.2 (phase-108 family opener; ADR-0171/0172/0173/0174).
> Everything here is MEASURED against the pinned `envoyproxy/envoy:v1.33.0`
> unless marked otherwise. The engine is `envoy_config::runtime` (108.1); the
> observer is admin `GET /runtime` + the nine `runtime.*` stats (108.2).
> Nothing READS the runtime store for behavior yet — the consumer slice
> (`runtime_key` honoring, route `runtime_fraction`, RTDS) is future work,
> which is why every landed "no runtime CONSUMER for this key" assertion
> (incl. the test `runtime_key_is_rtds_inert`) stays true.

**Layer grammar** (108.1): `layered_runtime.layers[]`, each with `name`
(PGV min length 1) + exactly ONE of four oneof arms. Only `static_layer` is
implemented; `disk_layer`/`rtds_layer`/`admin_layer` are declared and
loudly rejected (CF-108-1, ADR-0173 DECISION 4 — a recorded reject-direction
divergence; upstream accepts all three). Duplicate layer names reject
bilaterally.

**`GET /runtime`** — the eleventh admin endpoint, GET-only (POST → 405
`allow: GET` on both sides). 200 `application/json`; body is exactly two
top-level keys:

| Shape rule | Measured behaviour |
|---|---|
| `entries` | object: flattened dotted key → `{"final_value": S, "layer_values": [S, …]}` — every value a STRING |
| `layers` | array of layer NAMES, config order |
| stringification | `true`→`"true"`, `42`→`"42"`, `-7`→`"-7"`, strings verbatim, `""` stays `""` |
| floats | upstream preserves the RAW SOURCE TEXT (`1e6`→`"1e6"`, `1.50`→`"1.50"`, `.nan`→`".nan"` — 11 cells measured, ADR-0174); envoy-rust renders `f64` Display — divergent on every non-Display-stable spelling, RECORDED (CF-108-5 closed-as-recorded); fixtures use Display-stable spellings only (`1.5`) |
| flattening | ARBITRARY depth; NO intermediate-map entries; `num_keys` counts flattened LEAVES |
| slots | one `layer_values` slot per configured layer, `""` where the key is absent from that layer |
| precedence | `final_value` = the last NON-EMPTY slot; an explicit `""` does NOT override and is wire-indistinguishable from absence |
| absent vs empty | no block → `{"entries":{},"layers":[]}`; `layered_runtime: {}` or `{layers: []}` → ONE synthetic layer named `""` |
| nondeterminism | upstream shuffles key order per request (measured: 8 GETs, 8 md5s, equal bytes); envoy-rust is BTreeMap-canonical; `BodyRule::JsonShape` re-parses both sides, so the comparison is order-insensitive |
| query params | none exist; `?format=text` etc. silently ignored |
| no pre-population | upstream does NOT surface its 89 built-in `envoy.reloadable_features.*` flags in `/runtime` (the pick-enabling measurement, ADR-0171 DECISION 3) |

**The nine `runtime.*` stats** — registered unconditionally on both sides
(all nine exist even with no `layered_runtime` block). Kinds per upstream's
`/stats/prometheus` `# TYPE` lines: gauges `admin_overrides_active`,
`deprecated_feature_seen_since_process_start`, `num_keys`, `num_layers`;
counters `deprecated_feature_use`, `load_error`, `load_success`,
`override_dir_exists`, `override_dir_not_exists`. Only `num_keys`/`num_layers`
track config; `load_success` is `1` unconditionally (loads, not layers —
measured `1` on a two-layer config) and `override_dir_not_exists` is `1`
unconditionally; the other five are `0` on any in-scope config and are
therefore VACUOUS as differential value assertions (`scrape_admin_stat`
returns `Ok(0)` for an absent name) — their envoy-rust presence is pinned
in-process (`envoy-bin::runtime_stats` tests).

**Fixture `0011` prose correction (recorded here, fixture NOT edited —
D-3.5):** `0011`'s `expectations.yaml:35` and `README.md:55` call the nine
`runtime.*` stats "RTDS runtime layer. Deferred to the xDS family." As of
108.2 they are neither deferred nor xDS-family — they are emitted by the
static-layer runtime subsystem. The nine allow-list entries at
`expectations.yaml:234-242` stay (the allow-list filters a set difference in
the permissive direction; once both sides emit the names they leave the
difference entirely, so the entries are inert-but-harmless). Deleting them is
optional tightening for a session that legitimately touches `0011`.

**Recorded divergences and NOT-MEASURED cells** (all excluded from fixture
`0087`): CF-108-4 (unquoted `y`/`n`/`on`/`off` booleanize upstream — YAML
1.1 — but not here; ADR-0173); CF-108-5 (float source-text preservation,
above; also on the `/config_dump` cascade, where upstream emits float cells
as SOURCE-TEXT STRINGS while envoy-rust emits JSON NUMBERS — pinned as ours
by `config_dump_serializes_layered_runtime_positively`); non-finite floats
(`.nan`/`.inf` boot clean both sides; upstream renders source text,
envoy-rust would render `"NaN"`/`"inf"`; envoy-rust's own `Deserialize`
rejects the JSON `null` its `Serialize` would emit — 108.1 REVIEW M-3,
recorded); CF-108-3 (a nested map containing `numerator` is ONE
protobuf-text-format key upstream, flattened here); CF-108-2
(`/runtime_modify` upstream-only: POST-only, 405 on GET, 503 `No admin layer
specified` without an admin layer; envoy-rust 404s it); NOT MEASURED
upstream: a flattened-key collision inside one layer (`a: {b: 1}` + `a.b: 2`
— envoy-rust: deterministic, dotted spelling wins by BTreeMap byte order,
silently drops a value — 108.1 REVIEW M-1), an empty nested map as a value
(M-7), empty or dot-bearing key SEGMENTS (N-5), an explicit-null
`static_layer:` (N-4).
```

- [ ] **Step 2: the admin body-shapes row** — append to the
  `## Admin endpoint body shapes` table (after the `/healthcheck/ok` row):

```markdown
| `/runtime` | GET | JSON object | Top-level shape exactly `{ "entries": {...}, "layers": [...] }` (see `## Runtime`). Value-equal after canonical re-parse on identical configs — the fixture-0087 disposition is EMPTY allow-lists on both sides; upstream's per-request key-order shuffle is absorbed by `JsonShape`'s `serde_json` re-parse (`Map` is a `BTreeMap`; `preserve_order` enabled nowhere). Both sides pretty-print with no `content-length` (`transfer-encoding` handling is transport-level and not compared). POST → 405 `allow: GET` bilaterally. `/runtime_modify` is NOT served by envoy-rust (404) vs upstream POST-only (405 on GET) — recorded divergence CF-108-2, unwitnessed by any fixture. |
```

- [ ] **Step 3: the stat-name mapping entries** — append a `**108.2
  entries:**` block at the end of `## Stat-name mapping` (same 3-column
  format):

```markdown
**108.2 entries:**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `runtime.num_keys`, `runtime.num_layers` | value-exact | Gauges tracking the parsed config: flattened LEAF count (not declared top-level keys) and configured layer count. Deterministic on identical configs; fixture 0087 asserts `14`/`2` bilaterally. |
| `runtime.load_success`, `runtime.override_dir_not_exists` | value-exact | Counters, `1` unconditionally at startup (`load_success` counts loads, not layers — measured `1` on a two-layer config). Real non-zero witnesses. |
| `runtime.admin_overrides_active`, `runtime.deprecated_feature_seen_since_process_start`, `runtime.deprecated_feature_use`, `runtime.load_error`, `runtime.override_dir_exists` | value-exact (vacuous at 0) | `0` on every in-scope config (no admin layer, no deprecated-feature use, no disk layer). A differential `value: 0` assertion passes vacuously when the name is absent (`scrape_admin_stat` → `Ok(0)`); presence is pinned in-process by `envoy-bin::runtime_stats` tests. Names match one-to-one; kinds per upstream `# TYPE` lines (4 gauges / 5 counters). |
```

- [ ] **Step 4: the fixture README** —
  `tests/fixtures/0087-runtime-static-layer/README.md`:

```markdown
# Fixture 0087 — runtime static layer (`GET /runtime` + the nine `runtime.*` stats)

Sub-phase 108.2 D6 (phase-108 runtime family opener; ADR-0171/0172/0173/0174).
The first differential of the runtime subsystem: both proxies parse the SAME
two-layer `layered_runtime` block and must serve equivalent `GET /runtime`
snapshots and equal `runtime.*` stat values.

## Shape

- **Backend-free, CLUSTER-FREE** (`clusters: []`) — fully verifiable on the
  development host (no `192.168.65.2` bridge-IP exposure). The echo listener
  exists ONLY because `run_fixture`'s data-plane accept-ready wait needs a
  `{{PORT}}` listener; no traffic is driven at it.
- **Driver:** `admin_scrape`, `pre_requests: []`, TWO `/runtime` scrapes
  (`BodyRule::JsonShape` permits one `required_subtree` per rule — scrape 1
  anchors the whole 14-entry `entries` object, scrape 2 anchors `layers`),
  plus nine bilateral `expected_stats` assertions (the 108.2 harness
  extension).
- **Config divergence between the two YAMLs** (fixture-0001 precedent): the
  envoy-rust listener uses the NAME-ONLY echo filter spelling — envoy-rust's
  `typed_config` enum does not model the echo `@type`. Everything else,
  including the whole `layered_runtime` block, is byte-identical.

## What is witnessed

1. Scalar stringification (`true`/`false`/`42`/`-7`/`1.5`/`hello`/`"42"`/`""`).
2. Arbitrary-depth flattening (`diff.nested.deeper.leaf`; no intermediate
   `diff.nested` entry — its absence from the EXPECTED subtree is asserted
   because the subtree comparison is whole-object equality).
3. Two-layer slot ordering (`layer_values` in config order), the `""`-absent
   marker, and last-NON-EMPTY-wins precedence (`empty.in.override` —
   "last wins" would return `""`).
4. `layers` names in config order.
5. **Stat witnesses — the vacuous-pass ledger** (lib.rs:4504-4507 rule):
   `num_keys: 14` (flattened LEAVES: 13 declared + 1 override-only — NOT the
   13 top-level declared keys), `num_layers: 2`, `load_success: 1`,
   `override_dir_not_exists: 1` are the FOUR real witnesses. The five
   `value: 0` entries pass vacuously when the name is absent; their
   envoy-rust presence is pinned in-process by
   `envoy-bin::runtime_stats::registers_all_nine_runtime_stats_with_measured_values_and_kinds`.

## Deliberately excluded (recorded, not witnessed)

- Unquoted `y`/`n`/`on`/`off` (CF-108-4, ADR-0173: YAML 1.1 booleanizes
  upstream, YAML 1.2 does not here).
- Floats other than `1.5` (CF-108-5, ADR-0174: upstream preserves the raw
  SOURCE TEXT — `1e6`→`"1e6"`, `1.50`→`"1.50"` — envoy-rust renders `f64`
  Display; `1.5` is the Display-stable agreeing cell). Non-finite floats
  likewise (`".nan"` upstream vs `"NaN"` here).
- `numerator`-bearing nested maps (CF-108-3: protobuf text-format upstream).
- The `envoy.reloadable_features.` prefix (non-fatal `envoy_bug` stderr noise).
- `POST /runtime_modify` (CF-108-2: upstream POST-only / 405-on-GET;
  envoy-rust 404).
- NOT-MEASURED upstream cells: same-layer flattened-key collision (M-1),
  empty nested map value (M-7), empty/dot-bearing key segments (N-5),
  explicit-null `static_layer:` (N-4).

## Expected values

Measured against `envoyproxy/envoy:v1.33.0`
(`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`)
with this fixture's exact config at the 108.2 PLAN-write (2026-08-08):
1508-byte responses, per-request key-order shuffle (distinct md5s), 14
entries, `layers: ["base_layer","override_layer"]`, and the nine stat values
in `expectations.yaml`. Re-measured live by every fixture run — the YAML
carries the values, this README carries the provenance.
```

- [ ] **Step 5: verify + commit.** `cargo test -p differential --test
  runtime_static_layer` still green (contract edits touch no code, but run it
  — cheap insurance). Commit:
  `git add docs/envoy-rust/BEHAVIOR_CONTRACT.md tests/fixtures/0087-runtime-static-layer/README.md && git commit -m "phase 108.2 task 5: BEHAVIOR_CONTRACT ## Runtime + fixture 0087 README"`

---

### Task 6: PROGRESS.md records + the full regression sweep

**Files:**
- Modify: `docs/envoy-rust/phases/108.2-runtime-admin-endpoint-and-fixture/PROGRESS.md`

- [ ] **Step 1: Record the gate-(d) disposition explicitly** in PROGRESS.md:
  this slice adds NO fuzz target and NO corpus seed; `ci.yml` needs NO new
  step; the pre-existing `parse_bootstrap` short-budget run already covers
  the only parser this slice touches (it does not touch it — the parser
  landed in 108.1). SPEC §9(d) requires this RECORDED rather than silently
  skipped.

- [ ] **Step 2: The full workspace sweep** (state 4 re-runs this; running it
  at state-3 close catches cross-crate breakage early — the E0063 lesson):
  `cargo build --workspace --all-targets`, then
  `cargo test --workspace --no-fail-fast > /tmp/sweep-108-2.log 2>&1`
  (full redirect, NEVER `tail`; census failures by the
  `---- <name> stdout ----` markers). Expected: the deterministic five-member
  host-flake core (four `access_log_*_upstream_reset` + 
  `admin_config_dump_server_info`) plus an open-ended startup-race tail —
  classify any RED by ISOLATION, never by text; **`admin_*` and `runtime_*`
  REDs overlap this phase's surface, so read the failure TEXT before
  classifying**. The test-count identity: local `passed + failed` must equal
  **2180** (2170 CI baseline on `ced6802` + 10 new: 6 envoy-admin + 2
  envoy-bin + 1 differential-lib + 1 fixture binary), binaries **164**
  (163 + `runtime_static_layer`).

- [ ] **Step 3: Commit** any PROGRESS.md remainder:
  `git commit -m "phase 108.2 task 6: gate-(d) record + regression sweep"` —
  and leave `STATE.md` advancement to the SESSION-level close (the state-3
  session updates STATE.md → state 4 with the ADR-0035 relocation as its
  final act, per the standing per-session protocol).

---

## §6.1 gate — the size re-derivation, bottom-up

**This is not an estimate — it is a MEASUREMENT.** The whole plan was
pre-flighted in a scratch worktree and the diff measured on disk
(`git diff --numstat HEAD -- . ':(exclude)docs/'` after `git add -N`):

| File | + | − |
|---|---:|---:|
| `crates/envoy-admin/src/endpoint.rs` | 280 | 1 |
| `crates/envoy-bin/src/runtime_stats.rs` | 164 | 0 |
| `crates/envoy-bin/src/main.rs` | 5 | 0 |
| `tests/differential/src/lib.rs` | 72 | 0 |
| `tests/differential/tests/runtime_static_layer.rs` | 18 | 0 |
| fixture `0087` two YAMLs | 129 | 0 |
| fixture `0087` `expectations.yaml` | 121 | 0 |
| **measured pre-flight net** | **789** | **1** → **788** |

Plus the README (~115, Task 5, inside `tests/fixtures/` so it COUNTS) →
**≈905 projected net**. Calibration: `76.2` grew **+24%** from its §5.2
review re-entry (1265 → 1568); at +24% this slice lands near **1120** — still
**~25% under the ~1500 gate**. The SPEC's ≈655 was an UNDER-estimate (the
usual direction; the growth is in `endpoint.rs` tests + the README the SPEC's
D6 table under-counted), but not by enough to matter. **Tasks: 6** against
the ~25 threshold.

**VERDICT: the §6.1 gate does NOT fire. No split.** §6.1's mid-execution
trigger stays ARMED: if any task's sub-steps pass ~10 items or running net
LoC crosses ~1500 before Task 6, STOP and split with a new ADR (next free
after this session: ADR-0175).

**Predicted CI identity at state 4** (a PREDICTION, not a baseline): passed
`2170 + 10 = 2180`, binaries `163 + 1 = 164`, on the run for the state-3/4
commits. Baseline = the last commit WITH a CI run (`ced6802`, run
`31260569093`).

---

## Self-review against the SPEC

**1. Spec coverage:**

| SPEC deliverable | Task |
|---|---|
| D4 — variant + `from_path` + `allowed_method` + `render_with` + renderer (via `json_pretty_200`) | 1 |
| D4 — the two convention-only tests updated deliberately | 1 (Step 5) |
| D5 — nine stats, unconditional, dotted names, smallest-wiring precedent | 2 |
| D6 — fixture 0087: 2 `/runtime` scrapes, one subtree each, empty allow-lists, two static layers, all scalar shapes, 2-level nesting | 3, 4 |
| D6 — stats witnessed (four non-zero + the vacuous five recorded) | 2 (in-process), 3 (bilateral), README (ledger) |
| D9 — `## Runtime` + admin-shapes row + stat mapping + 0011 prose correction | 5 |
| §5 — the local fixture run RECORDED | 4 (Step 3) |
| §9(d) — no-new-fuzz-target recorded explicitly | 6 |
| REVIEW §8 ob.1 — CF-108-5 three axes measured BEFORE any float | §0.3 (done at plan-write; ADR-0174) |
| REVIEW §8 ob.2 — no `y`/`n`/`on`/`off` in 0087 | Task 3 YAML + README |
| REVIEW §8 ob.3 — M-1/M-7/N-5/N-4 in the NOT-MEASURED list | §0.5, Task 5 contract text, README |
| REVIEW §8 ob.4 — M-5 `from_layers` disposition | DD-2 (route around; both callers use `from_bootstrap`) |
| REVIEW §8 ob.5 — M-6 positive Serialize pin | DD-5, Task 1's `config_dump_serializes_layered_runtime_positively` |
| Parent-108 close | this slice's state-6 (rows `108.2` + `108` flip together — NOT this plan's job) |

**Non-goals honoured:** no config-schema/store reshaping; no
`disk_layer`/`rtds_layer`/`admin_layer`; no `/runtime_modify`; no `0011`
edit; no consumer wiring; no FractionalPercent rendering; no hot restart;
`/stats`,`/healthcheck/*` stay fixture-unwitnessed (the /stats content-type
divergence observed in passing — upstream `text/plain; charset=UTF-8` vs our
`text/plain` — is NOTED here, out of scope, unfixed).

**2. Placeholder scan:** no TBDs; every code step carries the literal,
pre-flighted, `fmt`-canonical code; every run step carries the command and
its expected output.

**3. Type consistency:** `RuntimeBody`/`RuntimeEntryBody`/`render_runtime`
(Task 1), `register_runtime_stats` (Task 2), `expected_stats:
Vec<KeepAliveExpectedStat>` (Task 3) are spelled identically at declaration,
wiring and test sites. The four non-zero witness names in Task 3's YAML match
Task 2's registration strings byte-for-byte.

---

## Handoff

**This plan is the state-2 deliverable; nothing in it may be executed by the
session that wrote it** (§5.1; ADR-0127). The next session runs §5 **state
3** via `superpowers:subagent-driven-development` or
`superpowers:executing-plans`, appending to `PROGRESS.md` per task.

Before Task 1 that session must: `git status --porcelain` +
`git fetch origin --prune` (a parallel workstream is active — leave the
`.claude/worktrees/agent-*` worktrees and the sibling containers alone);
re-derive this plan's `endpoint.rs`/`lib.rs`/`main.rs` anchors BY TEXT (they
were measured at `ced6802` and drift); and note the pre-flight evidence in §0
is a CLAIM to re-establish, not a result to inherit — the 76.2 lesson is that
a pre-flighted plan measured THREE defects anyway. The one defect THIS
pre-flight caught (DD-7) is already folded in; the second (the echo
`typed_config` boot-fatal on envoy-rust) became DD-3's name-only spelling;
the third (`NoRuntime` on admin-less validator-path test yamls) is folded
into Task 2's yamls. Every expected count in this plan (103 / 2 / 164 / 2180
/ 164 binaries) is stated so a mismatch reads as a SIGNAL, not a surprise.
