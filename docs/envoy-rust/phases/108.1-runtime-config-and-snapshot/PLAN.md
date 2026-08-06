# Sub-phase 108.1 — `layered_runtime` config surface + runtime snapshot store — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Written at the §5 state-2 PLAN-write on 2026-08-06**, from `docs/envoy-rust/phases/108.1-runtime-config-and-snapshot/SPEC.md`. Every `file:line` anchor below was **RE-DERIVED BY TEXT on disk at this commit** (`fb143376e58aa8726cc248a8cc86e817c9b16ed2`), not transcribed from the SPEC — `crates/envoy-config/src/bootstrap.rs` is **21 069** lines and drifts constantly.
>
> **DO NOT IMPLEMENT THIS PLAN IN THE SESSION THAT WROTE IT.** §5.1 / ADR-0127: one §5 state per session; the context that wrote an artifact must not grade it.

**Goal:** Land the `layered_runtime` / `static_layer` config schema, its four reject-direction validators, and the in-memory runtime snapshot store inside `crates/envoy-config` — witnessed entirely in-process, with **no new differential fixture** and **no admin endpoint** (those are sibling `108.2`).

**Architecture:** Schema types (`LayeredRuntime`, `RuntimeLayer`, `RuntimeValue`) live in `crates/envoy-config/src/bootstrap.rs` alongside every other serde schema type — this follows the landed `HeaderMatcher` / `RedirectAction` precedent. The snapshot **engine** (flattening, slot assignment, precedence) lives in a NEW sibling module `crates/envoy-config/src/runtime.rs` — this follows the landed `crates/envoy-config/src/matcher.rs` precedent, where the schema type sits in `bootstrap.rs` and the engine that interprets it sits in its own module. **No new crate** (SPEC §1 D3, V-3 DECIDED at the split): `envoy-config` is already a dependency of `envoy-http1` and `envoy-admin`, so the store adds no dependency edge and creates no cycle.

**Tech Stack:** Rust (pinned stable 1.95.0 via `rust-toolchain.toml`), `serde` + `serde_yaml 0.9.34` (already direct dependencies of `envoy-config`), `thiserror`. **No new dependency of any kind.**

---

## Global Constraints

Every task's requirements implicitly include this section.

- **TDD, no exceptions (D-3.1).** Every task writes its failing test FIRST, runs it to see it fail for the RIGHT reason, then writes the minimal implementation. A test that passes before the implementation is written is not evidence — see the RED-verification note in each task.
- **`#![forbid(unsafe_code)]` holds at every crate root (D-3.8).** `crates/envoy-config/src/lib.rs:1` already carries it; the new `runtime.rs` module inherits it. Do not add a crate-level opt-out.
- **No new dependency, no `Cargo.toml` change, no `Cargo.lock` change.** If a task appears to need one, STOP — that is a design error, not a licence.
- **No `ENVOY_TARGET.md` change (D-3.7), no `rust-toolchain.toml` change (D-3.9).**
- **No landed artifact of any closed phase is edited (D-3.5).** Specifically: do NOT edit any `76`/`76.1`/`76.2` artifact, and do NOT fix any banked Minor or Nit from those reviews (§6.3 — a phase picks its scope, it does not clear a backlog).
- **`ADR-0028` is NOT lifted.** Preserve ADR-0016 + ADR-0124 + ADR-0131 + ADR-0133 + ADR-0136/0137 + ADR-0139 + ADR-0140–0143 + ADR-0144–0149 + ADR-0150 through **ADR-0172**.
- **Never weaken a fixture; never trim `tests/conformance/h2spec/known-failures.txt`** (21 LINES holding exactly ONE real entry, `3.5/2`; lines 1–19 are a header comment and line 20 is blank — "21" is a line count, never a failure count).
- **`108.1` EDITS NONE of the eleven "no runtime subsystem" assertions** (SPEC §2 N-9). In particular `runtime_key_is_rtds_inert` (`crates/envoy-http1/src/hcm.rs:5641`) **keeps its name and its wording** — this slice builds a `static_layer` store that nothing reads. It is not RTDS and it wires neither the `RuntimeUInt32` nor the `RuntimeFractionalPercent` consumer.
- **Gate (e) must be clean at EVERY task boundary:** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`. **Known hazard, measured at 76.2:** a task that adds a non-test item whose only non-test consumer arrives in a LATER task fails `clippy -D warnings` with `never constructed` / `never used` at that boundary. This plan is ordered to avoid that (see the ordering note below); if a boundary is nevertheless unachievable, **record it in `PROGRESS.md` rather than suppressing it with `#[allow]`**.
- **`cargo clippy` prints `Checking`, NOT `Compiling`.** A clippy exit 0 with ZERO `Checking` lines is a fully-cached no-op, not evidence. Gate on the line count, never on the exit code or the duration.
- **Never pipe a verification run through `tail`.** Redirect full output to a file. Use `--no-fail-fast` BEFORE the `--`. `0 passed; N filtered out` is NOT a pass.
- **This slice adds NO fixture, NO fuzz target, NO `ci.yml` step, NO `deny.toml` change, NO `BEHAVIOR_CONTRACT.md` change, NO `expectations.yaml` change.** All of those belong to sibling `108.2`.
- **The `perl -0pi -e` mutation commands in Tasks 4, 5, 6 and 7 are WHITESPACE-SENSITIVE and their patterns encode this plan's rendering of the code, not necessarily `cargo fmt`'s.** If a pattern does not match, the file is left UNMUTATED and the subsequent test PASSES — which reads exactly like "the mutation survived" and would be a false adjudication. **That is why every mutation step is immediately followed by a `grep` that must print the `MUTATED` marker. Treat a silent no-match as a failed step, not a result:** re-derive the target text with `sed -n` and hand-edit if necessary. Every mutation belongs in its own scratch `git worktree` created `--detach` at HEAD, never in the main tree — a parallel workstream's `git checkout` can silently revert an in-place mutation.

### Task ordering note (why this order, and the clippy-boundary hazard)

Tasks 1→7 are ordered so that every non-test item acquires a non-test consumer within its own task or the next one, and Task 3 (the `ConfigError` variants) lands **immediately before** Task 4 (their only consumer). `ConfigError` is a `pub enum` in a library crate, so unused variants do **not** trip `dead_code` — Task 3 is safe standing alone. The one genuinely exposed boundary is Task 5, whose `runtime.rs` items are consumed by Task 7; Task 5 therefore ships its own `pub` surface plus its unit tests, and `pub` items in a library crate are not `dead_code`. **Expected: no `#[allow]` is needed anywhere in this plan.**

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | Modify | Add `RuntimeValue`, `RuntimeLayer`, `LayeredRuntime` schema types; add the `Bootstrap.layered_runtime` field; add `validate_layered_runtime` and call it from `validate()`. |
| `crates/envoy-config/src/runtime.rs` | **Create** | The snapshot engine: `RuntimeSnapshot`, `RuntimeEntry`, arbitrary-depth flattening, per-layer slot assignment, last-non-empty precedence. No serde, no I/O. |
| `crates/envoy-config/src/lib.rs` | Modify | Declare `pub mod runtime;`; re-export the new schema types; add the five new `ConfigError` variants. |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml` | **Create** | Fuzz corpus seed exercising the new parse path. |
| `crates/envoy-config/fuzz/.gitignore` | Modify | The `!`-un-ignore line **without which the seed is silently untracked and invisible to CI**. |
| `docs/envoy-rust/phases/108.1-runtime-config-and-snapshot/PROGRESS.md` | **Create** | Running log, appended per task. |
| `docs/envoy-rust/DECISIONS.md` | Modify (Task 9 only) | **ADR-0173** recording the CF-108-4 disposition. |

**Measured, so nobody has to re-derive it:** `git grep 'layered_runtime\|LayeredRuntime\|static_layer\|StaticLayer' -- 'crates/**/*.rs'` returns **ZERO** hits, and `git grep -l layered_runtime -- tests/` returns **ZERO** files. There is no partial implementation and no fixture that would change behaviour. This slice is purely additive.

---

## Design decisions this plan settles (read before Task 1)

### DD-1 — CF-108-4, the YAML-1.1 divergence: **RECORD IT, DO NOT NORMALISE.** This is the SPEC's one open design question.

SPEC §2 N-2 measured that upstream Envoy's YAML parser is **YAML 1.1**, so an unquoted `y` / `n` / `on` / `off` booleanizes (`key: y` → JSON `true` → `/runtime` `final_value` `"true"`), while `serde_yaml` implements the **YAML 1.2 core schema** where unquoted `y` is the string `"y"`. The SPEC left D1 to choose between normalising at parse time and recording the divergence.

**MEASURED AT THIS PLAN-WRITE, against the workspace's exact pinned `serde_yaml 0.9.34`, in a standalone scratch binary:**

```
a_unquoted_y     => Str("y")
b_quoted_y       => Str("y")
---- quoted vs unquoted y distinguishable? false ----
raw serde_yaml::Value x=String("y") z=String("y")
raw equal? true
```

**`y` and `"y"` are indistinguishable — not merely through an untagged enum, but at the `serde_yaml::Value` level itself.** The quoting bit is destroyed by the scanner before any serde code runs.

This makes normalisation **not implementable**, not merely undesirable. A parse-time transform that booleanized `String("y")` would necessarily also booleanize the *quoted* `"y"` — and upstream renders quoted `"y"` as `"y"` (SPEC §2 N-2, row 5, MEASURED). So normalising would **fix one divergence by minting a second one in the opposite direction**, on a spelling that is equally legal. The only correct normalisation is replacing the YAML scanner, which is out of scope and would be a D-3.2 foundations decision.

**DECISION: envoy-rust follows YAML 1.2 (`y` → `"y"`). The divergence is RECORDED as CF-108-4, made visible by a doc comment on `RuntimeValue` and PINNED by a test (Task 1 Step 7) so it is deliberate rather than accidental.** It is differentially unobservable in this slice (no fixture) and `108.2` must keep `y`/`n`/`on`/`off` spellings out of fixture `0087`.

**No ADR is fired by this session.** The decision is recorded here with its measurement, and **Task 9 fires ADR-0173 when the code lands** — matching the measured precedent that the `76.2` state-2 PLAN-write added no ADR (head stayed `ADR-0170`). The next session must still re-derive the head as **ADR-0172**, next free **ADR-0173**.

### DD-2 — `JsonFormatValue` already exists and MUST NOT be reused. The SPEC's D1 claim is refuted.

`108.1/SPEC.md` §1 D1 states the recursive value type "is new to the codebase — nothing in `bootstrap.rs` currently models a recursive YAML value." **MEASURED ON DISK AT THIS PLAN-WRITE: that is false.** `pub enum JsonFormatValue` at `crates/envoy-config/src/bootstrap.rs:936-960` is exactly such a type — `#[serde(untagged)]`, recursive over `Object(BTreeMap<String, JsonFormatValue>)` and `Array(Vec<JsonFormatValue>)`, with a recursive validator `validate_json_format_value` at `crates/envoy-config/src/bootstrap.rs:5568`.

**Reusing it would be a bug, for two independent reasons, both read off its own doc comment at `bootstrap.rs:940-942`:**

1. **It cannot represent numbers.** "NUMERIC literals are NOT accepted: a YAML number matches no `#[serde(untagged)]` arm → boot-reject (ADR-0094 §D / CF-39-1)." A runtime key `some.key: 42` is legal upstream and MEASURED (SPEC §2 N-3). Reusing `JsonFormatValue` would boot-reject it.
2. **Its `Format(String)` arm carries access-log semantics.** Every string leaf is compiled as an access-log command-operator format string by `validate_json_format_value` (`bootstrap.rs:5568-5590`), so a runtime value containing `%` would be parsed as an operator and could boot-reject.

**A NEW type is still required.** What changes is the risk profile, not the work: `JsonFormatValue` is a proven in-tree template for the untagged-recursive shape, and `validate_json_format_value` is a proven template for the recursive walk. **The estimate is unchanged; the design confidence is higher.** Task 1's doc comment must name `JsonFormatValue` and say why it was not reused, so a future reader does not "simplify" the two together.

### DD-3 — the three unsupported arms are RECOGNIZED-then-rejected, not left to `deny_unknown_fields`.

`RuntimeLayer` carries `#[serde(deny_unknown_fields)]`, so an undeclared `disk_layer:` key would fail with an opaque serde error. The landed house precedent is `HashPolicy` (`crates/envoy-config/src/bootstrap.rs`, fields `cookie` / `connection_properties` / `query_parameter` / `filter_state` typed `Option<serde_yaml::Value>` and rejected by `validate_hash_policy` at `bootstrap.rs:2344`), whose doc states the narrowing should surface "as a precise `ConfigError` … rather than an opaque serde unknown-field error."

