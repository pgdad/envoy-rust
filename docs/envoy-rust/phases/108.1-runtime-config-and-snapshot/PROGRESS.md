# Sub-phase 108.1 — `layered_runtime` config surface + runtime snapshot store — PROGRESS

> **§5 state-3 implementation.** Executed `108.1/PLAN.md`'s nine tasks TDD-first
> (D-3.1), one commit per task. Base commit
> `fb143376e58aa8726cc248a8cc86e817c9b16ed2`; this session's work begins at
> `fe1226a`. Every figure below is a MEASURED command output, not a summary.
>
> **State 4 is a SEPARATE SESSION** (§5.1; ADR-0127 — the context that ran the
> gate must not grade it). Nothing here is a gate adjudication.

## Contents

- §0 Anchor re-derivation (before Task 1)
- §1–§9 One section per task
- §10 Task 9: regression sweep, size gate, gate (d)
- §11 Deviations from `PLAN.md`, each with its reason
- §12 What this session did NOT do
- §13 CI on the state-3 head (a MEASUREMENT; state 4 owns the adjudication)

---

## §0 — Anchor re-derivation BY TEXT, before Task 1

`PLAN.md`'s anchors were measured at `fb14337`. Every one was re-derived by text
on disk at session start. **Two drifted; the rest reproduced exactly.**

| `PLAN.md` anchor | Measured | Verdict |
|---|---|---|
| `bootstrap.rs` is 21 069 lines | `21069` | EXACT |
| `JsonFormatValue` doc block `936-946` | `936-946` | EXACT |
| its `#[derive]` at `:947` | `947` | EXACT |
| **`pub enum JsonFormatValue` at `:948`** | **`:949`** | **DRIFT +1** — `:948` is `#[serde(untagged)]` |
| `JsonFormatValue` span `936-960` | `936-960` | EXACT |
| `validate_json_format_value` at `:5568` | `5568` | EXACT |
| `mod tests {` at `:5789`, `use super::*;` at `:5790` | `5789` / `5790` | EXACT |
| `pub struct Bootstrap` at `:10`, close `:38`, derive `:8`, `deny_unknown_fields` `:9` | same | EXACT |
| `fn validate_hash_policy` at `:2344` | `2344` | EXACT |
| `pub(crate) fn validate(bootstrap: &mut Bootstrap)` at `:3446` | `3446` | EXACT |
| `validate_redirect_oneofs` at `:2677` | `2677` | EXACT |
| `ConfigError` span `lib.rs:74-1011` | `74`, close `1011` | EXACT |
| 125 variants **and** 125 `#[error(...)]` | `125` / `125` | EXACT, both ways |
| `RedirectSchemeRewriteConflict` is the last variant | `lib.rs:1010` | EXACT |
| `lib.rs` module list `:7-12` | `7-12` | EXACT |
| **`pub use bootstrap::{` at `lib.rs:16-49`** | **`:14-49`** | **DRIFT −2** on the start line |
| `validate()` call sites `lib.rs:1025` / `:1280` | `1025` / `1280` | EXACT |
| `ConfigDumpEntry::Bootstrap` at `endpoint.rs:298`, used `:532` | `298` / `532` | EXACT |
| `serde_json = "1"` at `Cargo.toml:18` | `18` | EXACT |
| fuzz `.gitignore` 67 lines / 64 `!` / 64 tracked seeds | `67` / `64` / `64` | EXACT |
| `runtime_key_is_rtds_inert` at `hcm.rs:5641` | `5641` | EXACT |

Zero-hit claims, both re-derived and both confirmed **0**:

```
$ git grep -c 'layered_runtime\|LayeredRuntime\|static_layer\|StaticLayer' -- 'crates/**/*.rs'
(no output)
$ git grep -l layered_runtime -- tests/ | wc -l
0
```

---

## §1 — Task 1: the `RuntimeValue` recursive value type

**RED** (Step 2) — the type does not exist, which is the correct RED here:

```
$ cargo test -p envoy-config --lib -- bootstrap::tests::runtime_value_binds_each_scalar_arm_in_declared_order
exit=101
error[E0425]: cannot find type `RuntimeValue` in this scope
     --> crates/envoy-config/src/bootstrap.rs:19424:73
error[E0433]: cannot find type `RuntimeValue` in this scope   (×7)
error: could not compile `envoy-config` (lib test) due to 8 previous errors
```

**Implementation** inserted **above the `/// A \`json_format\` value` doc line**,
NOT above the `#[derive]` — the landed 76.1 M-1 orphaning defect.

**GREEN** (Step 4):

```
test bootstrap::tests::runtime_value_binds_each_scalar_arm_in_declared_order ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 682 filtered out
```

**Step 5 — doc-orphan check, the trap this step exists for:**

```
$ grep -n -B3 '^pub enum JsonFormatValue' crates/envoy-config/src/bootstrap.rs
1012-/// `Object`. A bare number deserializes to none of these arms.
1013-#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
1014-#[serde(untagged)]
1015:pub enum JsonFormatValue {
$ grep -n -B3 '^pub enum RuntimeValue' crates/envoy-config/src/bootstrap.rs
 969-/// ADR-0049 all-fatal posture; upstream behaviour UNMEASURED).
 970-#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
 971-#[serde(untagged)]
 972:pub enum RuntimeValue {
```

Both types carry their own doc block; `JsonFormatValue`'s 11-line block is
intact directly above its own derive. **Not orphaned.**

**Step 8 — all four tests:**

```
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 682 filtered out
```

**Step 9 — gate (e):** `fmt` exit 0, **0 bytes** of output; `clippy` exit 0 with
**13** `Checking` lines (a zero would mean a fully-cached no-op).

**EXTRA mutation check — DD-4, not scheduled by the plan.** Task 1 has no
scheduled mutation, but DD-4's claim ("arm order is load-bearing and fails
SILENTLY") is the single load-bearing assertion of the task, so it was proven.
Scratch worktree `--detach` at the clean commit `fe1226a`; `Float` moved ahead of
`Int`; marker verified present before the run:

```
$ grep -n 'MUTATED: Float moved AHEAD of Int' crates/envoy-config/src/bootstrap.rs
975:    /// MUTATED: Float moved AHEAD of Int (DD-4)

test bootstrap::tests::runtime_value_nests_to_arbitrary_depth_and_stringifies_scalars ... ok
test bootstrap::tests::runtime_value_follows_yaml_1_2_and_records_the_cf_108_4_divergence ... ok
test bootstrap::tests::runtime_value_rejects_shapes_that_match_no_arm ... ok
test bootstrap::tests::runtime_value_binds_each_scalar_arm_in_declared_order ... FAILED
assertion `left == right` failed
  left: Float(42.0)
 right: Int(42)
test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 682 filtered out
```

**This is DD-4 demonstrated, not merely asserted:** the VARIANT test fails while
the **stringification test still PASSES** under the same mutation — `42`
stringifies to `"42"` either way. A `test result:` line is present, so this is a
behavioural RED, not a compile error.

**Unmutated control, same worktree:**

```
$ grep -c 'MUTATED' .../bootstrap.rs   → 0
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 682 filtered out
```