`disk_layer`, `rtds_layer` and `admin_layer` are therefore declared as `Option<serde_yaml::Value>` and rejected by `validate_layered_runtime` with a precise variant. **This also makes the oneof cardinality count correct** — two arms set produces "more than one specifier" (matching upstream's MEASURED `'admin_layer' has already been set … as part of a oneof`) rather than a serde error. Banked as **CF-108-1**.

### DD-4 — untagged arm ORDER is load-bearing. MEASURED at this PLAN-write.

```
Bool,Int,Float,Str   OK  {"a": Int(42),   "b": Float(1.5), "c": Bool(true), "d": Float(1.0)}
Bool,Float,Int,Str   OK  {"a": Float(42.0), "b": Float(1.5), "c": Bool(true), "d": Float(1.0)}
```

With `Float` ahead of `Int`, the integer `42` binds to `Float(42.0)`. It happens to stringify to `"42"` either way, so **no test would catch the mistake** — which is exactly why the order must be pinned by a doc comment and by Task 1 Step 7's `Int` assertion. `JsonFormatValue`'s own doc comment (`bootstrap.rs:945-947`) makes the same point for the same reason.

### DD-5 — float stringification is UNMEASURED beyond `1.5`. Pin only what was measured. **Opens CF-108-5.**

SPEC §2 N-3 measured exactly one float: `my.float.key: 1.5` → JSON `"1.5"` → `/runtime` `"1.5"`. Rust's `f64` `Display`, MEASURED at this PLAN-write:

```
f64 1.5   -> "1.5"      f64 1.0  -> "1"        f64 -0.0 -> "-0"
f64 1e6   -> "1000000"  f64 1e-7 -> "0.0000001"
```

Only the `1.5` cell is known to agree with upstream. `1.0 → "1"` and `1e6 → "1000000"` are **plausible but UNMEASURED**, and this is precisely the "protobuf-`double` JSON formatting `1e+06`/`"1.5"` rabbit hole" that `CF-39-1` deferred for `JsonFormatValue` (`bootstrap.rs:941-942`). **Do not pin an unmeasured cell as if it were measured.** Task 1 pins `1.5` only; the rest is recorded as **CF-108-5 [NEW, opened by this PLAN-write]** and must be re-measured against the pinned image before any float appears in fixture `0087`.

### DD-6 — value shapes that match NO arm. MEASURED at this PLAN-write.

```
null            ERR data did not match any variant of untagged enum
sequence [1,2]  ERR data did not match any variant of untagged enum
absent value    ERR data did not match any variant of untagged enum
1e20 literal    ERR invalid type: integer `100000000000000000000` as u128, expected any value
i64::MAX+1      OK  Float(9.223372036854776e18)     <-- silently widens to f64
numeric key `1:` OK  key becomes the String "1"
empty string    OK  Str("")                          <-- required by SPEC §2 N-7
```

`null`, sequences and absent values boot-reject. Upstream's behaviour for them is **UNMEASURED**, so these are recorded reject-direction divergences under the ADR-0049 all-fatal posture, consistent with CF-108-1 — **not** silently unhandled (§6.3). Task 1 Step 7 pins the reject VERDICT (error text is not part of the equivalence contract, §7.2).

---

### Task 1: The `RuntimeValue` recursive value type

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — insert the type immediately BEFORE `pub enum JsonFormatValue` (currently at `bootstrap.rs:948`; **locate it by TEXT, not by line number**). Tests go in the `#[cfg(test)] mod tests` module whose `mod tests {` is at `bootstrap.rs:5789` and whose `use super::*;` is at `bootstrap.rs:5790`.

> ⚠ **`JsonFormatValue` carries a doc comment at `bootstrap.rs:936-946`, immediately above its `#[derive]` at `:947`.** Inserting "immediately before the `#[derive]`" would ORPHAN that doc block onto the new type and leave `JsonFormatValue` undocumented — this is exactly the landed 76.1 M-1 defect (`RedirectResponseCode` orphaned `RouteAction`'s doc). **Insert ABOVE the `/// A `json_format` value` line, not above the `#[derive]`.** Nothing in gate (e) catches this: `envoy-config` enables no `missing_docs` lint and `cargo fmt` does not reflow doc comments. After the edit, verify with `sed -n '/^\/\/\/ A `json_format` value/,+2p' crates/envoy-config/src/bootstrap.rs`.

**Interfaces:**
- Consumes: nothing (first task).
- Produces: `pub enum RuntimeValue { Bool(bool), Int(i64), Float(f64), Str(String), Map(std::collections::BTreeMap<String, RuntimeValue>) }` and the inherent method `pub fn stringify(&self) -> String`. Tasks 2, 5 and 6 depend on both names exactly as written.

- [ ] **Step 1: Write the failing test — scalar arms and arm order**

Add to the `mod tests` block in `crates/envoy-config/src/bootstrap.rs`:

```rust
    // --- 108.1 Task 1: RuntimeValue (SPEC §2 N-2/N-3/N-4/N-7; PLAN DD-1/DD-4/DD-5/DD-6) ---

    /// Parse a bare `static_layer` map body into the value model. Used by every
    /// RuntimeValue test below so the tests exercise the SAME serde path the
    /// real `RuntimeLayer.static_layer` field uses.
    fn runtime_values(yaml: &str) -> std::collections::BTreeMap<String, RuntimeValue> {
        serde_yaml::from_str(yaml).expect("static_layer body must parse")
    }

    #[test]
    fn runtime_value_binds_each_scalar_arm_in_declared_order() {
        let m = runtime_values(
            r#"
b_true: true
b_false: false
i_pos: 42
i_neg: -7
f_frac: 1.5
s_plain: hello
s_empty: ""
"#,
        );
        // DD-4: `Int` MUST precede `Float`. With the arms reversed, `42` binds
        // to `Float(42.0)` and still stringifies to "42" — so this assertion on
        // the VARIANT, not on the stringification, is the only thing that
        // catches the mistake.
        assert_eq!(m["b_true"], RuntimeValue::Bool(true));
        assert_eq!(m["b_false"], RuntimeValue::Bool(false));
        assert_eq!(m["i_pos"], RuntimeValue::Int(42));
        assert_eq!(m["i_neg"], RuntimeValue::Int(-7));
        assert_eq!(m["f_frac"], RuntimeValue::Float(1.5));
        assert_eq!(m["s_plain"], RuntimeValue::Str("hello".to_string()));
        // SPEC §2 N-7: an explicitly-empty string is a legitimate entry.
        assert_eq!(m["s_empty"], RuntimeValue::Str(String::new()));
    }
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p envoy-config --lib -- bootstrap::tests::runtime_value_binds_each_scalar_arm_in_declared_order 2>&1 | tee /tmp/t1s2.log
```

Expected: **compile failure**, `error[E0433]: failed to resolve: use of undeclared type `RuntimeValue`` (and/or `cannot find type RuntimeValue in this scope`).

> A compile error IS the correct RED here, because the type does not exist yet. **But a compile error is NOT a valid RED for a behavioural mutation check** — gate those on the existence of a `test result:` line, never on the exit code (measured at the 76.2 §5.2 re-entry).

- [ ] **Step 3: Write the minimal implementation**

Insert into `crates/envoy-config/src/bootstrap.rs` immediately above the line `/// A `json_format` value — a single `google.protobuf.Struct` value (ADR-0094`:

```rust
/// 108.1 D1: one value inside a `layered_runtime` `static_layer` map — a scalar
/// or a nested map, recursive to arbitrary depth (SPEC §2 N-4: `my.nested: {sub_key: v,
/// deeper: {leaf: w}}` yields entries `my.nested.sub_key` AND `my.nested.deeper.leaf`
/// with NO intermediate entry).
///
/// **NOT `JsonFormatValue` (below), and the two must never be merged.** That type
/// cannot represent NUMBERS by design (ADR-0094 §D / CF-39-1) — a runtime key
/// `some.key: 42` is legal upstream and MEASURED — and its `Format(String)` arm
/// compiles every string leaf as an access-log command-operator format string
/// (`validate_json_format_value`), so a runtime value containing `%` would
/// boot-reject. Different semantics, same shape; keep them apart.
///
/// **`#[serde(untagged)]` arm ORDER is load-bearing.** `Int` MUST precede
/// `Float`: MEASURED, with the two reversed the integer `42` binds to
/// `Float(42.0)`. It stringifies to `"42"` either way, so only an assertion on
/// the VARIANT catches the mistake.
///
/// **CF-108-4 — envoy-rust follows YAML 1.2; upstream Envoy is YAML 1.1.** An
/// unquoted `y`/`n`/`on`/`off` booleanizes upstream (`key: y` → `true` →
/// `final_value` `"true"`) but is a plain string here. This is RECORDED, not
/// fixed: MEASURED against `serde_yaml 0.9.34`, unquoted `y` and quoted `"y"`
/// both deserialize to `String("y")` — indistinguishable at the
/// `serde_yaml::Value` level itself — so no parse-time transform can booleanize
/// the first without also booleanizing the second, and upstream renders quoted
/// `"y"` as `"y"`. Normalising would replace one divergence with another.
///
/// **CF-108-5 — float rendering beyond `1.5` is UNMEASURED.** Only
/// `1.5` → `"1.5"` was measured upstream. Rust renders `1.0` → `"1"` and
/// `1e6` → `"1000000"`; both are plausible and neither is confirmed. Re-measure
/// before putting any float in a differential fixture.
///
/// A `null`, a sequence, an absent value, and an integer outside `i64` all match
/// NO arm and are boot-fatal (recorded reject-direction divergences under the
/// ADR-0049 all-fatal posture; upstream behaviour UNMEASURED).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuntimeValue {
    /// YAML `true` / `false` → `"true"` / `"false"` after stringification.
    Bool(bool),
    /// YAML integer. MUST stay ahead of `Float` — see the type doc.
    Int(i64),
    /// YAML float. MEASURED upstream only for `1.5` (CF-108-5).
    Float(f64),
    /// YAML string, including the empty string (SPEC §2 N-7).
    Str(String),
    /// YAML map → recurse. Flattened to dotted keys by
    /// [`crate::runtime::RuntimeSnapshot`]; produces no entry of its own.
    Map(std::collections::BTreeMap<String, RuntimeValue>),
}

impl RuntimeValue {
    /// 108.1 D3: render a SCALAR leaf the way upstream's `/runtime` does — every
    /// value stringifies (SPEC §2 N-3). `Map` is unreachable for a caller that
    /// flattens first; it renders as the empty string rather than panicking,
    /// because a `Map` leaf is not an entry at all and must never surface a value.
    pub fn stringify(&self) -> String {
        match self {
            RuntimeValue::Bool(b) => b.to_string(),
            RuntimeValue::Int(i) => i.to_string(),
            RuntimeValue::Float(f) => f.to_string(),
            RuntimeValue::Str(s) => s.clone(),
            RuntimeValue::Map(_) => String::new(),
        }
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cargo test -p envoy-config --lib -- bootstrap::tests::runtime_value_binds_each_scalar_arm_in_declared_order 2>&1 | tee /tmp/t1s4.log
```

Expected: `test result: ok. 1 passed; 0 failed`. **Assert the `1 passed` count — `0 passed; N filtered out` is a FALSE GREEN and exits 0.**

- [ ] **Step 5: Verify the doc comment was not orphaned**

```bash
grep -n -B2 '^pub enum JsonFormatValue' crates/envoy-config/src/bootstrap.rs
grep -n -B2 '^pub enum RuntimeValue' crates/envoy-config/src/bootstrap.rs
```

Expected: `JsonFormatValue` is still preceded by `#[serde(untagged)]` and `#[derive(...)]`, and its `/// A `json_format` value` doc block is still directly above ITS derive — not above `RuntimeValue`. Both types have their own doc block.

- [ ] **Step 6: Write the nesting + stringification test**

```rust
    #[test]
    fn runtime_value_nests_to_arbitrary_depth_and_stringifies_scalars() {
        let m = runtime_values(
            r#"
my.nested:
  sub_key: v
  deeper:
    leaf: w
"#,
        );
        // SPEC §2 N-4: the model keeps the nesting; FLATTENING is the snapshot
        // store's job (Task 5), not the value type's.
        let RuntimeValue::Map(outer) = &m["my.nested"] else {
            panic!("expected a Map, got {:?}", m["my.nested"]);
        };
        assert_eq!(outer["sub_key"], RuntimeValue::Str("v".to_string()));
        let RuntimeValue::Map(inner) = &outer["deeper"] else {
            panic!("expected a nested Map, got {:?}", outer["deeper"]);
        };
        assert_eq!(inner["leaf"], RuntimeValue::Str("w".to_string()));

        // SPEC §2 N-3: every scalar stringifies. Table-driven so a new measured
        // cell costs ONE line (the 76.2 design that measurably beat 22 separate
        // `#[test]` fns: 255 LoC vs ~400).
        let cells: &[(RuntimeValue, &str)] = &[
            (RuntimeValue::Bool(true), "true"),
            (RuntimeValue::Bool(false), "false"),
            (RuntimeValue::Int(42), "42"),
            (RuntimeValue::Int(-7), "-7"),
            // CF-108-5: `1.5` is the ONLY float cell MEASURED upstream.
            (RuntimeValue::Float(1.5), "1.5"),
            (RuntimeValue::Str("hello".to_string()), "hello"),
            (RuntimeValue::Str(String::new()), ""),
        ];
        for (v, expected) in cells {
            assert_eq!(&v.stringify(), expected, "stringify({v:?})");
        }
    }
```

- [ ] **Step 7: Write the divergence + reject-shape pins**

```rust
    #[test]
    fn runtime_value_follows_yaml_1_2_and_records_the_cf_108_4_divergence() {
        // CF-108-4 (PLAN DD-1). Upstream Envoy parses YAML 1.1, where unquoted
        // `y`/`n`/`on`/`off` booleanize: `key: y` → `true` → final_value "true".
        // envoy-rust follows serde_yaml's YAML 1.2 core schema, where they are
        // plain strings. This test PINS the divergence so it stays deliberate.
        //
        // It is NOT fixable at parse time: MEASURED, unquoted `y` and quoted
        // `"y"` both arrive as String("y") — the quoting bit is destroyed by the
        // scanner — and upstream renders quoted `"y"` as "y", so booleanizing
        // would mint an opposite-direction divergence on an equally legal
        // spelling.
        let m = runtime_values("a: y\nb: n\nc: on\nd: off\ne: \"y\"\n");
        for k in ["a", "b", "c", "d", "e"] {
            assert!(
                matches!(m[k], RuntimeValue::Str(_)),
                "CF-108-4: {k} must stay a string under YAML 1.2, got {:?}",
                m[k]
            );
        }
        assert_eq!(m["a"].stringify(), "y");
        assert_eq!(m["c"].stringify(), "on");
        // The two spellings are indistinguishable — this equality IS the reason
        // normalisation is impossible, not an incidental detail.
        assert_eq!(m["a"], m["e"]);

        // But real YAML 1.2 booleans DO bind to `Bool`.
        let t = runtime_values("k: true\n");
        assert_eq!(t["k"], RuntimeValue::Bool(true));
    }

    #[test]
    fn runtime_value_rejects_shapes_that_match_no_arm() {
        // PLAN DD-6. Recorded reject-direction divergences (ADR-0049 all-fatal);
        // upstream behaviour for these shapes is UNMEASURED. Error TEXT is not
        // part of the equivalence contract (§7.2) — only the VERDICT is pinned.
        for (label, yaml) in [
            ("null", "k: ~\n"),
            ("sequence", "k: [1, 2]\n"),
            ("absent value", "k:\n"),
            ("integer beyond i64", "k: 100000000000000000000\n"),
        ] {
            let r = serde_yaml::from_str::<std::collections::BTreeMap<String, RuntimeValue>>(yaml);
            assert!(r.is_err(), "{label} must be rejected, got {r:?}");
        }

        // Measured contrast: an integer just past i64::MAX silently WIDENS to
        // Float rather than rejecting. Pinned so the boundary is not mistaken
        // for a hard i64 limit.
        let m = runtime_values("k: 9223372036854775808\n");
        assert!(matches!(m["k"], RuntimeValue::Float(_)), "got {:?}", m["k"]);
    }
```

- [ ] **Step 8: Run all four tests**

```bash
cargo test -p envoy-config --lib -- bootstrap::tests::runtime_value 2>&1 | tee /tmp/t1s8.log
```

Expected: `test result: ok. 4 passed; 0 failed`. Assert the count is exactly 4.

- [ ] **Step 9: Gate (e) at this task boundary**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/t1clippy.log
grep -c '^ *Checking' /tmp/t1clippy.log
```

Expected: `fmt` exit 0 with no output; clippy exit 0. **The `Checking` count must be ≥ 1** — a zero means the run was fully cached and proved nothing.

- [ ] **Step 10: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 108.1 task 1: RuntimeValue recursive value type + stringify; CF-108-4 (YAML 1.2) and CF-108-5 (float rendering) pinned"
```

---

### Task 2: `LayeredRuntime` / `RuntimeLayer` schema and the `Bootstrap.layered_runtime` field

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — new types directly below `RuntimeValue`; new field on `pub struct Bootstrap` (`bootstrap.rs:10-38`, derive at `:8`, `#[serde(deny_unknown_fields)]` at `:9`).
- Modify: `crates/envoy-config/src/lib.rs` — add the three names to the `pub use bootstrap::{…}` re-export list (`lib.rs:16-49`).

**Interfaces:**
- Consumes: `RuntimeValue` (Task 1).
- Produces: `pub struct LayeredRuntime { pub layers: Vec<RuntimeLayer> }`; `pub struct RuntimeLayer { pub name: String, pub static_layer: Option<BTreeMap<String, RuntimeValue>>, pub disk_layer: Option<serde_yaml::Value>, pub rtds_layer: Option<serde_yaml::Value>, pub admin_layer: Option<serde_yaml::Value> }`; and `Bootstrap.layered_runtime: Option<LayeredRuntime>`. Tasks 4, 5, 6 and 7 depend on these names exactly.

> **Why `Option<LayeredRuntime>` and not `LayeredRuntime` + `#[serde(default)]`:** SPEC §2 N-8 measured that an ABSENT block yields `{"entries":{},"layers":[]}` with `num_layers: 0`, while `layered_runtime: {}` **or** `layered_runtime: {layers: []}` makes upstream synthesize ONE layer named the **EMPTY STRING** with `num_layers: 1`. A `#[serde(default)]` non-`Option` field collapses `None` and `Some(empty)` into the same value and **mints a divergence**. The `Option` is load-bearing — do not "simplify" it. This is the same reasoning the landed `RedirectAction` doc records for its own `Option`s (`bootstrap.rs:2205-2212`).

> **Why `skip_serializing_if`:** `Bootstrap` derives `Serialize` (`bootstrap.rs:8`) and is serialized by reference into the admin `/config_dump` (`crates/envoy-admin/src/endpoint.rs:532`, `ConfigDumpEntry::Bootstrap` at `:298`). A new always-serialized field would change `/config_dump` output for **every** existing fixture — fixture `0014-admin-config-dump-server-info` asserts it. With `#[serde(default, skip_serializing_if = "Option::is_none")]` and zero fixtures carrying `layered_runtime` (MEASURED: `git grep -l layered_runtime -- tests/` = 0 files), the field emits nothing and all 86 fixtures stay byte-identical. **This is the mechanism gate (b) rests on.**

- [ ] **Step 1: Write the failing test — absent vs empty vs populated**

```rust
    #[test]
    fn layered_runtime_absent_empty_and_populated_are_three_distinct_states() {
        // SPEC §2 N-8, MEASURED against envoyproxy/envoy:v1.33.0:
        //   no block            -> {"entries":{},"layers":[]}    num_layers 0
        //   layered_runtime: {} -> {"entries":{},"layers":[""]}  num_layers 1
        //   layers: []          -> {"entries":{},"layers":[""]}  num_layers 1
        // The Option is what keeps state 1 distinguishable from states 2/3.
        let base = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let absent = crate::parse_bootstrap(base).expect("valid");
        assert!(absent.layered_runtime.is_none(), "absent block must be None");

        let empty_block = crate::parse_bootstrap(&format!("{base}layered_runtime: {{}}\n"))
            .expect("empty layered_runtime must parse");
        assert!(
            empty_block
                .layered_runtime
                .as_ref()
                .expect("Some")
                .layers
                .is_empty(),
            "an empty block parses to Some(LayeredRuntime {{ layers: [] }})"
        );

        let empty_layers =
            crate::parse_bootstrap(&format!("{base}layered_runtime:\n  layers: []\n"))
                .expect("empty layers list must parse");
        assert!(
            empty_layers
                .layered_runtime
                .as_ref()
                .expect("Some")
                .layers
                .is_empty()
        );

        let populated = crate::parse_bootstrap(&format!(
            "{base}layered_runtime:\n  layers:\n  - name: base_layer\n    static_layer:\n      some.key: v\n"
        ))
        .expect("populated must parse");
        let lr = populated.layered_runtime.as_ref().expect("Some");
        assert_eq!(lr.layers.len(), 1);
        assert_eq!(lr.layers[0].name, "base_layer");
        assert_eq!(
            lr.layers[0].static_layer.as_ref().expect("static_layer")["some.key"],
            RuntimeValue::Str("v".to_string())
        );
    }
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p envoy-config --lib -- bootstrap::tests::layered_runtime_absent_empty_and_populated 2>&1 | tee /tmp/t2s2.log
```

Expected: compile failure — `no field `layered_runtime` on type `Bootstrap``.

- [ ] **Step 3: Write the schema types**

Insert into `crates/envoy-config/src/bootstrap.rs` directly below the `impl RuntimeValue { … }` block from Task 1:

```rust
/// 108.1 D1: `envoy.config.bootstrap.v3.LayeredRuntime` — the ordered layer
/// stack. Only the `static_layer` arm is implemented (SPEC §1 D2 / CF-108-1).
///
/// **`layers` being EMPTY is not the same as the block being ABSENT.** MEASURED
/// against `envoyproxy/envoy:v1.33.0`: no `layered_runtime:` at all yields
/// `{"entries":{},"layers":[]}` with `runtime.num_layers: 0`, whereas
/// `layered_runtime: {}` OR `layered_runtime: {layers: []}` makes upstream
/// synthesize ONE layer named the EMPTY STRING with `num_layers: 1`. That is why
/// `Bootstrap.layered_runtime` is an `Option` and this field is not — the
/// synthesis is performed by [`crate::runtime::RuntimeSnapshot::from_bootstrap`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct LayeredRuntime {
    #[serde(default)]
    pub layers: Vec<RuntimeLayer>,
}

/// 108.1 D1: one entry of `LayeredRuntime.layers` —
/// `envoy.config.core.v3.RuntimeLayer`. `name` plus a `layer_specifier` oneof of
/// which EXACTLY ONE arm must be set (MEASURED: absent → upstream
/// `field: "layer_specifier", reason: is required`; two arms → upstream
/// `'admin_layer' has already been set … as part of a oneof`).
///
/// **The three unimplemented arms are declared, not omitted (CF-108-1).** With
/// `deny_unknown_fields` an undeclared `disk_layer:` would fail as an opaque
/// serde unknown-field error; declaring them as `serde_yaml::Value` surfaces a
/// precise `ConfigError` instead AND makes the oneof cardinality count correct
/// when two arms are set. This is the landed `HashPolicy` recognize-then-reject
/// pattern. They are ACCEPTED by upstream and BOOT-FATAL here — a recorded
/// reject-direction divergence under the ADR-0049 all-fatal posture, and
/// differentially unobservable because a rejected config never reaches the wire.
///
/// `disk_layer` needs a filesystem watch (this host has virtiofs and no inotify),
/// `rtds_layer` needs an xDS cluster, and `admin_layer` is state-MUTATING via
/// `POST /runtime_modify` (CF-108-2). Each belongs to its own later phase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct RuntimeLayer {
    /// PGV-required, `min_len 1` upstream. Empty or absent is boot-fatal.
    #[serde(default)]
    pub name: String,
    /// The one implemented oneof arm: a map of runtime key → value, flattened to
    /// dotted keys at arbitrary depth by the snapshot store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_layer: Option<std::collections::BTreeMap<String, RuntimeValue>>,
    /// Recognized-but-unsupported arm (rejected by `validate_layered_runtime`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_layer: Option<serde_yaml::Value>,
    /// Recognized-but-unsupported arm (rejected by `validate_layered_runtime`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtds_layer: Option<serde_yaml::Value>,
    /// Recognized-but-unsupported arm (rejected by `validate_layered_runtime`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_layer: Option<serde_yaml::Value>,
}
```

- [ ] **Step 4: Add the `Bootstrap` field**

In `crates/envoy-config/src/bootstrap.rs`, inside `pub struct Bootstrap`, insert immediately AFTER the `pub dynamic_resources: Option<DynamicResources>,` line and BEFORE the `/// 18 D3: clusters loaded from the CDS file` doc block:

```rust
    /// 108.1 D1: `layered_runtime` — the runtime layer stack (ADR-0171/ADR-0172).
    /// `None` (absent) and `Some(LayeredRuntime { layers: [] })` are DIFFERENT
    /// states upstream and must stay different here — see `LayeredRuntime`.
    /// `skip_serializing_if` keeps `/config_dump` byte-identical for the 86
    /// pre-existing fixtures, none of which carries a `layered_runtime` block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layered_runtime: Option<LayeredRuntime>,
```

- [ ] **Step 5: Add the re-exports**

In `crates/envoy-config/src/lib.rs`, in the `pub use bootstrap::{…}` list (`lib.rs:16-49`), add `LayeredRuntime`, `RuntimeLayer` and `RuntimeValue` in **alphabetical position**. The list is alphabetically sorted and `cargo fmt` will re-wrap it: `LayeredRuntime` goes between `LbSubsetSelector` and `Listener`; `RuntimeLayer` and `RuntimeValue` go between `RuntimeFractionalPercent` and `RuntimeUInt32`.

- [ ] **Step 6: Run the test to verify it passes**

```bash
cargo test -p envoy-config --lib -- bootstrap::tests::layered_runtime_absent_empty_and_populated 2>&1 | tee /tmp/t2s6.log
```

Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 7: Write and run the `deny_unknown_fields` + `/config_dump`-inertness test**

```rust
    #[test]
    fn layered_runtime_rejects_unknown_keys_and_stays_out_of_config_dump_when_absent() {
        let base = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        // An unknown arm gives upstream `no such field`; here deny_unknown_fields
        // rejects it. Only the VERDICT is contracted (§7.2), not the text.
        let err = crate::parse_bootstrap(&format!(
            "{base}layered_runtime:\n  layers:\n  - name: l\n    bogus_layer: {{}}\n"
        ))
        .expect_err("an unknown layer arm must reject");
        assert!(matches!(err, crate::ConfigError::Yaml(_)), "got {err:?}");

        // Unknown key directly under `layered_runtime`.
        let err = crate::parse_bootstrap(&format!("{base}layered_runtime:\n  bogus: 1\n"))
            .expect_err("an unknown layered_runtime key must reject");
        assert!(matches!(err, crate::ConfigError::Yaml(_)), "got {err:?}");

        // Gate (b)'s mechanism: with no block, the field must be ABSENT from the
        // serialized bootstrap, so /config_dump is byte-identical for all 86
        // pre-existing fixtures.
        let b = crate::parse_bootstrap(base).expect("valid");
        let dumped = serde_json::to_string(&b).expect("serialize");
        assert!(
            !dumped.contains("layered_runtime"),
            "an absent layered_runtime must not appear in /config_dump; got {dumped}"
        );
    }
```

> **`serde_json` availability — MEASURED at this PLAN-write, no check needed:** `crates/envoy-config/Cargo.toml:18` already lists `serde_json = "1"` as a direct dependency, so this test adds none (Global Constraints hold).

```bash
cargo test -p envoy-config --lib -- bootstrap::tests::layered_runtime 2>&1 | tee /tmp/t2s7.log
```

Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 8: Gate (e) and commit**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/t2clippy.log
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 108.1 task 2: LayeredRuntime/RuntimeLayer schema + Bootstrap.layered_runtime (absent != empty, SPEC N-8)"
```

---

### Task 3: The five `ConfigError` variants

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` — append inside `pub enum ConfigError`, which spans **`lib.rs:74`** (`pub enum ConfigError {`) to **`lib.rs:1011`** (its closing `}`). Insert immediately AFTER the `RedirectSchemeRewriteConflict { listener: String, route: String },` variant — the current last variant — and BEFORE the closing `}`.

**Interfaces:**
- Consumes: nothing.
- Produces: five variants consumed by Task 4 — `EmptyRuntimeLayerName { position: usize }`, `DuplicateRuntimeLayerName { layer: String }`, `RuntimeLayerMissingSpecifier { layer: String }`, `RuntimeLayerMultipleSpecifiers { layer: String }`, `UnsupportedRuntimeLayerType { layer: String, arm: &'static str }`.

> **Measured before/after invariant:** the enum currently holds **125** variants and **125** `#[error(...)]` attributes over the span `lib.rs:74-1011` (cross-checked both ways). After this task both counts must be **130**. `ConfigError` is a `pub enum` in a library crate, so the variants standing without a consumer for one commit do **not** trip `dead_code` — this task's boundary is clippy-clean.

- [ ] **Step 1: Record the pre-edit census**

```bash
sed -n '75,1010p' crates/envoy-config/src/lib.rs | grep -cE '^    [A-Z][A-Za-z0-9]*( \{|\(|,|$)'
sed -n '75,1010p' crates/envoy-config/src/lib.rs | grep -c '#\[error('
```

Expected: `125` and `125`. If either differs, the file has drifted — re-derive the span with `grep -n '^pub enum ConfigError {' crates/envoy-config/src/lib.rs` and `awk 'NR>=74 && /^}$/ {print NR; exit}' crates/envoy-config/src/lib.rs` before proceeding.

- [ ] **Step 2: Write the variants**

```rust

    /// 108.1 D2: a `layered_runtime` layer has an empty or absent `name`.
    /// Upstream enforces PGV `min_len 1` (MEASURED). `position` is the layer's
    /// index within `layers` — the layer cannot be named, because its name is
    /// the thing that is missing.
    #[error("layered_runtime layer at position {position} has an empty name; a layer name is required and must be non-empty")]
    EmptyRuntimeLayerName { position: usize },

    /// 108.1 D2: two `layered_runtime` layers share a `name`. Upstream rejects
    /// this at a POST-PGV stage with the bare string `Duplicate layer name: <n>`
    /// (MEASURED). Error TEXT is not part of the equivalence contract (§7.2);
    /// only the reject VERDICT is.
    #[error("layered_runtime contains duplicate layer name `{layer}`; layer names must be unique")]
    DuplicateRuntimeLayerName { layer: String },

    /// 108.1 D2: a `layered_runtime` layer sets NO `layer_specifier` oneof arm.
    /// Upstream rejects with `field: "layer_specifier", reason: is required`
    /// (MEASURED).
    #[error("layered_runtime layer `{layer}` sets no layer_specifier; exactly one of static_layer/disk_layer/rtds_layer/admin_layer is required")]
    RuntimeLayerMissingSpecifier { layer: String },

    /// 108.1 D2: a `layered_runtime` layer sets MORE THAN ONE `layer_specifier`
    /// oneof arm. Upstream rejects with `'<arm>' has already been set … as part
    /// of a oneof` (MEASURED). Detecting this is why the three unimplemented
    /// arms are DECLARED rather than left to `deny_unknown_fields` — an
    /// undeclared arm would fail as an opaque serde error and could not be
    /// counted.
    #[error("layered_runtime layer `{layer}` sets more than one layer_specifier; they are members of one oneof and are mutually exclusive")]
    RuntimeLayerMultipleSpecifiers { layer: String },

    /// 108.1 D2 (CF-108-1): a `layered_runtime` layer uses `disk_layer`,
    /// `rtds_layer` or `admin_layer`. Upstream ACCEPTS all three; envoy-rust
    /// rejects them loudly under the ADR-0049 all-fatal posture. A RECORDED
    /// reject-direction divergence, differentially unobservable — a rejected
    /// config never reaches the wire. `disk_layer` needs a filesystem watch,
    /// `rtds_layer` an xDS cluster, `admin_layer` the state-mutating
    /// `POST /runtime_modify` (CF-108-2). Each belongs to its own later phase.
    #[error("layered_runtime layer `{layer}` uses `{arm}`, which envoy-rust does not implement; only static_layer is supported")]
    UnsupportedRuntimeLayerType { layer: String, arm: &'static str },
```

- [ ] **Step 3: Verify the post-edit census and that it still builds**

```bash
grep -n '^pub enum ConfigError {' crates/envoy-config/src/lib.rs
END=$(awk 'NR>=74 && /^}$/ {print NR; exit}' crates/envoy-config/src/lib.rs); echo "end=$END"
sed -n "75,$((END-1))p" crates/envoy-config/src/lib.rs | grep -cE '^    [A-Z][A-Za-z0-9]*( \{|\(|,|$)'
sed -n "75,$((END-1))p" crates/envoy-config/src/lib.rs | grep -c '#\[error('
cargo build -p envoy-config 2>&1 | tee /tmp/t3build.log
```

Expected: both counts **130**, and the build succeeds. **`grep 'Compiling envoy-config' /tmp/t3build.log` must be non-empty** — a fully-cached build proves nothing.

- [ ] **Step 4: Gate (e) and commit**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/t3clippy.log
git add crates/envoy-config/src/lib.rs
git commit -m "phase 108.1 task 3: five layered_runtime ConfigError variants (125 -> 130)"
```

---

### Task 4: `validate_layered_runtime` and its wiring into `validate()`

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — add the validator adjacent to the other `validate_*` functions (put it immediately BEFORE `fn validate_hash_policy`, which is currently at `bootstrap.rs:2344`; **locate by TEXT**), and add the call site inside `pub(crate) fn validate(bootstrap: &mut Bootstrap)` (currently at `bootstrap.rs:3446`).

**Interfaces:**
- Consumes: `LayeredRuntime`, `RuntimeLayer` (Task 2); the five `ConfigError` variants (Task 3).
- Produces: `pub(crate) fn validate_layered_runtime(lr: &LayeredRuntime) -> Result<(), crate::ConfigError>`. No later task calls it directly; `validate()` is its only caller.

> **Signature rationale:** it takes `&LayeredRuntime`, not `&Bootstrap`, so the `None` case is handled at the call site and the function is trivially unit-testable without constructing a whole `Bootstrap`. `pub(crate)` matches `validate_redirect_oneofs` (`bootstrap.rs:2677`); it is not `pub` because nothing outside the crate calls it.

> **Idempotency requirement:** `validate()` is called TWICE on a config that uses dynamic resources — once from `parse_bootstrap` (`crates/envoy-config/src/lib.rs:1025`) and again from `load_dynamic_resources` (`crates/envoy-config/src/lib.rs:1280`). `validate_layered_runtime` mutates nothing and is therefore trivially idempotent. **Do not make it normalize in place.**

- [ ] **Step 1: Write the failing tests — all four reject rules plus the three unsupported arms**

```rust
    // --- 108.1 Task 4: validate_layered_runtime (SPEC §2 N-12; PLAN DD-3) ---

    /// Build a `LayeredRuntime` from a layers-only YAML fragment, bypassing
    /// `parse_bootstrap` so the validator can be unit-tested in isolation.
    fn layered_runtime(yaml: &str) -> LayeredRuntime {
        serde_yaml::from_str(yaml).expect("layered_runtime fragment must parse")
    }

    #[test]
    fn validate_layered_runtime_accepts_one_and_two_static_layers() {
        // SPEC §2 N-6: TWO static layers with distinct names are LEGAL upstream
        // (num_layers: 2). This is what makes multi-layer precedence witnessable
        // inside 108.1 without the out-of-scope admin_layer.
        let ok = layered_runtime(
            r#"
layers:
- name: base_layer
  static_layer:
    shared.key: from_base
- name: override_layer
  static_layer:
    shared.key: from_override
"#,
        );
        super::validate_layered_runtime(&ok).expect("two distinct static layers are legal");

        // An empty layers list is legal (SPEC §2 N-8).
        super::validate_layered_runtime(&LayeredRuntime::default()).expect("empty layers is legal");

        // An empty static_layer map is legal — it is a set arm with no keys.
        let empty_arm = layered_runtime("layers:\n- name: l\n  static_layer: {}\n");
        super::validate_layered_runtime(&empty_arm).expect("an empty static_layer is a set arm");
    }

    #[test]
    fn validate_layered_runtime_rejects_empty_and_duplicate_names() {
        // Rule 1: PGV min_len 1 on `name`.
        let unnamed = layered_runtime("layers:\n- name: \"\"\n  static_layer: {}\n");
        match super::validate_layered_runtime(&unnamed).expect_err("empty name rejects") {
            crate::ConfigError::EmptyRuntimeLayerName { position } => assert_eq!(position, 0),
            other => panic!("expected EmptyRuntimeLayerName, got {other:?}"),
        }

        // An ABSENT name is the same rejection, not a serde error: `name` carries
        // #[serde(default)] so it arrives as the empty string.
        let absent_name = layered_runtime("layers:\n- static_layer: {}\n");
        assert!(
            matches!(
                super::validate_layered_runtime(&absent_name),
                Err(crate::ConfigError::EmptyRuntimeLayerName { position: 0 })
            ),
            "an absent name must reject exactly like an empty one"
        );

        // The position must identify the OFFENDING layer, not always 0.
        let second_unnamed = layered_runtime(
            "layers:\n- name: ok\n  static_layer: {}\n- name: \"\"\n  static_layer: {}\n",
        );
        match super::validate_layered_runtime(&second_unnamed).expect_err("rejects") {
            crate::ConfigError::EmptyRuntimeLayerName { position } => assert_eq!(position, 1),
            other => panic!("expected EmptyRuntimeLayerName at 1, got {other:?}"),
        }

        // Rule 2: duplicate names.
        let dup = layered_runtime(
            "layers:\n- name: same\n  static_layer: {}\n- name: same\n  static_layer: {}\n",
        );
        match super::validate_layered_runtime(&dup).expect_err("duplicate name rejects") {
            crate::ConfigError::DuplicateRuntimeLayerName { layer } => assert_eq!(layer, "same"),
            other => panic!("expected DuplicateRuntimeLayerName, got {other:?}"),
        }
    }

    #[test]
    fn validate_layered_runtime_enforces_oneof_cardinality_and_rejects_unsupported_arms() {
        // Rule 3: no arm at all.
        let none_set = layered_runtime("layers:\n- name: l\n");
        match super::validate_layered_runtime(&none_set).expect_err("no specifier rejects") {
            crate::ConfigError::RuntimeLayerMissingSpecifier { layer } => assert_eq!(layer, "l"),
            other => panic!("expected RuntimeLayerMissingSpecifier, got {other:?}"),
        }

        // Rule 4: more than one arm. Checked BEFORE the unsupported-arm check, so
        // static_layer + admin_layer reports the cardinality violation — which is
        // what upstream reports ('admin_layer' has already been set ...).
        let two_arms = layered_runtime("layers:\n- name: l\n  static_layer: {}\n  admin_layer: {}\n");
        match super::validate_layered_runtime(&two_arms).expect_err("two arms reject") {
            crate::ConfigError::RuntimeLayerMultipleSpecifiers { layer } => assert_eq!(layer, "l"),
            other => panic!("expected RuntimeLayerMultipleSpecifiers, got {other:?}"),
        }

        // CF-108-1: each unsupported arm ALONE is rejected, and names itself.
        // Table-driven so a future arm costs one line.
        for (yaml, expected_arm) in [
            ("layers:\n- name: l\n  disk_layer:\n    symlink_root: /srv\n", "disk_layer"),
            ("layers:\n- name: l\n  rtds_layer:\n    name: rtds\n", "rtds_layer"),
            ("layers:\n- name: l\n  admin_layer: {}\n", "admin_layer"),
        ] {
            let lr = layered_runtime(yaml);
            match super::validate_layered_runtime(&lr).expect_err("unsupported arm rejects") {
                crate::ConfigError::UnsupportedRuntimeLayerType { layer, arm } => {
                    assert_eq!(layer, "l");
                    assert_eq!(arm, expected_arm);
                }
                other => panic!("expected UnsupportedRuntimeLayerType({expected_arm}), got {other:?}"),
            }
        }
    }

    #[test]
    fn parse_bootstrap_runs_the_layered_runtime_validator() {
        // The wiring test: the validator must be reachable from the real entry
        // point, not merely callable in a unit test.
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
layered_runtime:
  layers:
  - name: same
    static_layer: {}
  - name: same
    static_layer: {}
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject via parse_bootstrap");
        assert!(
            matches!(err, crate::ConfigError::DuplicateRuntimeLayerName { .. }),
            "got {err:?}"
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

```bash
cargo test -p envoy-config --lib -- bootstrap::tests::validate_layered_runtime bootstrap::tests::parse_bootstrap_runs_the_layered_runtime_validator 2>&1 | tee /tmp/t4s2.log
```

Expected: compile failure — `cannot find function `validate_layered_runtime` in module `super``.

- [ ] **Step 3: Write the validator**

Insert into `crates/envoy-config/src/bootstrap.rs` immediately above the doc comment of `fn validate_hash_policy`:

```rust
/// 108.1 D2: the four MEASURED reject-direction rules for `layered_runtime`,
/// plus the CF-108-1 fail-loud rejection of the three unimplemented arms.
///
/// Rule order is deliberate and matches upstream's MEASURED order of complaint:
/// PGV `name` first (it is a field constraint), then oneof CARDINALITY (upstream
/// reports `'admin_layer' has already been set … as part of a oneof` for two
/// arms, so a `static_layer` + `admin_layer` config must report the cardinality
/// violation, NOT the unsupported arm), then the unsupported-arm rejection, and
/// duplicate names LAST because upstream raises that at a post-PGV stage.
///
/// Mutates nothing, so it is idempotent — required, because `validate()` runs
/// twice on a config using dynamic resources (`parse_bootstrap` then
/// `load_dynamic_resources`).
///
/// Error TEXT is not part of the equivalence contract (§7.2); only the VERDICT is.
pub(crate) fn validate_layered_runtime(lr: &LayeredRuntime) -> Result<(), crate::ConfigError> {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (position, layer) in lr.layers.iter().enumerate() {
        if layer.name.is_empty() {
            return Err(crate::ConfigError::EmptyRuntimeLayerName { position });
        }

        // Count the set oneof arms. The three unimplemented arms are DECLARED
        // (PLAN DD-3) precisely so they can be counted here; leaving them to
        // `deny_unknown_fields` would make a two-arm config an opaque serde
        // error instead of a precise cardinality rejection.
        let arms: [(bool, &'static str); 4] = [
            (layer.static_layer.is_some(), "static_layer"),
            (layer.disk_layer.is_some(), "disk_layer"),
            (layer.rtds_layer.is_some(), "rtds_layer"),
            (layer.admin_layer.is_some(), "admin_layer"),
        ];
        let set: Vec<&'static str> = arms
            .iter()
            .filter(|(present, _)| *present)
            .map(|(_, name)| *name)
            .collect();

        match set.len() {
            0 => {
                return Err(crate::ConfigError::RuntimeLayerMissingSpecifier {
                    layer: layer.name.clone(),
                });
            }
            1 => {
                // CF-108-1: exactly one arm — reject it if it is not static_layer.
                if set[0] != "static_layer" {
                    return Err(crate::ConfigError::UnsupportedRuntimeLayerType {
                        layer: layer.name.clone(),
                        arm: set[0],
                    });
                }
            }
            _ => {
                return Err(crate::ConfigError::RuntimeLayerMultipleSpecifiers {
                    layer: layer.name.clone(),
                });
            }
        }

        if !seen.insert(layer.name.as_str()) {
            return Err(crate::ConfigError::DuplicateRuntimeLayerName {
                layer: layer.name.clone(),
            });
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Wire it into `validate()`**

In `crates/envoy-config/src/bootstrap.rs`, inside `pub(crate) fn validate(bootstrap: &mut Bootstrap)`, insert immediately AFTER the closing `}` of the `for cs in [ … ] .into_iter().flatten() { … }` `resource_api_version` loop and immediately BEFORE the comment line `// 18 D1/D3: while CDS is configured-but-unloaded, cluster-reference checks`:

```rust
    // 108.1 D2: `layered_runtime` is a bootstrap-level block with no listener or
    // cluster dependency, so it validates here — before the listener walk and
    // before any cluster-reference deferral. `None` (absent) is legal and is NOT
    // the same as an empty block; see `LayeredRuntime`.
    if let Some(lr) = bootstrap.layered_runtime.as_ref() {
        validate_layered_runtime(lr)?;
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p envoy-config --lib -- bootstrap::tests::validate_layered_runtime bootstrap::tests::parse_bootstrap_runs_the_layered_runtime_validator 2>&1 | tee /tmp/t4s5.log
```

Expected: `test result: ok. 4 passed; 0 failed`.

- [ ] **Step 6: Mutation check — prove the wiring test is not vacuous**

The wiring test would still pass if the duplicate-name check fired somewhere else. Prove the call site is load-bearing.

```bash
# Commit clean FIRST, then mutate a scratch worktree at that commit — never the
# main tree (a parallel workstream's `git checkout` can silently revert an
# in-place mutation).
git worktree add --detach /tmp/108-1-mut HEAD
cd /tmp/108-1-mut
# Comment out the validate() call site.
perl -0pi -e 's/    if let Some\(lr\) = bootstrap\.layered_runtime\.as_ref\(\) \{\n        validate_layered_runtime\(lr\)\?;\n    \}/    \/\/ MUTATED: call site removed/' crates/envoy-config/src/bootstrap.rs
grep -n 'MUTATED: call site removed' crates/envoy-config/src/bootstrap.rs   # must print a line
cargo test -p envoy-config --lib -- bootstrap::tests::parse_bootstrap_runs_the_layered_runtime_validator 2>&1 | tee /tmp/t4mut.log
```

Expected: **`test result: FAILED. 0 passed; 1 failed`** — and the log MUST contain a `test result:` line. An exit 101 with NO `test result:` line is a COMPILE ERROR, not a mutation RED, and proves nothing.

Then the unmutated control from the SAME worktree:

```bash
git -C /tmp/108-1-mut checkout -- crates/envoy-config/src/bootstrap.rs
grep -c 'MUTATED: call site removed' /tmp/108-1-mut/crates/envoy-config/src/bootstrap.rs   # must be 0
cargo test -p envoy-config --lib -- bootstrap::tests::parse_bootstrap_runs_the_layered_runtime_validator 2>&1 | tee /tmp/t4ctl.log
cd /home/esa/git/envoy-rust && git worktree remove /tmp/108-1-mut
git worktree list   # re-verify removal FROM THE REPO ROOT
```

Expected: control `test result: ok. 1 passed`. Record both outcomes in `PROGRESS.md`.

- [ ] **Step 7: Gate (e) and commit**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/t4clippy.log
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 108.1 task 4: validate_layered_runtime (4 reject rules + CF-108-1 arms) wired into validate()"
```

---

### Task 5: The `runtime.rs` snapshot module — arbitrary-depth flattening

**Files:**
- Create: `crates/envoy-config/src/runtime.rs`
- Modify: `crates/envoy-config/src/lib.rs` — add `pub mod runtime;` to the module list at `lib.rs:7-12` (alphabetically, between `pub mod matcher;` and `pub mod rds;`).

**Interfaces:**
- Consumes: `RuntimeValue`, `RuntimeLayer`, `LayeredRuntime` (Task 2).
- Produces:
  - `pub struct RuntimeEntry { pub layer_values: Vec<String>, pub final_value: String }`
  - `pub struct RuntimeSnapshot { pub layer_names: Vec<String>, pub entries: std::collections::BTreeMap<String, RuntimeEntry> }`
  - `pub fn flatten_layer(layer: &RuntimeLayer) -> std::collections::BTreeMap<String, String>`
  - `impl RuntimeSnapshot { pub fn num_layers(&self) -> usize; pub fn num_keys(&self) -> usize }`

  Tasks 6 and 7 depend on all of these names exactly as written. `RuntimeSnapshot::from_bootstrap` is added in Task 7.

> **Why a separate module rather than more of `bootstrap.rs`:** `bootstrap.rs` is already 21 069 lines. The landed precedent for a self-contained engine is `crates/envoy-config/src/matcher.rs` (the 75.1 `HeaderMatcher::matches` engine, +241 net), whose schema type nevertheless lives in `bootstrap.rs`. This slice follows that split exactly: schema in `bootstrap.rs`, engine in `runtime.rs`. `108.2` renders this module; nothing else reads it.

> **`BTreeMap`, not `HashMap`, and the reason is load-bearing for `108.2`.** The sibling fixture's whole design rests on `serde_json::Map` being a `BTreeMap` (V-2, CONFIRMED BY EXPERIMENT at the split: `serde_json`'s `Cargo.lock` dependency block lists `itoa, memchr, serde, serde_core, zmij` — no `indexmap` — and `git grep preserve_order` returns zero hits). Using a `BTreeMap` here keeps the store's own iteration order canonical too, so `108.2`'s renderer is deterministic before `serde_json` ever sees it.

- [ ] **Step 1: Write the failing test — arbitrary-depth flattening**

Create `crates/envoy-config/src/runtime.rs` containing ONLY the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a single `RuntimeLayer` from a YAML fragment.
    fn layer(yaml: &str) -> crate::RuntimeLayer {
        serde_yaml::from_str(yaml).expect("layer fragment must parse")
    }

    #[test]
    fn flatten_layer_recurses_to_arbitrary_depth_and_emits_no_intermediate_keys() {
        // SPEC §2 N-4, MEASURED against envoyproxy/envoy:v1.33.0:
        //   my.nested: {sub_key: v, deeper: {leaf: w}}
        // yields entries `my.nested.sub_key` AND `my.nested.deeper.leaf`, with
        // NO `my.nested` and NO `my.nested.deeper` entry. The parent SPEC
        // measured only ONE level; this recurses.
        let l = layer(
            r#"
name: l
static_layer:
  flat.key: top
  my.nested:
    sub_key: v
    deeper:
      leaf: w
"#,
        );
        let flat = flatten_layer(&l);
        let mut keys: Vec<&str> = flat.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["flat.key", "my.nested.deeper.leaf", "my.nested.sub_key"],
            "no intermediate map key may appear as an entry"
        );
        assert_eq!(flat["flat.key"], "top");
        assert_eq!(flat["my.nested.sub_key"], "v");
        assert_eq!(flat["my.nested.deeper.leaf"], "w");
    }

    #[test]
    fn flatten_layer_stringifies_every_scalar_and_keeps_the_empty_string() {
        // SPEC §2 N-3 / N-7. Table-driven: a new measured cell costs one line.
        let l = layer(
            r#"
name: l
static_layer:
  k.bool.t: true
  k.bool.f: false
  k.int: 42
  k.negint: -7
  k.float: 1.5
  k.str: hello
  k.empty: ""
  k.yaml11: y
"#,
        );
        let flat = flatten_layer(&l);
        for (key, expected) in [
            ("k.bool.t", "true"),
            ("k.bool.f", "false"),
            ("k.int", "42"),
            ("k.negint", "-7"),
            // CF-108-5: the ONLY float cell measured upstream.
            ("k.float", "1.5"),
            ("k.str", "hello"),
            // SPEC §2 N-7: an explicit "" IS an entry and IS counted.
            ("k.empty", ""),
            // CF-108-4: upstream (YAML 1.1) would render "true" here.
            ("k.yaml11", "y"),
        ] {
            assert_eq!(flat[key], expected, "flatten_layer key {key}");
        }
        assert_eq!(flat.len(), 8, "an empty-string value is still an entry");
    }

    #[test]
    fn flatten_layer_handles_absent_and_empty_static_layers() {
        // A layer whose static_layer is an empty map contributes no keys...
        let empty = layer("name: l\nstatic_layer: {}\n");
        assert!(flatten_layer(&empty).is_empty());

        // ...and neither does an EMPTY NESTED map, because it has no leaves.
        // SPEC §2 N-4: intermediate maps never produce entries of their own.
        let empty_nested = layer("name: l\nstatic_layer:\n  a.b: {}\n");
        assert!(
            flatten_layer(&empty_nested).is_empty(),
            "an empty nested map has no leaves and so yields no entry"
        );

        // A layer with NO static_layer arm at all contributes nothing. (The
        // validator rejects such a layer at boot; flatten_layer must still be
        // total, because 108.2 renders snapshots and must never panic.)
        let none = layer("name: l\n");
        assert!(flatten_layer(&none).is_empty());
    }
}
```

- [ ] **Step 2: Declare the module and run the test to verify it fails**

Add to `crates/envoy-config/src/lib.rs` module list:

```rust
pub mod runtime;
```

```bash
cargo test -p envoy-config --lib -- runtime::tests 2>&1 | tee /tmp/t5s2.log
```

Expected: compile failure — `cannot find function `flatten_layer` in this scope`.

- [ ] **Step 3: Write the implementation**

Prepend to `crates/envoy-config/src/runtime.rs`, above the `#[cfg(test)]` module:

```rust
//! 108.1 D3: the runtime snapshot store — the in-memory view of a parsed
//! `layered_runtime` block, shaped exactly as upstream Envoy's admin
//! `GET /runtime` exposes it.
//!
//! This module is the ENGINE; the serde schema it consumes (`LayeredRuntime`,
//! `RuntimeLayer`, `RuntimeValue`) lives in [`crate::bootstrap`]. That split
//! follows the landed `matcher.rs` precedent, where the `HeaderMatcher` schema
//! sits in `bootstrap.rs` and the matching engine sits in its own module.
//!
//! **Nothing reads this store yet.** 108.1 builds the PRODUCER; sibling 108.2
//! adds the admin `GET /runtime` endpoint and the nine `runtime.*` stats that
//! observe it. This slice deliberately wires NEITHER the `RuntimeUInt32`
//! (`status_code_filter`) NOR the `RuntimeFractionalPercent` (CSRF) consumer, so
//! every existing "no runtime subsystem" assertion in the tree stays true.
//!
//! All ordering is `BTreeMap`-canonical. That is not incidental: sibling 108.2's
//! differential fixture rests on `serde_json::Map` being a `BTreeMap` (the
//! workspace enables `preserve_order` nowhere), and a canonically-ordered store
//! keeps the renderer deterministic before `serde_json` is involved.

use crate::{LayeredRuntime, RuntimeLayer, RuntimeValue};
use std::collections::BTreeMap;

/// One runtime key's view across the layer stack.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeEntry {
    /// One slot per CONFIGURED layer, in config order, holding `""` where the
    /// key is absent from that layer (SPEC §2 N-6, MEASURED).
    pub layer_values: Vec<String>,
    /// The last NON-EMPTY slot (SPEC §2 N-7, MEASURED) — **not** the last slot.
    /// An explicitly-set empty string does NOT override a lower layer, and is
    /// indistinguishable on the wire from the key being absent from that layer.
    pub final_value: String,
}

/// The whole snapshot: the ordered layer names plus every flattened key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeSnapshot {
    /// Layer names in config order. For an ABSENT `layered_runtime` block this
    /// is empty; for a PRESENT but empty one it holds exactly one EMPTY STRING
    /// (SPEC §2 N-8, MEASURED) — see `from_bootstrap`.
    pub layer_names: Vec<String>,
    /// Flattened key → entry, canonically ordered.
    pub entries: BTreeMap<String, RuntimeEntry>,
}

impl RuntimeSnapshot {
    /// Backs upstream's `runtime.num_layers` stat: the count of CONFIGURED
    /// layers (SPEC §2 N-5).
    pub fn num_layers(&self) -> usize {
        self.layer_names.len()
    }

    /// Backs upstream's `runtime.num_keys` stat. MEASURED (SPEC §2 N-5): it
    /// counts FLATTENED LEAVES, not declared top-level YAML keys — a layer
    /// declaring 11 top-level keys, one of them a nested map holding two
    /// leaves, yields `num_keys: 12`. This is exactly `entries.len()`.
    pub fn num_keys(&self) -> usize {
        self.entries.len()
    }
}

/// Flatten one layer's `static_layer` into dotted keys → stringified values.
///
/// Recurses to ARBITRARY depth (SPEC §2 N-4): `my.nested: {sub_key: v, deeper:
/// {leaf: w}}` yields `my.nested.sub_key` AND `my.nested.deeper.leaf`, and NO
/// entry for either intermediate map. An empty nested map therefore yields
/// nothing at all — it has no leaves.
///
/// TOTAL by construction: a layer with no `static_layer` arm yields an empty
/// map rather than panicking. The validator rejects such a layer at boot, but
/// 108.2 renders snapshots and must never panic on one.
///
/// **Not in scope, and recorded rather than silently mishandled (CF-108-3):** a
/// nested map containing `numerator` is NOT flattened like every other nested
/// map upstream — it is kept as ONE key whose value is the protobuf TEXT-FORMAT
/// dump of the Struct, complete with literal `\n`s. Matching that byte-for-byte
/// means reimplementing protobuf `DebugString`. This function flattens it like
/// any other map; the divergence is banked, not hidden.
pub fn flatten_layer(layer: &RuntimeLayer) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(map) = layer.static_layer.as_ref() {
        for (key, value) in map {
            flatten_into(key, value, &mut out);
        }
    }
    out
}

/// Recursive worker for [`flatten_layer`]. Mirrors the shape of the landed
/// `validate_json_format_value` recursive walk in `bootstrap.rs`.
fn flatten_into(prefix: &str, value: &RuntimeValue, out: &mut BTreeMap<String, String>) {
    match value {
        RuntimeValue::Map(inner) => {
            for (key, sub) in inner {
                flatten_into(&format!("{prefix}.{key}"), sub, out);
            }
        }
        scalar => {
            out.insert(prefix.to_string(), scalar.stringify());
        }
    }
}
```

> `LayeredRuntime` is imported here because Task 7 uses it. If this task's boundary reports `unused import: LayeredRuntime`, **move that one name into the Task 7 edit rather than adding `#[allow(unused_imports)]`** — and record the adjustment in `PROGRESS.md`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p envoy-config --lib -- runtime::tests 2>&1 | tee /tmp/t5s4.log
```

Expected: `test result: ok. 3 passed; 0 failed`.

- [ ] **Step 5: Mutation check — prove the recursion is load-bearing**

A one-level-only flattener would pass a shallow test. Prove the depth test catches it.

```bash
git worktree add --detach /tmp/108-1-mut5 HEAD
cd /tmp/108-1-mut5
# Make the recursion one level deep: stop descending into nested maps.
perl -0pi -e 's/        RuntimeValue::Map\(inner\) => \{\n            for \(key, sub\) in inner \{\n                flatten_into\(&format!\("\{prefix\}\.\{key\}"\), sub, out\);\n            \}\n        \}/        RuntimeValue::Map(_) => \{ \/* MUTATED: recursion removed *\/ \}/' crates/envoy-config/src/runtime.rs
grep -n 'MUTATED: recursion removed' crates/envoy-config/src/runtime.rs   # must print
cargo test -p envoy-config --lib -- runtime::tests::flatten_layer_recurses 2>&1 | tee /tmp/t5mut.log
```

Expected: **`test result: FAILED. 0 passed; 1 failed`**, with a `test result:` line present. Then run the unmutated control from the same worktree and remove it:

```bash
git -C /tmp/108-1-mut5 checkout -- crates/envoy-config/src/runtime.rs
grep -c 'MUTATED' /tmp/108-1-mut5/crates/envoy-config/src/runtime.rs   # must be 0
cargo test -p envoy-config --lib -- runtime::tests::flatten_layer_recurses 2>&1 | tee /tmp/t5ctl.log
cd /home/esa/git/envoy-rust && git worktree remove /tmp/108-1-mut5 && git worktree list
```

- [ ] **Step 6: Gate (e) and commit**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/t5clippy.log
git add crates/envoy-config/src/runtime.rs crates/envoy-config/src/lib.rs
git commit -m "phase 108.1 task 5: runtime.rs snapshot module + arbitrary-depth flattening (SPEC N-4)"
```

---

### Task 6: Layer slots and last-non-empty precedence

**Files:**
- Modify: `crates/envoy-config/src/runtime.rs`

**Interfaces:**
- Consumes: `flatten_layer`, `RuntimeEntry`, `RuntimeSnapshot` (Task 5).
- Produces: `impl RuntimeSnapshot { pub fn from_layers(layer_names: Vec<String>, layers: &[RuntimeLayer]) -> RuntimeSnapshot }`. Task 7 calls it.

> **The two rules this task implements are the ones most likely to be got wrong, and both are MEASURED.** (1) Every key gets one slot per CONFIGURED layer — `""` where absent — so slot COUNT is a property of the stack, not of the key. (2) `final_value` is the last **NON-EMPTY** slot, not the last slot. A key set to `real_value` in a base layer and to `""` in an override layer keeps `final_value: "real_value"`.

- [ ] **Step 1: Write the failing test — the measured two-layer transcript**

Add to the `mod tests` block in `crates/envoy-config/src/runtime.rs`:

```rust
    #[test]
    fn from_layers_reproduces_the_measured_two_layer_transcript() {
        // SPEC §2 N-6 and N-7, MEASURED against envoyproxy/envoy:v1.33.0. TWO
        // static layers with distinct names are LEGAL, which is what makes
        // multi-layer precedence witnessable in 108.1 without the out-of-scope
        // admin_layer. The upstream response was, verbatim:
        //
        //   "shared.key":       {"layer_values":["from_base","from_override"],"final_value":"from_override"}
        //   "only.in.base":     {"layer_values":["base_val",""],              "final_value":"base_val"}
        //   "only.in.override": {"layer_values":["","over_val"],              "final_value":"over_val"}
        //   "empty.in.override":{"layer_values":["real_value",""],            "final_value":"real_value"}
        //   with "layers":["base_layer","override_layer"], num_layers 2, num_keys 4.
        let base = layer(
            r#"
name: base_layer
static_layer:
  shared.key: from_base
  only.in.base: base_val
  empty.in.override: real_value
"#,
        );
        let over = layer(
            r#"
name: override_layer
static_layer:
  shared.key: from_override
  only.in.override: over_val
  empty.in.override: ""
"#,
        );
        let snap = RuntimeSnapshot::from_layers(
            vec!["base_layer".to_string(), "override_layer".to_string()],
            &[base, over],
        );

        assert_eq!(snap.layer_names, vec!["base_layer", "override_layer"]);
        assert_eq!(snap.num_layers(), 2);
        assert_eq!(snap.num_keys(), 4);

        // Table-driven: (key, expected slots, expected final_value).
        for (key, slots, final_value) in [
            ("shared.key", vec!["from_base", "from_override"], "from_override"),
            ("only.in.base", vec!["base_val", ""], "base_val"),
            ("only.in.override", vec!["", "over_val"], "over_val"),
            // THE rule most likely to be got wrong: an explicitly-set "" does
            // NOT override a lower layer. "last wins" would give "" here.
            ("empty.in.override", vec!["real_value", ""], "real_value"),
        ] {
            let e = snap.entries.get(key).unwrap_or_else(|| panic!("missing {key}"));
            assert_eq!(e.layer_values, slots, "layer_values for {key}");
            assert_eq!(e.final_value, final_value, "final_value for {key}");
        }
    }

    #[test]
    fn from_layers_gives_every_key_one_slot_per_configured_layer() {
        // Slot COUNT is a property of the layer STACK, not of the key: a key
        // present in only one of three layers still gets three slots.
        let a = layer("name: a\nstatic_layer:\n  only.in.a: v\n");
        let b = layer("name: b\nstatic_layer: {}\n");
        let c = layer("name: c\nstatic_layer: {}\n");
        let snap = RuntimeSnapshot::from_layers(
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            &[a, b, c],
        );
        let e = &snap.entries["only.in.a"];
        assert_eq!(e.layer_values, vec!["v", "", ""]);
        assert_eq!(e.final_value, "v");
        assert_eq!(snap.num_layers(), 3);
        assert_eq!(snap.num_keys(), 1);
    }

    #[test]
    fn from_layers_keeps_an_all_empty_key_as_an_entry_with_an_empty_final_value() {
        // SPEC §2 N-7, single-layer probe, MEASURED:
        //   my.empty.string.key: "" -> {"final_value":"","layer_values":[""]}
        // and it IS counted in num_keys.
        let only = layer("name: l\nstatic_layer:\n  my.empty.string.key: \"\"\n");
        let snap = RuntimeSnapshot::from_layers(vec!["l".to_string()], &[only]);
        let e = &snap.entries["my.empty.string.key"];
        assert_eq!(e.layer_values, vec![""]);
        assert_eq!(e.final_value, "");
        assert_eq!(snap.num_keys(), 1, "an all-empty key is still a key");
    }
```

- [ ] **Step 2: Run them to verify they fail**

```bash
cargo test -p envoy-config --lib -- runtime::tests::from_layers 2>&1 | tee /tmp/t6s2.log
```

Expected: compile failure — `no function or associated item named `from_layers` found for struct `RuntimeSnapshot``.

- [ ] **Step 3: Write the implementation**

Add to the `impl RuntimeSnapshot` block in `crates/envoy-config/src/runtime.rs`:

```rust
    /// Build a snapshot from an ordered layer stack.
    ///
    /// `layer_names` is passed separately from `layers` because an ABSENT
    /// `layered_runtime` block and a PRESENT-but-empty one differ in their layer
    /// NAMES but not in their layer CONTENT (SPEC §2 N-8): upstream synthesizes
    /// one layer named the EMPTY STRING for the empty block, and that synthetic
    /// layer has no `RuntimeLayer` behind it. `from_bootstrap` owns that
    /// distinction; this function is a total function over whatever stack it is
    /// handed. **Invariant: `layer_names.len()` MUST equal `layers.len()` unless
    /// `layers` is empty**, in which case each key simply gets `layer_names.len()`
    /// empty slots — which is exactly the empty-block case.
    ///
    /// Two MEASURED rules, both easy to get wrong:
    /// - every key gets ONE SLOT PER CONFIGURED LAYER, `""` where absent, in
    ///   config order (N-6) — slot count is a property of the stack, not the key;
    /// - `final_value` is the last NON-EMPTY slot (N-7), NOT the last slot. An
    ///   explicitly-set `""` does not override a lower layer, and is
    ///   indistinguishable on the wire from absence.
    pub fn from_layers(layer_names: Vec<String>, layers: &[RuntimeLayer]) -> RuntimeSnapshot {
        let slot_count = layer_names.len();
        let flattened: Vec<BTreeMap<String, String>> = layers.iter().map(flatten_layer).collect();

        let mut entries: BTreeMap<String, RuntimeEntry> = BTreeMap::new();
        for per_layer in &flattened {
            for key in per_layer.keys() {
                entries.entry(key.clone()).or_insert_with(|| RuntimeEntry {
                    layer_values: vec![String::new(); slot_count],
                    final_value: String::new(),
                });
            }
        }

        for (index, per_layer) in flattened.iter().enumerate() {
            for (key, value) in per_layer {
                if let Some(entry) = entries.get_mut(key)
                    && let Some(slot) = entry.layer_values.get_mut(index)
                {
                    *slot = value.clone();
                }
            }
        }

        for entry in entries.values_mut() {
            // Last NON-EMPTY wins; an all-empty key keeps the empty string.
            entry.final_value = entry
                .layer_values
                .iter()
                .rev()
                .find(|v| !v.is_empty())
                .cloned()
                .unwrap_or_default();
        }

        RuntimeSnapshot {
            layer_names,
            entries,
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p envoy-config --lib -- runtime::tests 2>&1 | tee /tmp/t6s4.log
```

Expected: `test result: ok. 6 passed; 0 failed` (3 from Task 5 + 3 here).

- [ ] **Step 5: Mutation check — prove "last non-empty" is not "last"**

This is the single most important mutation in the phase: "last wins" is the natural wrong implementation and passes every test that does not include the `empty.in.override` cell.

```bash
git worktree add --detach /tmp/108-1-mut6 HEAD
cd /tmp/108-1-mut6
perl -0pi -e 's/                \.rev\(\)\n                \.find\(\|v\| !v\.is_empty\(\)\)\n                \.cloned\(\)\n                \.unwrap_or_default\(\);/                .next_back()          \/\/ MUTATED: last wins, not last NON-EMPTY\n                .cloned()\n                .unwrap_or_default();/' crates/envoy-config/src/runtime.rs
grep -n 'MUTATED: last wins' crates/envoy-config/src/runtime.rs   # must print
cargo test -p envoy-config --lib -- runtime::tests::from_layers_reproduces 2>&1 | tee /tmp/t6mut.log
```

Expected: **`test result: FAILED. 0 passed; 1 failed`** with the failure naming `final_value for empty.in.override` and `left: ""`, `right: "real_value"`. **A `test result:` line MUST be present** — if the perl edit does not compile, fix the mutation and re-run; a compile error is not a RED.

Control and cleanup:

```bash
git -C /tmp/108-1-mut6 checkout -- crates/envoy-config/src/runtime.rs
grep -c 'MUTATED' /tmp/108-1-mut6/crates/envoy-config/src/runtime.rs   # must be 0
cargo test -p envoy-config --lib -- runtime::tests::from_layers_reproduces 2>&1 | tee /tmp/t6ctl.log
cd /home/esa/git/envoy-rust && git worktree remove /tmp/108-1-mut6 && git worktree list
```

- [ ] **Step 6: Gate (e) and commit**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/t6clippy.log
git add crates/envoy-config/src/runtime.rs
git commit -m "phase 108.1 task 6: layer slots + last-NON-EMPTY precedence (SPEC N-6/N-7)"
```

---

### Task 7: `RuntimeSnapshot::from_bootstrap` — the absent-vs-empty distinction

**Files:**
- Modify: `crates/envoy-config/src/runtime.rs`

**Interfaces:**
- Consumes: `RuntimeSnapshot::from_layers` (Task 6); `Bootstrap.layered_runtime` (Task 2).
- Produces: `impl RuntimeSnapshot { pub fn from_bootstrap(bootstrap: &crate::Bootstrap) -> RuntimeSnapshot }`. **This is the entry point sibling `108.2` calls.** Its name and signature must not change without updating `108.2/SPEC.md`.

- [ ] **Step 1: Write the failing test — the three measured states**

```rust
    #[test]
    fn from_bootstrap_distinguishes_absent_from_empty_from_populated() {
        // SPEC §2 N-8, MEASURED against envoyproxy/envoy:v1.33.0:
        //   | config                            | /runtime                       | num_layers | num_keys |
        //   | no layered_runtime block          | {"entries":{},"layers":[]}     | 0          | 0        |
        //   | layered_runtime: {}               | {"entries":{},"layers":[""]}   | 1          | 0        |
        //   | layered_runtime: { layers: [] }   | {"entries":{},"layers":[""]}   | 1          | 0        |
        // Upstream synthesizes ONE layer named the EMPTY STRING for both empty
        // spellings. Collapsing None and Some(empty) MINTS a divergence.
        let base = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let absent = crate::parse_bootstrap(base).expect("valid");
        let snap = RuntimeSnapshot::from_bootstrap(&absent);
        assert!(snap.layer_names.is_empty(), "absent block -> layers: []");
        assert_eq!(snap.num_layers(), 0);
        assert_eq!(snap.num_keys(), 0);

        for spelling in ["layered_runtime: {}\n", "layered_runtime:\n  layers: []\n"] {
            let b = crate::parse_bootstrap(&format!("{base}{spelling}")).expect("valid");
            let snap = RuntimeSnapshot::from_bootstrap(&b);
            assert_eq!(
                snap.layer_names,
                vec![String::new()],
                "an empty block synthesizes ONE layer named the empty string ({spelling:?})"
            );
            assert_eq!(snap.num_layers(), 1);
            assert_eq!(snap.num_keys(), 0);
        }

        // Populated: names come from config, in config order.
        let b = crate::parse_bootstrap(&format!(
            "{base}layered_runtime:\n  layers:\n  - name: base_layer\n    static_layer:\n      a.b: 1\n      n:\n        deep: x\n  - name: override_layer\n    static_layer:\n      a.b: 2\n"
        ))
        .expect("valid");
        let snap = RuntimeSnapshot::from_bootstrap(&b);
        assert_eq!(snap.layer_names, vec!["base_layer", "override_layer"]);
        assert_eq!(snap.num_layers(), 2);
        // SPEC §2 N-5: num_keys counts FLATTENED LEAVES — `a.b` plus `n.deep`.
        assert_eq!(snap.num_keys(), 2);
        assert_eq!(snap.entries["a.b"].layer_values, vec!["1", "2"]);
        assert_eq!(snap.entries["a.b"].final_value, "2");
        assert_eq!(snap.entries["n.deep"].layer_values, vec!["x", ""]);
        assert_eq!(snap.entries["n.deep"].final_value, "x");
    }

    #[test]
    fn from_bootstrap_counts_flattened_leaves_not_declared_keys() {
        // SPEC §2 N-5, MEASURED: a layer declaring 11 top-level YAML keys, one
        // of them a nested map holding TWO leaves, yields num_keys: 12 — and
        // that equals the `entries` object size exactly.
        let mut yaml = String::from(
            "admin:\n  address:\n    socket_address:\n      address: 127.0.0.1\n      port_value: 9901\nlayered_runtime:\n  layers:\n  - name: l\n    static_layer:\n",
        );
        for i in 0..10 {
            yaml.push_str(&format!("      k{i}: v{i}\n"));
        }
        yaml.push_str("      nested:\n        one: a\n        two: b\n");
        let b = crate::parse_bootstrap(&yaml).expect("valid");
        let snap = RuntimeSnapshot::from_bootstrap(&b);
        assert_eq!(snap.num_keys(), 12, "10 flat + 2 nested leaves");
        assert_eq!(snap.entries.len(), snap.num_keys());
        assert!(!snap.entries.contains_key("nested"), "no intermediate entry");
        assert_eq!(snap.entries["nested.one"].final_value, "a");
    }
```

- [ ] **Step 2: Run them to verify they fail**

```bash
cargo test -p envoy-config --lib -- runtime::tests::from_bootstrap 2>&1 | tee /tmp/t7s2.log
```

Expected: compile failure — `no function or associated item named `from_bootstrap``.

- [ ] **Step 3: Write the implementation**

Add to the `impl RuntimeSnapshot` block in `crates/envoy-config/src/runtime.rs`:

```rust
    /// Build the snapshot a parsed `Bootstrap` implies. **This is the entry
    /// point sibling 108.2's admin `GET /runtime` renderer calls.**
    ///
    /// The absent-vs-empty distinction lives here and nowhere else (SPEC §2 N-8,
    /// MEASURED): no `layered_runtime:` block yields ZERO layers, while
    /// `layered_runtime: {}` or `layered_runtime: {layers: []}` yields ONE layer
    /// named the EMPTY STRING. Upstream synthesizes that layer internally, which
    /// is why it is created here rather than in the schema — a config-declared
    /// layer named `""` is boot-fatal (PGV `min_len 1`), so the synthetic layer
    /// deliberately bypasses `validate_layered_runtime`.
    pub fn from_bootstrap(bootstrap: &crate::Bootstrap) -> RuntimeSnapshot {
        let Some(lr): Option<&LayeredRuntime> = bootstrap.layered_runtime.as_ref() else {
            // Absent: zero layers, zero keys.
            return RuntimeSnapshot::default();
        };
        if lr.layers.is_empty() {
            // Present but empty, in EITHER spelling: one synthetic layer named
            // the empty string, and no keys.
            return RuntimeSnapshot::from_layers(vec![String::new()], &[]);
        }
        let names: Vec<String> = lr.layers.iter().map(|l| l.name.clone()).collect();
        RuntimeSnapshot::from_layers(names, &lr.layers)
    }
```

- [ ] **Step 4: Run the whole module and verify**

```bash
cargo test -p envoy-config --lib -- runtime::tests 2>&1 | tee /tmp/t7s4.log
```

Expected: `test result: ok. 8 passed; 0 failed`.

- [ ] **Step 5: Mutation check — prove absent and empty are not collapsed**

```bash
git worktree add --detach /tmp/108-1-mut7 HEAD
cd /tmp/108-1-mut7
# Collapse the empty-block case into the absent case — the natural wrong model.
perl -0pi -e 's/            return RuntimeSnapshot::from_layers\(vec!\[String::new\(\)\], &\[\]\);/            return RuntimeSnapshot::default(); \/\/ MUTATED: collapse empty into absent/' crates/envoy-config/src/runtime.rs
grep -n 'MUTATED: collapse empty into absent' crates/envoy-config/src/runtime.rs   # must print
cargo test -p envoy-config --lib -- runtime::tests::from_bootstrap_distinguishes 2>&1 | tee /tmp/t7mut.log
git -C /tmp/108-1-mut7 checkout -- crates/envoy-config/src/runtime.rs
cargo test -p envoy-config --lib -- runtime::tests::from_bootstrap_distinguishes 2>&1 | tee /tmp/t7ctl.log
cd /home/esa/git/envoy-rust && git worktree remove /tmp/108-1-mut7 && git worktree list
```

Expected: mutated `test result: FAILED. 0 passed; 1 failed`; control `ok. 1 passed`.

- [ ] **Step 6: Gate (e) and commit**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/t7clippy.log
git add crates/envoy-config/src/runtime.rs
git commit -m "phase 108.1 task 7: RuntimeSnapshot::from_bootstrap; absent vs empty per SPEC N-8"
```

---

### Task 8: Fuzz corpus seed and its `!`-un-ignore line

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore`

**Interfaces:** none — this task adds no Rust.

> **This is the task most likely to silently no-op.** MEASURED at this PLAN-write with `git check-ignore -v`:
>
> ```
> $ git check-ignore -v crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml
> crates/envoy-config/fuzz/.gitignore:1:corpus/parse_bootstrap/*   crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml
> EXIT=0
> ```
>
> Line 1 of that `.gitignore` blanket-ignores the whole corpus directory. **Without the `!` line the seed is invisible to `git add`, to `git status` and to CI**, and nothing fails — CI checks out only tracked files (`.github/workflows/ci.yml:82`), and the fuzz step names only the TARGET (`ci.yml:107`), never a seed filename.
>
> **NO `ci.yml` change is required or permitted** — a corpus seed is discovered from the filesystem. The `ci.yml` fuzz job spans lines 77–134 and enumerates targets and fuzz subcrates only.

> **Measured invariants:** the file is currently **67** lines with **64** `!` lines, and there are exactly **64** tracked seeds — a perfect 1:1 correspondence. After this task: **68** lines, **65** `!` lines, **65** tracked seeds. The `!` lines are appended **chronologically, not sorted**, and the trailing `artifacts/` / `target/` pair must stay last.

- [ ] **Step 1: Record the pre-edit census**

```bash
wc -l < crates/envoy-config/fuzz/.gitignore
grep -c '^!' crates/envoy-config/fuzz/.gitignore
git ls-files 'crates/envoy-config/fuzz/corpus/parse_bootstrap/*' | wc -l
```

Expected: `67`, `64`, `64`. If any differs, re-derive before proceeding.

- [ ] **Step 2: Write the seed**

Create `crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml`. Seeds are FULL, self-contained, valid bootstrap configs (never fragments) — this one exercises every new parse path in one document: two layers, every scalar arm, two-level nesting, and an empty-string value.

```yaml
node: { id: fuzz-108.1, cluster: fuzz-108.1 }
static_resources:
  listeners:
    - name: l1
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: v
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 503, body: { inline_string: "fuzz\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
# Phase 108.1: the `layered_runtime` static_layer surface. TWO layers with
# distinct names are legal (MEASURED), which exercises the slot/precedence path;
# `empty.in.override` pins that an explicit "" does NOT override a lower layer.
# Every scalar arm of the recursive value type appears, plus two-level nesting.
layered_runtime:
  layers:
    - name: base_layer
      static_layer:
        shared.key: from_base
        only.in.base: base_val
        empty.in.override: real_value
        k.bool: true
        k.int: 42
        k.negint: -7
        k.float: 1.5
        k.empty: ""
        my.nested:
          sub_key: v
          deeper:
            leaf: w
    - name: override_layer
      static_layer:
        shared.key: from_override
        only.in.override: over_val
        empty.in.override: ""
```

- [ ] **Step 3: Add the un-ignore line**

In `crates/envoy-config/fuzz/.gitignore`, insert as the new line 66 — immediately AFTER `!corpus/parse_bootstrap/route_redirect_action.yaml` and immediately BEFORE `artifacts/`:

```
!corpus/parse_bootstrap/layered_runtime.yaml
```

- [ ] **Step 4: PROVE the seed is tracked — this step is the whole point of the task**

```bash
git check-ignore -v crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml; echo "check-ignore exit=$?"
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml crates/envoy-config/fuzz/.gitignore
git ls-files 'crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml'
git ls-files 'crates/envoy-config/fuzz/corpus/parse_bootstrap/*' | wc -l
wc -l < crates/envoy-config/fuzz/.gitignore
grep -c '^!' crates/envoy-config/fuzz/.gitignore
```

Expected, ALL of them: `check-ignore` prints nothing and exits **1** (not ignored); `git ls-files` prints the seed path (**a non-empty result is the proof — an empty result means the task silently failed**); the seed count is **65**; the `.gitignore` is **68** lines with **65** `!` lines.

- [ ] **Step 5: Prove the seed actually parses**

A seed that fails to parse still "works" as a fuzz input but tests nothing interesting. Assert it is a config the tree accepts:

```bash
cargo build -p envoy-bin 2>&1 | tee /tmp/t8build.log
grep -c 'Compiling envoy-bin' /tmp/t8build.log   # must be >= 1, else the build was cached
sed 's/port_value: 10000/port_value: 0/' crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml > /tmp/seed-probe.yaml
timeout 5 ./target/debug/envoy-bin -c /tmp/seed-probe.yaml; echo "exit=$?"
```

Expected: **exit 124** (the `timeout` killed a still-running process), with **no `ConfigError` text on stdout**. That proves the config parses AND the listener binds — strictly stronger than a schema check. **`envoy-bin` has NO `--mode` flag** (it accepts exactly `-c <path>` / `--config-path <path>`), and it writes `ConfigError` to **stdout**, not stderr. An exit of **2** with `unknown argument` means a flag was invented; an immediate non-124 exit with `ConfigError` text means the seed is invalid.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml crates/envoy-config/fuzz/.gitignore
git commit -m "phase 108.1 task 8: parse_bootstrap corpus seed for layered_runtime + its !-un-ignore line"
git show --stat HEAD   # MUST list BOTH files; a one-file commit means the seed was swallowed
```

---

### Task 9: ADR-0173, PROGRESS.md, and the full-suite regression sweep

**Files:**
- Modify: `docs/envoy-rust/DECISIONS.md` — append **ADR-0173**.
- Create: `docs/envoy-rust/phases/108.1-runtime-config-and-snapshot/PROGRESS.md`

**Interfaces:** none — documentation and verification only. **No code changes in this task.**

> **Re-derive the ADR head before writing.** MEASURED at this PLAN-write: head **ADR-0172**, next free **ADR-0173**. Re-derive with `grep -o '^## ADR-[0-9]\{4\}' docs/envoy-rust/DECISIONS.md | sort -t- -k2 -n | tail -1`. **NEVER derive the next free number from a count** — `grep -c '^## ADR-'` returns **169** because it also counts the template near line 10, and the numbers are NOT contiguous (`0082`, `0116`, `0117`, `0119` are missing). `DECISIONS.md` is **4116** lines but ~310 000 tokens (single-line ADR blocks) and a whole-file Read is REFUSED — chunk it with `grep -n '^## ADR-'` plus offset/limit. It is NOT chronological: ascending `ADR-0001..0100`, then a NEWEST-FIRST block, so **ADR-0173 is inserted at the HEAD of the newest-first block**, not at EOF.

- [ ] **Step 1: Write ADR-0173**

Append at the head of the newest-first block. It must record the CF-108-4 disposition with the measurement that forced it, plus the three subsidiary decisions this plan settled:

- **Decision 1 — CF-108-4:** envoy-rust follows YAML 1.2; the upstream YAML-1.1 booleanization of unquoted `y`/`n`/`on`/`off` is a RECORDED divergence, not normalised. Rationale: MEASURED against `serde_yaml 0.9.34`, unquoted `y` and quoted `"y"` both deserialize to `String("y")` and are indistinguishable at the `serde_yaml::Value` level, so no parse-time transform can booleanize the first without also booleanizing the second — and upstream renders quoted `"y"` as `"y"`. Options considered: (a) normalise at parse time — REJECTED as not implementable without replacing the YAML scanner; (b) record the divergence — CHOSEN; (c) reject `y`/`n`/`on`/`off` values outright — REJECTED as inventing a third behaviour that matches neither side.
- **Decision 2 — CF-108-5 [NEW]:** float rendering beyond the single MEASURED cell `1.5 → "1.5"` is UNMEASURED and must be re-measured before any float enters a differential fixture. Names the Rust renderings `1.0 → "1"`, `1e6 → "1000000"`, `-0.0 → "-0"` as plausible-but-unconfirmed, and cites `CF-39-1` as the same deferred rabbit hole.
- **Decision 3:** `JsonFormatValue` (`bootstrap.rs:936-960`) is NOT reused, correcting `108.1/SPEC.md` §1 D1's claim that no recursive YAML value type exists. Rationale: it cannot represent numbers (ADR-0094 §D / CF-39-1) and its `Format(String)` arm compiles string leaves as access-log command operators.
- **Decision 4:** `disk_layer` / `rtds_layer` / `admin_layer` are DECLARED as `Option<serde_yaml::Value>` and rejected by a precise `ConfigError`, following the landed `HashPolicy` recognize-then-reject pattern, rather than left to `deny_unknown_fields`. This is what makes the oneof cardinality count correct. **CF-108-1.**

- [ ] **Step 2: Write PROGRESS.md**

Append one section per task, quoting the ACTUAL command outputs (not summaries): the RED output, the GREEN output, the mutation RED and its unmutated control, the `Checking`/`Compiling` counts, and the Task 8 `git ls-files` proof. Record any task boundary where clippy could not be kept clean, and why.

- [ ] **Step 3: Full-workspace regression sweep — gate (b)**

This slice adds NO fixture, so gate (a) is vacuously satisfied and gate (b) is the real witness. Run the sweep **twice** and diff the failing SET — a single sweep cannot satisfy ADR-0164 leg (iii).

```bash
cargo build --workspace --all-targets 2>&1 | tee /tmp/t9build.log
cargo test --workspace --no-fail-fast > /tmp/t9sweep1.log 2>&1; echo "sweep1 exit=$?"
cargo test --workspace --no-fail-fast > /tmp/t9sweep2.log 2>&1; echo "sweep2 exit=$?"
for f in /tmp/t9sweep1.log /tmp/t9sweep2.log; do
  echo "== $f"
  grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' "$f" \
    | awk '{b++; p+=$4; fl+=$6} END{printf "binaries=%d passed=%d failed=%d\n", b, p, fl}'
  grep -oE '^---- [^ ]+ stdout ----' "$f" | sort -u
done
diff <(grep -oE '^---- [^ ]+ stdout ----' /tmp/t9sweep1.log | sort -u) \
     <(grep -oE '^---- [^ ]+ stdout ----' /tmp/t9sweep2.log | sort -u)
```

> **NEVER pipe these through `tail`** — it truncates the `failures:` block and hides `Compiling`. **Do NOT census the `failures:` block by indentation** — it invents phantom test names from the failure BODY; use the `---- <name> stdout ----` markers only.

**Adjudicating any RED:** apply ADR-0164's four-part test — assertion never reached, passes in isolation, absent from at least one sweep, untouched by this phase's surface — **never** membership in a remembered list. The stable local core of five fails DETERMINISTICALLY in isolation and that determinism IS the environmental signature: the four `access_log_*_upstream_reset` binaries (`TcpCloseBackend`, IPv6-unreachable) and `admin_config_dump_server_info` (the `192.168.65.2` bridge-IP family). The tail is open-ended and its size carries no signal.

**Phase-specific notes.** `admin_config_dump_server_info` (fixture `0014`) is a core member AND asserts `/config_dump` — the one place this slice could plausibly regress. Its known failure is in `/clusters` backend-endpoint ADDRESSES, not in the bootstrap dump. If it REDs, read the failure TEXT: an address-family failure is the known flake; a failure naming `layered_runtime` is a REAL regression and means Task 2's `skip_serializing_if` is wrong. **`108.1` adds no fixture, so no differential fixture can newly fail by construction.**

- [ ] **Step 4: Arithmetic identity — the strongest flake-vs-regression discriminator**

```bash
# Base is THIS sub-phase's base commit, not a previous phase's close-out.
BASE=fb143376e58aa8726cc248a8cc86e817c9b16ed2
git diff "$BASE"..HEAD -- 'crates/*.rs' | grep -cE '^\+\s*#\[(tokio::)?test\]'
git diff --numstat "$BASE"..HEAD -- . ':(exclude)docs/' | awk '{a+=$1;d+=$2} END{printf "net code LoC = %d\n", a-d}'
```

The last CI baseline on this plan's base commit is **`binaries=163 passed=2152 failed=0`** (run `31065720371` on `fb143376e58aa8726cc248a8cc86e817c9b16ed2`). **Re-confirm that yourself rather than inherit it.** Then: `local passed + local failed` must equal `2152 + (the number of tests this phase adds)`, and the binary count must stay at **163** — this slice adds no new test binary, only tests inside `envoy-config`'s existing lib target. A mismatch is a real signal.

Counting this plan's tests: Task 1 adds **4**, Task 4 adds **4**, Task 5 adds **3**, Task 6 adds **3**, Task 7 adds **2**, Task 2 adds **2** → **18** new tests. **Predicted CI total: `binaries=163 passed=2170 failed=0`.** This is a PREDICTION, not a measurement; state 4 must measure it.

- [ ] **Step 5: Remaining gate-(e) commands and gate (d)**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/t9clippy.log
grep -c '^ *Checking' /tmp/t9clippy.log     # must be >= 1
cargo fmt --all -- --check
cargo deny check 2>&1 | tee /tmp/t9deny.log
```

`cargo deny check` can RED on a fresh unrelated RustSec advisory — that is a patch-bump of the dependency, **not** a phase regression. Its five `license-not-encountered` warnings are unmatched ALLOWANCES in `deny.toml`, not violations.

**Gate (d):** record EXPLICITLY that this slice adds **no new fuzz target**, that gate (d) is satisfied by the pre-existing `parse_bootstrap` target, that `ci.yml` therefore needs no change (cited to `ci.yml:107`), and that the new seed is proven tracked by Task 8 Step 4's `git ls-files`. **Do not skip gate (d) silently** — the SPEC §6(d) requires it recorded.

- [ ] **Step 6: Commit**

```bash
git add docs/envoy-rust/DECISIONS.md docs/envoy-rust/phases/108.1-runtime-config-and-snapshot/PROGRESS.md
git commit -m "phase 108.1 task 9: ADR-0173 (CF-108-4 disposition, CF-108-5 opened); PROGRESS.md; state-3 regression sweep [ADR-0173]"
```

> **STOP HERE.** Task 9 completes §5 **state 3**. The state-4 verification gate is a **SEPARATE SESSION** (§5.1; ADR-0127 — the context that wrote the implementation must not grade it). Update `STATE.md` to point at state 4 and exit.

---

## §6.1 gate — the size re-derivation, bottom-up

The SPEC projected **≈1098** net LoC. This plan re-derives it from its own task decomposition rather than inheriting that number, per the handoff's instruction that the gate can fire AGAIN at the sub-phase's own PLAN-write.

**Calibration anchors, RE-MEASURED ON DISK at this PLAN-write** (`git diff --numstat cf5cf85 9556b2c -- . ':(exclude)docs/'`), not inherited:

| Landed unit | non-test + test, measured |
|---|---|
| `76.1` total net | **774** (`+793 −19`) |
| — `crates/envoy-config/src/bootstrap.rs` | **+655 / −10** |
| — `crates/envoy-config/src/lib.rs` | **+28 / −8** |
| — corpus seed + `.gitignore` | **+37 / +1 = 38** |
| — `crates/envoy-http1/src/hcm.rs` | **+72 / −1** |
| `76.2` full, incl. its §5.2 re-entry | **1568** (`+1643 −75`) |

Every figure reproduces exactly as the SPEC and the standing ledger record them.

**This plan's bottom-up estimate:**

| Task | non-test | test | total |
|---|---:|---:|---:|
| 1 — `RuntimeValue` + `stringify` | ~50 | ~130 | ~180 |
| 2 — schema types + `Bootstrap` field + re-exports | ~60 | ~110 | ~170 |
| 3 — five `ConfigError` variants | ~45 | — | ~45 |
| 4 — `validate_layered_runtime` + wiring | ~79 | ~180 | ~259 |
| 5 — `runtime.rs` + flattening | ~110 | ~130 | ~240 |
| 6 — slots + last-non-empty precedence | ~45 | ~110 | ~155 |
| 7 — `from_bootstrap` | ~35 | ~90 | ~125 |
| 8 — corpus seed + `.gitignore` | ~41 | — | ~41 |
| 9 — docs only (`docs/` is excluded from net code LoC) | — | — | 0 |
| **Total** | **~465** | **~750** | **≈1215** |

**VERDICT: the §6.1 gate does NOT fire. No split.** 9 tasks against the ~25-task threshold, and ≈1215 against the ~1500 LoC threshold — about **19% headroom**. The estimate lands **~11% above** the SPEC's ≈1098, entirely in the test half, which is the direction every calibration phase has drifted (`76.1` overran its own projection by **+50%**, all of it in the test half).

**The mitigation is the design, not a promise.** The three largest test groups (Task 1's stringification cells, Task 5's flattening cells, Task 6's precedence cells) are **TABLE-DRIVEN**, the design that measurably beat house-style per-case `#[test]` fns at `76.2` (255 LoC vs ~400 for 22 cells). A new measured cell costs ONE line. Do not expand these tables into separate `#[test]` functions.

**§6.1's mid-execution trigger stays ARMED.** If any single task's sub-steps blow past ~10 items, or running net LoC (`git diff --numstat <base> HEAD -- . ':(exclude)docs/'`) crosses **~1500** before Task 9, **STOP and split** with a new ADR rather than absorbing it. Budget the §5.2 re-entry honestly: `76.2` grew **+24%** from its review's fixes alone (1265 → 1568). At +24% this slice lands near **1507** — i.e. *a review-driven re-entry could itself push the total over the gate*, which is a mid-execution split trigger at that point, not a reason to split now.

---

## Self-review against the SPEC

**1. Spec coverage.** Every SPEC §1 deliverable maps to a task:

| SPEC deliverable | Task(s) |
|---|---|
| D1 — `Bootstrap.layered_runtime: Option<LayeredRuntime>` | 2 |
| D1 — `LayeredRuntime { layers }`, `RuntimeLayer` + `deny_unknown_fields` + four oneof arms | 2 |
| D1 — recursive scalar-or-map value type | 1 |
| D1 — `Serialize` arms / `/config_dump` cascade whole | 2 (Step 7 pins it) |
| D2 — empty/absent `name`; duplicate `name`; no arm; >1 arm | 4 |
| D2 — fail-loud rejection of `disk_layer`/`rtds_layer`/`admin_layer` (CF-108-1) | 3, 4 |
| D3 — flatten to dotted keys at arbitrary depth | 5 |
| D3 — stringify every scalar | 1, 5 |
| D3 — one `layer_values` slot per layer, `""` where absent | 6 |
| D3 — `final_value` = last NON-EMPTY slot | 6 |
| D3 — module inside `envoy-config`, no new crate (V-3) | 5 |
| D7 — absent-vs-empty backstop (N-8) | 7 |
| D7 — two-static-layer precedence witness (N-6) | 4, 6 |
| D7 — YAML-1.1 divergence made explicit (N-2 / CF-108-4) | 1 (DD-1) |
| D8 — corpus seed + `!`-un-ignore, proven by `git ls-files` | 8 |
| §6 (d) — record no-new-fuzz-target / no `ci.yml` edit explicitly | 9 |
| §6 (a)/(b) — no fixture; 86 pre-existing fixtures green | 9 |

**Non-goals honoured.** No admin endpoint, no `runtime.*` stats, no fixture `0087`, no `BEHAVIOR_CONTRACT.md` edit (all four are `108.2`); no `disk_layer`/`rtds_layer`/`admin_layer` implementation; no `runtime_key` consumer wiring; no route `runtime_fraction`; no `POST /runtime_modify`; no FractionalPercent text-format rendering (CF-108-3, recorded in Task 5's doc comment); no hot restart. **None of the eleven N-9 "no runtime subsystem" assertions is edited**, and `runtime_key_is_rtds_inert` keeps its name and wording.

**2. Placeholder scan.** No "TBD", no "implement later", no "add appropriate error handling", no "similar to Task N". Every code step carries literal code; every test step carries the actual test body; every verification step carries the actual command and its expected output.

**3. Type consistency.** `RuntimeValue` / `RuntimeLayer` / `LayeredRuntime` / `RuntimeEntry` / `RuntimeSnapshot`, and the methods `stringify` / `flatten_layer` / `from_layers` / `from_bootstrap` / `num_layers` / `num_keys`, are spelled identically in every task that references them and in the Interfaces blocks. The five `ConfigError` variants declared in Task 3 are constructed with exactly those names and field sets in Task 4. `from_layers(Vec<String>, &[RuntimeLayer])` is declared in Task 6 and called with that signature in Task 7.

**4. Corrections this plan makes to the SPEC**, each measured on disk and carried into the code as a doc comment so a later reader does not re-derive them:

- **SPEC §1 D1 is wrong** that no recursive YAML value type exists — `JsonFormatValue` (`bootstrap.rs:936-960`) is one. It still must not be reused (DD-2).
- **SPEC §2's one open question is now decided and the decision was forced by measurement, not preference** — CF-108-4 cannot be normalised at all (DD-1).
- **A new carry-forward, CF-108-5**, is opened for float rendering beyond the single measured `1.5` cell (DD-5).
- **Two value-model behaviours the SPEC did not measure** are now pinned: untagged arm ORDER is load-bearing and silently wrong-but-passing if reversed (DD-4), and `null` / sequences / absent values / out-of-`i64` integers match no arm and boot-reject (DD-6).

---

## Handoff

**This plan is the state-2 deliverable and nothing in it may be executed by the session that wrote it** (§5.1; ADR-0127 — 0→1, 1→2, 2→3, 3→4 and 4→5 are never chained, because the context that wrote an artifact would then be grading it). The next session runs §5 **state 3**, the implementation, via `superpowers:subagent-driven-development` or `superpowers:executing-plans`, and appends to `PROGRESS.md` per task.

**Before Task 1, that session must:** re-run `git status --porcelain` (a parallel workstream is active — `.claude/worktrees/agent-*` worktrees plus long-running containers on `envoyproxy/envoy:v1.33.0` and `envoyproxy/envoy:contrib-v1.37.2`; exclude those worktrees from any repo-wide census and prefer `git ls-files`); re-run `git fetch origin --prune`; and re-derive the `bootstrap.rs` / `lib.rs` line anchors **BY TEXT**, because this plan's anchors were measured at `fb143376e58aa8726cc248a8cc86e817c9b16ed2` and `bootstrap.rs` is 21 069 lines that drift constantly.