Worktree removed; removal re-verified from the repo root (`git worktree list`
shows only the main tree plus the parallel workstream's four `agent-*`).

**Commit `fe1226a`**, `git diff --numstat`: `194 0 crates/envoy-config/src/bootstrap.rs`.

---

## §2 — Task 2: `LayeredRuntime` / `RuntimeLayer` + the `Bootstrap` field

**RED** (Step 2):

```
$ cargo test -p envoy-config --lib -- bootstrap::tests::layered_runtime_absent_empty_and_populated
exit=101
error[E0609]: no field `layered_runtime` on type `bootstrap::Bootstrap`   (×4)
```

**GREEN** (Step 6): `test result: ok. 1 passed; 0 failed; ... 686 filtered out`.

**Step 7** — `deny_unknown_fields` + the `/config_dump`-inertness pin (the
mechanism gate (b) rests on):

```
test bootstrap::tests::layered_runtime_rejects_unknown_keys_and_stays_out_of_config_dump_when_absent ... ok
test bootstrap::tests::layered_runtime_absent_empty_and_populated_are_three_distinct_states ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 686 filtered out
```

**Commit `9b91bbf`**: `153 0 bootstrap.rs`, `5 5 lib.rs`.

### §2a — Task 2 FIXUP: a cross-crate break the plan did not anticipate

Gate (e) at this boundary **FAILED**, and the failure is a real finding:

```
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
clippy exit=101
error[E0063]: missing field `layered_runtime` in initializer of `envoy_config::Bootstrap`   (×4)
error: could not compile `envoy-listener` (lib test) due to 3 previous errors
```

**`Bootstrap` has public fields and no `#[non_exhaustive]`, so adding a field is
a CROSS-CRATE change.** Four `#[cfg(test)]` struct-literal initializers break:

```
crates/envoy-cluster/src/cluster.rs:3980
crates/envoy-listener/src/lib.rs:2245
crates/envoy-listener/src/lib.rs:2442
crates/envoy-listener/src/lib.rs:2493
```

`PLAN.md` Task 2 lists only `bootstrap.rs` and `lib.rs` under **Files** and never
greps for `Bootstrap {` literals. **`cargo test -p envoy-config` was fully
green** — only the workspace-wide `--all-targets` clippy exposed it.

Each site now sets `layered_runtime: None` with a comment recording that absent
is NOT the same as an empty block. Re-run:

```
fmt exit=0 bytes=0
clippy exit=0    Checking lines: 10    error lines: 0
$ git grep -c 'layered_runtime: None' -- '*.rs'
crates/envoy-cluster/src/cluster.rs:1
crates/envoy-listener/src/lib.rs:3
```

**Lesson, general:** adding a field to a struct with public fields is the same
class as widening a returnable error set — grep the construction sites, do not
trust a single-crate test run. Gate (e) DID catch this one (unlike the
`unreachable!()` class, which compiles clean), but only at workspace scope.

**Commit `774b24d`**: `4 0 cluster.rs`, `10 0 envoy-listener/src/lib.rs`.

---

## §3 — Task 3: the five `ConfigError` variants

**Pre-edit census** (Step 1) — both derivations agree:

```
variants: 125      #[error( attrs: 125      span: lib.rs:74 .. close 1011
```

**Post-edit census + forced rebuild** (Step 3):

```
end=1058
variants: 130      #[error( attrs: 130
build exit=0       Compiling envoy-config lines: 1
```

The `Compiling envoy-config` line is non-empty, so the build was **not cached**.

**Gate (e):** `fmt` exit 0 / 0 bytes; `clippy` exit 0, **13** `Checking`, 0 errors.
As the plan predicted, `ConfigError` is a `pub enum` in a library crate, so five
variants standing without a consumer for one commit do **not** trip `dead_code`.

**TDD note, recorded honestly:** Task 3 ships no test of its own. The variants
are pure data with no behaviour; their RED arrives in Task 4, whose tests
construct and match every one of the five. This is the plan's design, not a
skipped step.

**Commit `78bf1ad`**: `47 0 crates/envoy-config/src/lib.rs`.

---

## §4 — Task 4: `validate_layered_runtime` + its wiring into `validate()`

**RED** (Step 2):

```
$ cargo test -p envoy-config --lib -- bootstrap::tests::validate_layered_runtime bootstrap::tests::parse_bootstrap_runs_the_layered_runtime_validator
exit=101
error[E0425]: cannot find function `validate_layered_runtime` in module `super`   (×10)
```

**GREEN** (Step 5) — all four tests, covering all four reject rules, the
per-layer `position`, all three CF-108-1 arms and the `parse_bootstrap` wiring:

```
test bootstrap::tests::parse_bootstrap_runs_the_layered_runtime_validator ... ok
test bootstrap::tests::validate_layered_runtime_rejects_empty_and_duplicate_names ... ok
test bootstrap::tests::validate_layered_runtime_accepts_one_and_two_static_layers ... ok
test bootstrap::tests::validate_layered_runtime_enforces_oneof_cardinality_and_rejects_unsupported_arms ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 688 filtered out
```

**Gate (e):** `fmt` 0 bytes; `clippy` exit 0, **13** `Checking`, 0 errors.
**Commit `b220ae8`**: `219 0 bootstrap.rs` — committed CLEAN before mutating.

**Mutation check (Step 6)** — is the `validate()` call site load-bearing? Scratch
worktree `--detach` at `b220ae8`; call site commented out; both directions of the
marker check verified before running:

```
$ grep -n 'MUTATED: call site removed' crates/envoy-config/src/bootstrap.rs
3689:    // MUTATED: call site removed
$ grep -c 'validate_layered_runtime(lr)?;' crates/envoy-config/src/bootstrap.rs
0
```

```
test bootstrap::tests::parse_bootstrap_runs_the_layered_runtime_validator ... FAILED
thread '...' panicked at crates/envoy-config/src/bootstrap.rs:19976:48
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 691 filtered out
```

A `test result:` line IS present — a behavioural RED, not a compile error. The
duplicate-name config now parses successfully because the validator is
unreachable, which is exactly the claim under test.

**Unmutated control, same worktree:** marker count `0`,
`test result: ok. 1 passed; 0 failed; ... 691 filtered out`. Worktree removed and
removal re-verified from the repo root.

---

## §5 — Task 5: `runtime.rs` + arbitrary-depth flattening

**RED** (Step 2), after declaring `pub mod runtime;`:

```
$ cargo test -p envoy-config --lib -- runtime::tests
exit=101
error[E0425]: cannot find function `flatten_layer` in this scope   (×5)
```

**GREEN** (Step 4):

```
test runtime::tests::flatten_layer_handles_absent_and_empty_static_layers ... ok
test runtime::tests::flatten_layer_stringifies_every_scalar_and_keeps_the_empty_string ... ok
test runtime::tests::flatten_layer_recurses_to_arbitrary_depth_and_emits_no_intermediate_keys ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 692 filtered out
```

**Gate (e):** `fmt` 0 bytes; `clippy` exit 0, **13** `Checking`, **0** errors and
**0** warnings — see the deviation in §11 (D-2) for why.

**Commit `0f00b8f`**: `201 0 runtime.rs`, `1 0 lib.rs`.

**Mutation check (Step 5)** — is the recursion load-bearing? Worktree at
`0f00b8f`; `RuntimeValue::Map` arm emptied:

```
$ grep -n 'MUTATED: recursion removed' crates/envoy-config/src/runtime.rs
94:        RuntimeValue::Map(_) => { /* MUTATED: recursion removed */ }
$ grep -c 'flatten_into(&format!' crates/envoy-config/src/runtime.rs
0
```

```
test runtime::tests::flatten_layer_recurses_to_arbitrary_depth_and_emits_no_intermediate_keys ... FAILED
  left: ["flat.key"]
 right: ["flat.key", "my.nested.deeper.leaf", "my.nested.sub_key"]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 694 filtered out
```

**Unmutated control, same worktree:** marker `0`, `test result: ok. 1 passed`.
Worktree removed; verified from the repo root.

---

## §6 — Task 6: layer slots + last-NON-EMPTY precedence

**RED** (Step 2):

```
error[E0599]: no function or associated item named `from_layers` found for struct `runtime::RuntimeSnapshot` in the current scope   (×3)
```

**GREEN** (Step 4) — 6 tests, 3 from Task 5 + 3 here:

```
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 692 filtered out
```

**Gate (e):** `fmt` 0 bytes; `clippy` exit 0, **13** `Checking`, 0 errors.
**Commit `adbdb45`**.

### Mutation check (Step 5) — the most important one in the phase

"Last wins" is the natural WRONG implementation. Worktree at `adbdb45`;
`.rev().find(|v| !v.is_empty())` replaced by `.next_back()`:

```
$ grep -n 'MUTATED: last wins' crates/envoy-config/src/runtime.rs
109:                .next_back()          // MUTATED: last wins, not last NON-EMPTY
$ grep -c 'find(|v| !v.is_empty())' crates/envoy-config/src/runtime.rs
0
```

```
assertion `left == right` failed: final_value for only.in.base
  left: ""
 right: "base_val"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 697 filtered out
```

**The plan predicted this would name `empty.in.override` with
`left: ""` / `right: "real_value"`. It named `only.in.base` instead.** That is an
abort-at-first-failing-assert artifact, not a misaimed mutation: the table
reaches `only.in.base` (slots `["base_val", ""]`) before `empty.in.override`, and
both die under "last wins" because both end in an empty slot. **A plan's
predicted failure MODE is not a checkable fact** — the same lesson the 76.2
state-3 session recorded for its `&mut T` coercion prediction.

**Blast radius, censused across the whole module under the same mutation:**

```
test runtime::tests::flatten_layer_handles_absent_and_empty_static_layers ... ok
test runtime::tests::from_layers_keeps_an_all_empty_key_as_an_entry_with_an_empty_final_value ... ok
test runtime::tests::flatten_layer_stringifies_every_scalar_and_keeps_the_empty_string ... ok
test runtime::tests::flatten_layer_recurses_to_arbitrary_depth_and_emits_no_intermediate_keys ... ok
test runtime::tests::from_layers_reproduces_the_measured_two_layer_transcript ... FAILED
test runtime::tests::from_layers_gives_every_key_one_slot_per_configured_layer ... FAILED
test result: FAILED. 4 passed; 2 failed; 0 ignored; 0 measured; 692 filtered out
```

Note `from_layers_keeps_an_all_empty_key_...` **PASSES** under the mutation — a
single all-empty key yields `""` under both rules, so it is correct by
coincidence there and carries no discriminating power against this mutation.

**Is the `empty.in.override` cell INDIVIDUALLY load-bearing?** The first run could
not say, because it aborted earlier. Proven with a second probe **in the
throwaway worktree only** — the three earlier table rows removed so the cell is
reached, mutation still present (`grep -c 'MUTATED: last wins'` → `1`):

```
$ grep -n 'PROBE: earlier rows removed' crates/envoy-config/src/runtime.rs
301:            // PROBE: earlier rows removed so empty.in.override is reached
assertion `left == right` failed: final_value for empty.in.override
  left: ""
 right: "real_value"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 697 filtered out
```

**Exactly the plan's predicted cell and values.** The plan was right about the
cell and wrong only about which of two equally-fatal cells reports first. The
shipped test is unchanged; the probe existed only inside the removed worktree.

**Unmutated control, same worktree:** `MUTATED|PROBE` markers `0`,
`test result: ok. 6 passed; 0 failed`. Worktree removed; verified from repo root.

---

## §7 — Task 7: `RuntimeSnapshot::from_bootstrap`

**RED** (Step 2):

```
error[E0599]: no function or associated item named `from_bootstrap` found for struct `runtime::RuntimeSnapshot` in the current scope   (×3)
```

**GREEN** (Step 4): `test result: ok. 8 passed; 0 failed; ... 692 filtered out`.

**Gate (e):** `fmt` 0 bytes; `clippy` exit 0, **13** `Checking`, 0 errors.
**Commit `1878b62`**: `... crates/envoy-config/src/runtime.rs`.

**Mutation check (Step 5)** — are absent and empty collapsed? Worktree at
`1878b62`; the empty-block arm replaced by `RuntimeSnapshot::default()`:

```
$ grep -n 'MUTATED: collapse empty into absent' crates/envoy-config/src/runtime.rs
139:            return RuntimeSnapshot::default(); // MUTATED: collapse empty into absent
$ grep -c 'from_layers(vec![String::new()], &[])' crates/envoy-config/src/runtime.rs
0
```

```
assertion `left == right` failed: an empty block synthesizes ONE layer named the empty string ("layered_runtime: {}\n")
  left: []
 right: [""]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 699 filtered out
```

**Unmutated control, same worktree:** marker `0`, `test result: ok. 1 passed`.
Worktree removed; verified from repo root.

---

## §8 — Task 8: the fuzz corpus seed and its `!`-un-ignore line

**Pre-edit census** (Step 1) — reproduces the plan exactly:

```
lines: 67      bang: 64      seeds: 64
```

**The trap, reproduced BEFORE the fix:**

```
$ git check-ignore -v crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml
crates/envoy-config/fuzz/.gitignore:1:corpus/parse_bootstrap/*	crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml
exit=0
```

Line 1 blanket-ignores the whole corpus directory. Without the `!` line the seed
is invisible to `git add`, to `git status` and to CI.

**Step 4 — THE PROOF the task did not silently no-op:**

```
$ git ls-files 'crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml'
crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml

seeds: 65      lines: 68      bang: 65
$ tail -2 crates/envoy-config/fuzz/.gitignore
artifacts/
target/
```

All four post-edit invariants hit, and the trailing `artifacts/` / `target/` pair
is still last.

**One correction to the plan's expectation.** `PLAN.md` Step 4 expects
`check-ignore` to "print nothing and exit **1**". With `-v` it prints the matching
rule **even when that rule is a negation**, and exits 0:

```
$ git check-ignore -v ...layered_runtime.yaml
crates/envoy-config/fuzz/.gitignore:66:!corpus/parse_bootstrap/layered_runtime.yaml	...
exit=0
```

The plan's expectation is correct only for the PLAIN form. Settled both ways,
with a negative control so the check is not vacuous:

```
$ git check-ignore ...layered_runtime.yaml            → exit 1   (NOT ignored)
$ git check-ignore ...definitely_not_unignored.yaml   → exit 0   (ignored)
```

**Step 5 — the seed actually parses AND binds** (`envoy-bin` has NO `--mode`
flag; it takes `-c <path>` and writes `ConfigError` to STDOUT):

```
$ cargo build -p envoy-bin        → exit 0, "Compiling envoy-bin" lines: 1
$ timeout 5 ./target/debug/envoy-bin -c <seed with port_value: 0>
exit=124

INFO node registered node.id=fuzz-108.1 node.cluster=fuzz-108.1
INFO listener bound with SO_REUSEPORT (one accept queue per worker) addr=127.0.0.1:0 sockets=32
INFO envoy-rust listening (http_connection_manager) addr=127.0.0.1:0 stat_prefix=ingress_http codec_type=HTTP1
INFO SIGTERM received
INFO envoy-rust exited cleanly

$ grep -c 'ConfigError\|error' <output>   → 0
```

Exit **124** means `timeout` killed a still-running process: the config parsed,
the two-layer `layered_runtime` block validated, and the listener bound —
strictly stronger than a schema check.

**Commit `1829793`** — `git show --stat` lists **BOTH** files, so the seed was not
swallowed:

```
 crates/envoy-config/fuzz/.gitignore                |  1 +
 .../corpus/parse_bootstrap/layered_runtime.yaml    | 50 ++++++++++++++++++++++
 2 files changed, 51 insertions(+)
```

**No `ci.yml` change was made** — none is required or permitted; a corpus seed is
discovered from the filesystem, and `ci.yml` names only the fuzz TARGET.

---

## §9 — Task 9: ADR-0173

**Head re-derived from the MAX, never from a count:**

```
$ grep -o '^## ADR-[0-9]\{4\}' docs/envoy-rust/DECISIONS.md | sort -t- -k2 -n | tail -1
## ADR-0172                                    ← before
## ADR-0173                                    ← after
$ grep -c '^## ADR-' docs/envoy-rust/DECISIONS.md
169     ← counts the TEMPLATE near line 10; numbers are non-contiguous
        (0082, 0116, 0117, 0119 missing) — NEVER derive the next free number from this
```

**ADR-0173** is inserted at the HEAD of the newest-first block (`:2405`,
immediately above ADR-0172 at `:2428`), not at EOF. It carries the four decisions
`PLAN.md` Task 9 Step 1 specifies — DECISION 1 the CF-108-4 record-don't-normalise
disposition with the measurement that FORCED it, DECISION 2 opening CF-108-5,
DECISION 3 refuting the SPEC's `JsonFormatValue` novelty claim, DECISION 4 the
declared-then-rejected arms — plus the two implementation facts that are
decisions in substance (untagged arm order; the five reject shapes).

**Ledger head is now ADR-0173; next free is ADR-0174.**

---

## §10 — Task 9: regression sweep, the size gate, and gate (d)

### §10.1 — Size: the §6.1 mid-execution trigger did NOT fire

```
$ git diff --numstat fb143376e58aa8726cc248a8cc86e817c9b16ed2..HEAD -- . ':(exclude)docs/'
4	0	crates/envoy-cluster/src/cluster.rs
1	0	crates/envoy-config/fuzz/.gitignore
50	0	crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml
566	0	crates/envoy-config/src/bootstrap.rs
53	5	crates/envoy-config/src/lib.rs
449	0	crates/envoy-config/src/runtime.rs
10	0	crates/envoy-listener/src/lib.rs

added=1133 deleted=5 net=1128
```

**Net code LoC = 1128** against the plan's bottom-up **≈1215** (−7%) and the
§6.1 **~1500** gate — about **25% headroom**. No single task's sub-steps
exceeded ~10 items. **No split. §6.1's mid-execution trigger stays ARMED for a
§5.2 re-entry:** at `76.2`'s measured +24% re-entry overrun this slice would land
near **1399**, still under the gate — a materially better position than the
plan's own ≈1507 projection.

The table-driven design held: the three largest test groups were NOT expanded
into per-cell `#[test]` fns.

### §10.2 — Test count

```
$ git diff fb14337..HEAD -- 'crates/*.rs' | grep -cE '^\+\s*#\[(tokio::)?test\]'
18
```

**Exactly the plan's predicted 18** (T1 4, T2 2, T4 4, T5 3, T6 3, T7 2). The
binary count must stay **163** — this slice adds tests to `envoy-config`'s
existing lib target and creates no new test binary.

### §10.3 — Full-workspace regression sweep (gate (b))

Run **twice**, `--no-fail-fast` BEFORE the `--`, full output redirected to a
file — never piped through `tail`. **This is EVIDENCE for state 4, not a gate
adjudication** (§5.1; ADR-0127).

```
build exit=0
sweep1 exit=101       sweep2 exit=101      (~5.5 min each)

$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' <log> \
    | awk '{b++; p+=$4; fl+=$6} END{printf "binaries=%d passed=%d failed=%d\n", b, p, fl}'
sweep1: binaries=163 passed=2164 failed=6
sweep2: binaries=163 passed=2164 failed=6
```

**The arithmetic identity — the strongest flake-vs-regression discriminator —
CLOSES EXACTLY, on both sweeps:**

```
local passed + local failed = 2164 + 6 = 2170
CI baseline + this phase's tests = 2152 + 18 = 2170        ✓
binary count 163 UNMOVED                                    ✓
```

The CI baseline was **re-derived independently**, not inherited: run
`31065720371` on `fb143376e58aa8726cc248a8cc86e817c9b16ed2`,
`conclusion=success`, jobs enumerated via `gh api …/jobs` (because
`gh run view --log` returns only ONE job) at **15** steps (`build + test + lint`)
and **13** steps (`fuzz`), a **666562**-byte log, **0** `test result: FAILED`
lines:

```
correct recipe  (ok|FAILED), awk $4/$6 : binaries=163 passed=2152 failed=0
VACUOUS control, awk $5/$7           : binaries=163 passed=0    failed=0
```

The vacuous variant was reproduced **live on that same log** — disbelieve a zero.

**Failing tests, extracted from the `---- <name> stdout ----` markers** (never
censused by indentation, which invents phantom names from the failure body):

```
sweep 1                              sweep 2
access_log_h2_rcd_upstream_reset     access_log_h2_rcd_upstream_reset
access_log_h2_uc_upstream_reset      access_log_h2_uc_upstream_reset
access_log_rcd_upstream_reset        access_log_rcd_upstream_reset
access_log_rf_upstream_reset         access_log_rf_upstream_reset
admin_config_dump_server_info        admin_config_dump_server_info
upstream_grpc_health_check_fixture   access_log_rf_overflow
```

**The two sets DIFFER, and that is the ADR-0164 signature working as documented.**
Union **7**, intersection exactly the **stable core of five** (the four
`access_log_*_upstream_reset` binaries and `admin_config_dump_server_info`). The
tail turned over completely between sweeps — a single sweep could not have
satisfied ADR-0164 leg (iii) at all.

**Both tail members PASS IN ISOLATION** — the tail signature, measured:

```
$ cargo test -p differential --test access_log_rf_overflow
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.11s
$ cargo test -p differential --test upstream_grpc_health_check
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.04s
```

Both are `1 passed`, not `0 passed; N filtered out` (which is a FALSE GREEN that
also exits 0). Both failed in-sweep with `upstream Envoy never became
accept-ready` — a container-startup timeout, **assertion never reached**.

### The one RED that could plausibly have been this phase's fault

`admin_config_dump_server_info` (fixture `0014`) is a core member **and** asserts
`/config_dump` — the single place this slice could regress, via Task 2's
`skip_serializing_if`. **Its failure TEXT settles it:**

```
thread 'admin_config_dump_server_info' panicked at tests/differential/tests/admin_config_dump_server_info.rs:18:10:
fixture green: admin body rule: /clusters
```

**`/clusters`, not `/config_dump`** — the known `192.168.65.2` bridge-IP
backend-endpoint-address family. Not a regression.

**And the count-vs-line trap, which fired here and is benign:**

```
$ grep -c 'layered_runtime' <sweep log>
6
```

A scary non-zero. Adjudicated BY LINE: **all six are our own PASSING test names**
(`... ok`), **zero** inside any failure block, on both sweeps.

```
test bootstrap::tests::layered_runtime_rejects_unknown_keys_and_stays_out_of_config_dump_when_absent ... ok
test bootstrap::tests::layered_runtime_absent_empty_and_populated_are_three_distinct_states ... ok
test bootstrap::tests::parse_bootstrap_runs_the_layered_runtime_validator ... ok
test bootstrap::tests::validate_layered_runtime_accepts_one_and_two_static_layers ... ok
test bootstrap::tests::validate_layered_runtime_rejects_empty_and_duplicate_names ... ok
test bootstrap::tests::validate_layered_runtime_enforces_oneof_cardinality_and_rejects_unsupported_arms ... ok
```

**`108.1` adds no fixture, so no differential fixture can newly fail by
construction.**

### Remaining gate-(e) commands

```
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
exit=0    Checking lines: 0        ← FULLY CACHED NO-OP, NOT EVIDENCE
```

Re-run after dirtying `envoy-config` alone (`touch` guarded with `[ -f ]`, and
`git status --porcelain` immediately after confirmed it created nothing):

```
exit=0    Checking lines: 13       ← real
$ cargo fmt --all -- --check       exit=0, 0 bytes
$ cargo deny check                 exit=0
advisories ok, bans ok, licenses ok, sources ok
5 × "unmatched license allowance"  ← unmatched ALLOWANCES in deny.toml, NOT violations
```

**The ADR-0150 seam still holds, witnessed by the causal experiment rather than
an inherited count.** Dirtying `envoy-config` ALONE re-checks 13 crates and
**`envoy-accesslog` is ABSENT** from the list, exactly as its manifest predicts
(it depends only on `tokio`/`bytes`/`tracing`/`thiserror`, never on
`envoy-config`):

```
envoy-admin envoy-bin envoy-cluster envoy-config envoy-filter envoy-health
envoy-http1 envoy-http2 envoy-listener envoy-tcp envoy-tls
http1-echo-server http2-echo-server
$ grep -c 'Checking envoy-accesslog' <log>   → 0
```

**`108.1` puts the snapshot store inside `envoy-config`** (ADR-0172 DECISION 8),
already a dependency of `envoy-http1` and `envoy-admin` — no new edge, no cycle,
and the seam untouched.

### §10.4 — Gate (d), recorded EXPLICITLY per SPEC §6(d)

- **This slice adds NO new fuzz target.** Gate (d) is satisfied by the
  pre-existing `parse_bootstrap` target, which already covers the whole
  `Bootstrap` surface — the phase-66/67/76 disposition.
- **`ci.yml` therefore needs no change and received none.** `ci.yml:107` names
  only the fuzz TARGET, never a seed filename; a corpus seed is discovered from
  the filesystem. The fuzz job spans `ci.yml:77-134` and was not touched.
- **The new seed is proven TRACKED** by §8's `git ls-files`, with the tracked-seed
  census moving 64 → 65 and a negative control confirming the check discriminates.
- The short-budget `cargo fuzz` CI run itself is **state 4's** to execute and
  adjudicate; this session did not run it.

### §10.5 — Gate (a), (c) and the untouched surfaces

- **(a)** No new or changed differential fixture — vacuously satisfied, and
  RECORDED here rather than skipped silently. `tests/fixtures/` is byte-untouched
  (it does not appear in the diff above).
- **(c)** No H2 codec or framing change; `known-failures.txt` untouched (21 lines
  / exactly ONE real entry, `3.5/2`).
- **None of the eleven "no runtime subsystem" assertions is edited.**
  `runtime_key_is_rtds_inert` keeps its name and its wording — this slice builds a
  `static_layer` store that nothing reads, wiring neither the `RuntimeUInt32` nor
  the `RuntimeFractionalPercent` consumer.
- **The ADR-0150 seam is untouched** — `envoy-accesslog` gains no dependency; the
  snapshot store lives in `envoy-config`, already a dependency of `envoy-http1`
  and `envoy-admin` (ADR-0172 DECISION 8), so no new edge and no cycle.

---

## §11 — Deviations from `PLAN.md`, each with its reason

**D-1 — Task 2 Step 5: the re-export positions in the plan are alphabetically
WRONG, and were corrected.** The plan says `LayeredRuntime` goes "between
`LbSubsetSelector` and `Listener`" and that `RuntimeLayer` and `RuntimeValue` both
go "between `RuntimeFractionalPercent` and `RuntimeUInt32`". Sorted correctly,
`LayeredRuntime` precedes `LbEndpoint` (`a` < `b`) and `RuntimeValue` FOLLOWS
`RuntimeUInt32` (`U` < `V`). Placed at the true sort positions;
`cargo fmt --all -- --check` then passes with 0 bytes of output and rustfmt did
**not** reorder them, which is the mechanical confirmation that the corrected
placement — not the plan's — is canonical.

**D-2 — Task 5 Step 3: `LayeredRuntime` is NOT imported by `runtime.rs` until
Task 7.** Taken under the plan's own contingency (Task 5, note after Step 3). Its
only consumer is Task 7's `from_bootstrap`; importing it at Task 5 would fail that
task's `clippy -D warnings` boundary with `unused import`. It joins the `use` list
in the Task 7 edit. No `#[allow]` was added anywhere in this phase, as the plan's
ordering note predicted.

**D-3 — a Task 2 FIXUP commit was required** for four cross-crate `Bootstrap`
struct-literal initializers the plan does not mention. Full detail in §2a.

**D-4 — Task 1 gained a mutation check the plan does not schedule** (DD-4 arm
order). Detail in §1.

**D-5 — Task 6's mutation check gained a second probe** to prove the
`empty.in.override` cell is individually load-bearing after the first run aborted
earlier in the table. Detail in §6. The probe lived only in the removed worktree.

**D-6 — the plan's `git check-ignore` expectation is corrected** for the `-v`
form. Detail in §8.

**D-7 — commit messages carry the repository's `Co-Authored-By` /
`Claude-Session` trailers**, per the environment's standing git instruction. The
plan's message bodies are otherwise verbatim.

---

## §12 — What this session did NOT do

- **No state-4 verification and no gate adjudication.** Task 9 ends §5 state 3.
  State 4 is a SEPARATE session (§5.1; ADR-0127) — the context that ran the gate
  must not grade it. The sweep in §10.3 is EVIDENCE for state 4, not a verdict.
- **No `REVIEW.md`** (state 5), no ROADMAP status-cell flip (a state-3 commit
  flips none; row `108.1` stays `planned` until its close-out).
- **No fixture, no `expectations.yaml`, no `BEHAVIOR_CONTRACT.md` `## Runtime`
  section, no admin endpoint, no `runtime.*` stats** — all four are sibling
  `108.2`.
- **No new crate, no new dependency, no `Cargo.toml` / `Cargo.lock` change, no
  `ci.yml`, no `deny.toml`, no `known-failures.txt`, no `ENVOY_TARGET.md`, no
  `rust-toolchain.toml` change.**
- **No landed artifact of any closed phase edited** (D-3.5), and **no banked
  `76.1` / `76.2` Minor or Nit fixed** (§6.3 — a phase picks its scope, it does
  not clear a backlog).
- **No `stop` file.** The mission is not complete.
- **The parallel workstream's four `.claude/worktrees/agent-*` worktrees were not
  touched.** Every worktree this session created was its own and was removed, with
  removal re-verified from the repo root.

---

## §13 — CI on the state-3 head (measured after the Task 9 commit)

**This is a MEASUREMENT, not a gate adjudication.** State 4 owns §7.5 and must
re-confirm it rather than inherit this.

GitHub Actions recovered during this session (the `Incident with Actions` that
began `2026-08-06T15:22:49Z` moved to `monitoring`; the component reads
`operational`), so CI was confirmable. Run **`31134453388`** on the full 40-char
SHA `c002d796d74fbc62f924112f10d0aa7f65ccd158`:

```
$ gh api repos/pgdad/envoy-rust/actions/runs/31134453388/jobs --jq '...'
build + test + lint   success   15 steps   runner=GitHub Actions 1000004990
fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + ...)
                      success   13 steps   runner=GitHub Actions 1000004989

conclusion=success    log bytes: 730058
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' <log> | awk '{b++;p+=$4;fl+=$6}...'
binaries=163 passed=2170 failed=0
$ grep -c 'test result: FAILED' <log>
0
```

**Step counts are 15 / 13 as required**, both jobs ran on real runners (a
`runner_name:""` with `steps:0` would mean starvation and a rerun of the same
SHA), and the log is 730 KB — not the ~120-byte artifact a `gh` invocation from
outside the repo produces.

**The plan's prediction landed EXACTLY:**

```
predicted (PLAN.md Task 9 Step 4) : binaries=163  passed=2170
measured  (run 31134453388)       : binaries=163  passed=2170  failed=0
2152 (baseline, run 31065720371) + 18 (new tests) = 2170          ✓
binary count UNMOVED at 163 — no new test binary                  ✓
```

This also closes the identity from the other side: the local sweeps' `2164
passed + 6 failed = 2170` equals CI's `2170 passed`, i.e. **every one of the six
local REDs is green in CI** — the documented host-flake core plus tail, exactly
as ADR-0164 predicts.

**The three docs-only commits `4e80009` / `55dae04` / `879978f` still have no
run and never will** — the outage swallowed them and GitHub does not
retroactively create runs. They are docs-only (zero `crates/`/`tests/` bytes)
and this run builds their content anyway.
