# Sub-phase 108.1 — `layered_runtime` config surface + runtime snapshot store — CODE REVIEW

**Verdict: APPROVED.**

Per `BOOTSTRAP_PROMPT.md` §7.5, an approved `REVIEW.md` closes gate **(f)** — the only
gate still open. Gates (a), (b), (c), (d) and (e) were run and adjudicated by the §5
state-4 verification session and are recorded, with actual command outputs, at
`PROGRESS.md` §14. **This review did not re-run them and does not re-adjudicate them**
(§5.1; ADR-0127 — the context that ran the gate must not grade it, and the context that
grades it must not fix it). It re-confirmed CI on this HEAD independently (§0.3) because
that is a fact about the commit under review, not a re-run of the gate.

**Zero Issues. Seven Minors, twelve Nits.** Not one of them changes an accept/reject verdict,
alters any wire behaviour, or affects a differential fixture. Every one is either a
documentation-accuracy gap at a code site or a coverage gap on a surface that **nothing
reads yet** — 108.1 builds the producer; sibling `108.2` adds the first consumer. Per
§6.3 and ADR-0165 **nothing was fixed by this session**; the findings are banked for the
`108.1` state-6 close-out to carry and for `108.2` to weigh.

---

## §0 — How this review was conducted

### §0.1 — Scope

The unit of review is the non-`docs/` diff `879978f..17ce7a2`, measured with
`git diff --numstat 879978f 17ce7a2 -- . ':(exclude)docs/'` — **seven files, +1133 / −5,
net 1128 LoC**:

| file | + | − |
|---|---:|---:|
| `crates/envoy-config/src/bootstrap.rs` | 566 | 0 |
| `crates/envoy-config/src/runtime.rs` (NEW) | 449 | 0 |
| `crates/envoy-config/src/lib.rs` | 53 | 5 |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/layered_runtime.yaml` (NEW) | 50 | 0 |
| `crates/envoy-listener/src/lib.rs` | 10 | 0 |
| `crates/envoy-cluster/src/cluster.rs` | 4 | 0 |
| `crates/envoy-config/fuzz/.gitignore` | 1 | 0 |

`docs/` additions in the same range are `PROGRESS.md` (+1293), `DECISIONS.md` (+23,
ADR-0173), `STATE.md` (15/15) and `STATE_HISTORY.md` (+80) — artifacts, not code under
review.

### §0.2 — Method

Five read-only review dimensions were fanned out in parallel (the `RuntimeValue` type;
the snapshot store; the validators and `ConfigError` variants; the artifacts, banked
findings, fuzz seed and doc hygiene) with explicit instructions not to write and not to
run `cargo`. **Every finding below was RE-VERIFIED on disk by the main session**, and
every line number in this document was re-derived at this commit rather than quoted from
a subagent — a subagent finding is a claim, not a result, and its line numbers drift even
when its reasoning is right. Two suspected findings suggested in the dimension briefs
were **refuted** by the evidence and are recorded as such in §5, not filed.

### §0.3 — CI re-confirmed independently on this HEAD

Not inherited from the handoff. Run **`31168428279`** on the full 40-char SHA
`17ce7a2c2fc3449cf25354e3c0ed23f232edeb07`, `conclusion=success`, two jobs on REAL
runners (`GitHub Actions 1000005030` / `1000005031`) with step counts **15** and **13**:

```
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' <log> \
    | awk '{b++; p+=$4; f+=$6} END{print "binaries="b" passed="p" failed="f}'
binaries=163 passed=2170 failed=0

$ grep -c 'test result: FAILED' <log>
0
```

Log size **668 752 bytes** — asserted only to be in the hundreds of KB, deliberately NOT
compared to the inherited 540 681 (the same totals arrived in a 26 %-different log size on
the parent commit). The vacuous `$5`/`$7` field variant was reproduced LIVE on this same
log as a believable `binaries=163 passed=0 failed=0` — disbelieve a zero.

### §0.4 — The test-count arithmetic identity CLOSES EXACTLY, and closing it exposed a trap

The strongest available cross-check that no test was lost or double-counted:

| quantity | value | source |
|---|---:|---|
| CI `passed` on `fb14337` (last run before the implementation) | **2152** | run `31065720371`, 163 binaries |
| new `#[test]` functions in the diff | **+18** | 10 in `bootstrap.rs`, 8 in `runtime.rs` |
| CI `passed` on `17ce7a2` | **2170** | run `31168428279`, 163 binaries |

**2152 + 18 = 2170**, with the binary count unmoved at **163** on both. The state-2
commits between them (`4e80009`, `55dae04`, `879978f`) are docs-only and produced no CI
run at all — the 2026-08-06 GitHub Actions incident permanently swallowed them.

**A trap fired while deriving this, and it is worth banking.**
`git diff … | grep -c '^+.*#\[test\]'` returns a plausible **19**. The nineteenth hit is
the literal `` `#[test]` `` inside a PROSE COMMENT at `bootstrap.rs:19693`
(*"255 LoC vs ~400"*). The true count is **18**, confirmed by enumerating the function
names. Had 19 been believed, the identity would have appeared to miss by one and invited
a hunt for a lost test. **Adjudicate by LINE, never by COUNT — including in your own
cross-check.**

### §0.5 — Standing censuses re-derived at this commit

**86** fixture dirs (highest `0086`), **86** differential test files, **130**
`ConfigError` variants (by `#[error(...)]` count), **5** fuzz targets across five crates,
**65** tracked `parse_bootstrap` seeds, `fuzz/.gitignore` **68** lines / **65** `!` lines,
**21**-line `known-failures.txt`, **14** crates under `crates/` (still **no
`envoy-runtime`** — ADR-0172 DECISION 8 puts the store inside `envoy-config`), **114**
phase directories, **110** ROADMAP rows (**107 done / 1 in-progress / 2 planned**), ADR
head **ADR-0173** / next free **ADR-0174**. Rows `108` `in-progress`, `108.1` `planned`,
`108.2` `planned`. Three family headings still carry ZERO rows (`HTTP/3 + QUIC`, `gRPC`,
`WASM host`); `Runtime + hot restart` carries three.

---

## §1 — Strengths

1. **The load-bearing untagged arm order is correct AND pinned on the VARIANT, not on
   the value.** `bootstrap.rs:979-991` declares `Bool, Int, Float, Str, Map`, with `Int`
   ahead of `Float`. This is the single mistake on this surface that fails silently — with
   the arms reversed the integer `42` binds to `Float(42.0)` and *still stringifies to
   `"42"`*, so no value-level assertion catches it. The phase understood this: the
   stringification table (`bootstrap.rs:19685-19697`) is a **separate** test from
   `runtime_value_binds_each_scalar_arm_in_declared_order` (`:19634-19659`), whose
   `assert_eq!(m["i_pos"], RuntimeValue::Int(42))` at `:19653` is what actually REDs
   under a swap. ADR-0173 records that a scratch-worktree mutation proved exactly this:
   the variant test failed `left: Float(42.0)` / `right: Int(42)` while the
   stringification test **still passed**.

2. **"Last NON-EMPTY wins" is implemented literally and is discriminated by three
   independent tests.** `runtime.rs:104-113` is
   `.rev().find(|v| !v.is_empty()).cloned().unwrap_or_default()`. A "last wins" mutation
   REDs `from_layers_reproduces_the_measured_two_layer_transcript` (`runtime.rs:335`,
   `empty.in.override` → `["real_value",""]` / `"real_value"`),
   `from_layers_gives_every_key_one_slot_per_configured_layer` (`:358`) and
   `from_bootstrap_distinguishes_absent_from_empty_from_populated` (`:423-424`). The
   phase independently measured the mutation's blast radius (2 FAILED / 4 ok,
   `PROGRESS.md:395-401`) and **recorded honestly** that
   `from_layers_keeps_an_all_empty_key_...` passes under BOTH rules and therefore carries
   no discriminating power (`:411-418`). Recording a test's *lack* of power is rarer and
   more valuable than adding another that has it.

3. **The absent-vs-empty distinction (SPEC N-8) is honoured end to end, and the `Option`
   is genuinely load-bearing.** `from_bootstrap` (`runtime.rs:131-143`) maps `None` →
   zero layers and `Some` with an empty `layers` → **one synthetic layer named the empty
   string**, in both spellings. The synthetic layer correctly bypasses
   `validate_layered_runtime`, whose `EmptyRuntimeLayerName` check (`bootstrap.rs:2484`)
   would otherwise reject the very layer upstream synthesizes — and that bypass is
   *documented as deliberate* at `runtime.rs:128-130` rather than left to be discovered.

4. **Determinism is designed in, not incidental.** Every container in the data path is a
   `BTreeMap`: `entries` (`runtime.rs:44`), `flatten_layer`'s return (`:163`),
   `RuntimeValue::Map` (`bootstrap.rs:990`), `static_layer` (`:1053`). `layer_names` is a
   `Vec` — correct, since layer order is config order, not sorted. The only hash container
   is a `HashSet<&str>` used for a membership test whose iteration order never escapes
   (`bootstrap.rs:2482`). The module doc (`runtime.rs:16-19`) states *why*: 108.2's
   differential fixture rests on `serde_json::Map` being a `BTreeMap`, and this store is
   canonically ordered before `serde_json` is ever involved.

5. **The CF-108-4 disposition is argued, not asserted, and pinned at the code site.**
   `bootstrap.rs:960-967` carries the full reasoning: unquoted `y` and quoted `"y"` both
   arrive as `String("y")` — the quoting bit is destroyed by the scanner — so
   normalisation would *mint an opposite divergence on an equally legal spelling*. The
   test `runtime_value_follows_yaml_1_2_and_records_the_cf_108_4_divergence`
   (`:19700-19729`) pins it, and its `assert_eq!(m["a"], m["e"])` **is** the argument
   rather than an incidental assertion. This is what "recorded, not silently unhandled"
   should look like.

6. **The three unimplemented arms are DECLARED, and the reason is load-bearing rather
   than stylistic.** With `deny_unknown_fields` on `RuntimeLayer` (`bootstrap.rs:1045`),
   an *undeclared* `disk_layer:` would fail as an opaque serde error. Declaring them as
   `Option<serde_yaml::Value>` (`:1054-1062`) yields a precise `ConfigError` **and** makes
   the oneof cardinality countable, so `static_layer` + `admin_layer` reports a cardinality
   violation — matching upstream's measured complaint — instead of an unsupported-arm
   error. The house precedent (`HashPolicy`) is named. The ordering that produces this is
   itself pinned: the two-arm fixture at `bootstrap.rs:19928-19933` is deliberately
   `static_layer` + `admin_layer`, so a mis-ordered implementation trips the `other =>
   panic!` arm.

7. **The cross-crate `unreachable!()` hazard — the highest-consequence class this repo has
   on record (76.2 I-1) — was checked and is not reachable.** The five new variants are
   constructed at exactly five sites, all inside `validate_layered_runtime`
   (`bootstrap.rs:2485/2506/2513/2520/2527`). The one `ConfigError` catch-all
   `unreachable!()` in the tree (`crates/envoy-http1/src/rds_watcher.rs:235-241`, under
   `panic = "abort"`, `Cargo.toml:42`) sits downstream only of
   `reparse_and_select_route_config`, which never calls `bootstrap::validate()`. The
   warning comment the 76.2 fix left at `rds_watcher.rs:230-234` is still there and still
   correct.

8. **`Bootstrap`'s public-field blast radius was found and fixed completely, and with the
   semantically right value.** Adding a public field to a struct with no
   `#[non_exhaustive]` breaks every struct-literal initializer workspace-wide. All four
   sites — `envoy-cluster/src/cluster.rs:3999`, `envoy-listener/src/lib.rs:2256`, `:2456`,
   `:2513` — got `layered_runtime: None`, which is correct: they build no runtime stack,
   and `Some(LayeredRuntime::default())` would have been *wrong* under N-8. The plan did
   not mention them; the implementation caught them, took a fixup commit, and recorded the
   deviation (`PROGRESS.md` §11 D-3).

9. **Every SPEC non-goal held.** Nothing in the diff touches an admin endpoint, a
   `runtime.*` stat, a fixture, or `BEHAVIOR_CONTRACT.md`. **SPEC N-9's eleven "no runtime
   subsystem" assertions are verifiably unedited**: eight live in files absent from the
   diff (`hcm.rs` was last touched at `32a4c52`, a 76.2 commit, so
   `runtime_key_is_rtds_inert` keeps its name and its wording), and the three inside
   `envoy-config` are **byte-identical before and after**, checked by md5 over the
   surrounding block with the byte count asserted alongside to rule out the empty-file
   md5 (`528` bytes for the `RuntimeUInt32` block).

10. **The fuzz seed is genuinely tracked, and its guard was proven both ways.**
    `git ls-files` returns the path; plain `git check-ignore` exits **1** (NOT ignored).
    `fuzz/.gitignore` moved 67 → 68 lines / 64 → 65 `!` lines, and the tracked seed census
    for `parse_bootstrap` is **65**, matching the `seed corpus: files: 65` banner CI's fuzz
    job printed. The seed exercises two layers, every scalar arm, the empty string, and
    two-level nesting.

11. **Deviations from `PLAN.md` are recorded, not silently taken — and the set is
    provably complete.** Seven, at `PROGRESS.md` §11, including that the plan's re-export
    sort positions were alphabetically wrong (D-1, with `cargo fmt`'s *non*-reordering as
    the mechanical arbiter rather than the exit code alone) and that the plan's
    `git check-ignore -v` expectation was incorrect (D-6). A line-level diff of every
    added code line against `PLAN.md`'s literals found **no unrecorded deviation** — the
    only lines absent from the plan are rustfmt reflows, the D-1 placements and the D-3
    initializers. The plan's literal Rust transcribed clean: no clippy trip, no
    nonexistent helper, no non-compiling assert — the failure mode that hit `76.2` three
    times in one plan.

12. **`PROGRESS.md` is accurate where it can be checked.** Its §10.1 numstat reproduces
    byte-for-byte (`4 / 1 / 50 / 566 / 53−5 / 449 / 10`, added 1133 / deleted 5 / net
    1128); its `ConfigError` 125→130 prediction is confirmed both ways on disk; and its
    Contents block (`:11-19`) covers every actual heading including the §14 appended by
    the *separate* state-4 session — it does **not** reproduce the stale-Contents defect
    banked as `76.1` N-10, which is a measurable improvement on the landed sibling.

---

## §2 — Issues (Must Fix)

**None.**

No finding in this review changes an accept/reject verdict, alters wire behaviour,
introduces a reachable panic or abort, weakens a fixture, or leaves a validator unwired.

---

## §3 — Minor

### M-1 — A flattened-key collision inside ONE layer silently drops a declared value; the winner is deterministic but undocumented, unpinned and unmeasured

`crates/envoy-config/src/runtime.rs:175-186`

```rust
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

`static_layer` is a `BTreeMap<String, RuntimeValue>` (`bootstrap.rs:1053`), so
`a: {b: 1}` and `a.b: 2` are two **distinct** top-level entries that both flatten to the
key `a.b`. `out.insert` overwrites, so `num_keys` is 1 rather than 2 and one declared
value is lost with no diagnostic.

The winner is fully deterministic and was traced exactly: a nested spelling's top-level
key (`"a"`) is always a strict prefix of the flattened key it produces (`"a.b"`), and a
prefix always sorts before the full string in `BTreeMap` byte order — so **the literal
dotted spelling is visited last and always wins**. There is no nondeterminism risk for
108.2's `JsonShape` fixture.

**Why Minor and not an Issue:** nothing is measured against it. Greps over `SPEC.md`,
`PLAN.md`, `PROGRESS.md` and `DECISIONS.md` find no collision measurement, no test pins
it, and — most tellingly — SPEC §8 ("NOT MEASURED — stated explicitly per D-3.4",
`SPEC.md:447-466`) lists eight items and this is not among them. Upstream's behaviour is
unknown; ours may well match. **The gap is that a slice whose whole discipline is
"record, don't silently mishandle" (the discipline `flatten_layer`'s own doc applies to
CF-108-3 at `runtime.rs:157-162`) left this one unrecorded.** The honest fix is a
characterization test plus one line in 108.2's record, not a code change.

**Reached independently by the main session and by review dimension 2.** Per the standing
rule, weight a finding by how many independent routes reached it.

### M-2 — `RuntimeValue`'s type doc makes an accept/reject claim that the phase's own adjacent test refutes

`crates/envoy-config/src/bootstrap.rs:974-976`

```rust
/// A `null`, a sequence, an absent value, and an integer outside `i64` all match
/// NO arm and are boot-fatal (recorded reject-direction divergences under the
/// ADR-0049 all-fatal posture; upstream behaviour UNMEASURED).
```

`crates/envoy-config/src/bootstrap.rs:19746-19750`, landed in the same commit, measures
the opposite for the near boundary:

```rust
        // Measured contrast: an integer just past i64::MAX silently WIDENS to
        // Float rather than rejecting. Pinned so the boundary is not mistaken
        // for a hard i64 limit.
        let m = runtime_values("k: 9223372036854775808\n");
        assert!(matches!(m["k"], RuntimeValue::Float(_)), "got {:?}", m["k"]);
```

`9223372036854775808` **is** "an integer outside `i64`" and it is **accepted**, as
`Float`. Mechanism: `serde_yaml` widens it to `u64`; the untagged `Int(i64)` arm rejects
it and the `Float(f64)` arm's `visit_u64` accepts it. Only the far case
(`100000000000000000000`, `bootstrap.rs:19740`) genuinely rejects.

**Why Minor:** the behaviour on disk is correct and pinned, so no verdict changes. But
this is a doc comment *at the code site* making a false claim about an accept/reject
boundary, and the code site is exactly where `108.2` — which renders these values — will
read it. Noted in mitigation: ADR-0173's "two further implementation facts (b)" carries
both halves in one sentence, so this is an abbreviation at the code site rather than a
new error introduced here.

### M-3 — Non-finite floats are accepted, undocumented, untested, and break `Serialize`/`Deserialize` coherence

`crates/envoy-config/src/bootstrap.rs:977` and `:1002`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
```
```rust
            RuntimeValue::Float(f) => f.to_string(),
```

The chain, verified by reading the pinned dependency sources on disk (no `cargo` was run):

1. `serde_yaml 0.9.34+deprecated` parses `.inf` / `-.inf` / `.nan` into real `f64` values
   — `serde_yaml-0.9.34+deprecated/src/de.rs:1075-1082` returns `f64::INFINITY`,
   `f64::NEG_INFINITY`, `f64::NAN`. So `some.key: .nan` binds to `RuntimeValue::Float(NaN)`
   and boots clean.
2. `stringify()` then renders `"NaN"` / `"inf"` / `"-inf"` (Rust `f64` Display) into
   `RuntimeSnapshot::entries` — which is what `108.2`'s `GET /runtime` will serve.
3. `serde_json 1.0.149` writes JSON **`null`** for any non-finite —
   `serde_json-1.0.149/src/ser.rs:169-174`, `FpCategory::Nan | FpCategory::Infinite =>
   write_null(...)`. And `null` is precisely the shape `RuntimeValue`'s `Deserialize`
   **rejects**, pinned at `bootstrap.rs:19737` (`("null", "k: ~\n")`).

So a config that boots produces a `/config_dump` that cannot be re-ingested — the one
direction in which `Serialize` and `Deserialize` are required to agree. A grep for
`nan|\.inf|infinity|non-finite` over the 108.1 artifacts and `DECISIONS.md` returns
**zero** hits.

**Why Minor:** no fixture or corpus seed writes `.nan`/`.inf`, so no verdict changes today
and gate (b) is unaffected; envoy-rust never re-parses its own `/config_dump`. I also
checked the obvious escalation and it does not apply: **`Bootstrap` does not derive
`PartialEq`** (`bootstrap.rs:8` is `#[derive(Debug, Serialize, Deserialize)]`) and no
production path compares configs for equality, so `Float(NaN)`'s broken `PartialEq`
reflexivity has no blast radius. It becomes wire-visible the moment `108.2` renders
`/runtime`.

### M-4 — `Serialize` emits a JSON *number* for a float where SPEC N-3 measured upstream producing a *string*, and the resulting `/config_dump` divergence is unrecorded

`SPEC.md:45` names *"`Serialize` arms, to keep the `/config_dump` cascade whole"* as a D1
deliverable. `SPEC.md:163-167` measured upstream's YAML→JSON conversion:

> From the same dump: `my.numeric.key: 42` → JSON `42`, `my.negative.key: -7` → JSON `-7`,
> but **`my.float.key: 1.5` → JSON `"1.5"`, a STRING**. […] the value model must not
> assume floats arrive as JSON numbers.

envoy-rust's untagged `Float(f64)` (`bootstrap.rs:984-985`) serializes to `1.5` — a JSON
**number**. Bools and integers match upstream; floats do not. Nothing in the code, the
tests, `PROGRESS.md` or ADR-0173 records that the `/config_dump` cascade is therefore
**not** shape-equivalent for a float cell.

**The inferential step, stated plainly:** N-3 measured Envoy's YAML→JSON conversion (read
out of a `--mode validate` error dump), not `/config_dump` itself. That upstream's
`/config_dump` re-renders the resulting `Struct`'s `string_value` as `"1.5"` follows, but
**is not measured**. Graded Minor rather than filed as an ungraded hypothesis (§5's
disposition for the 76.2 default-port case) because the divergent half — what *our*
`Serialize` emits — is a fact on disk; only upstream's half is inferred.

**Consequence for the sibling:** this widens **CF-108-5** from a *rendering* question
(`1.0` → `"1"`?) to a *shape* question (number or string?), and `108.2` must measure both
before any float enters fixture `0087`.

### M-5 — `from_layers`' documented invariant is unenforced on a `pub` API and fails by silently dropping values

`crates/envoy-config/src/runtime.rs:70-72` (doc) and `:96-101` (behaviour)

```rust
    /// handed. **Invariant: `layer_names.len()` MUST equal `layers.len()` unless
    /// `layers` is empty**, in which case each key simply gets `layer_names.len()`
    /// empty slots — which is exactly the empty-block case.
```
```rust
                if let Some(entry) = entries.get_mut(key)
                    && let Some(slot) = entry.layer_values.get_mut(index)
                {
                    *slot = value.clone();
```

If a caller passes fewer names than layers, `layer_values.get_mut(index)` is `None` for
every overflow layer and their values vanish — their keys still surface in `entries`, with
all-empty slots and `final_value: ""`. A mis-call produces a plausible-looking **wrong**
snapshot rather than a loud failure.

Totality is the right call here given `panic = "abort"` (`Cargo.toml:42`) and I am **not**
asking for a panic — a `debug_assert!` would keep the guard without one. No caller
violates it today: `from_bootstrap` (`runtime.rs:131-143`) is the only one and is correct.
But `from_layers` is `pub`, and `108.2` is its first external caller.

**Reached independently by the main session and by review dimension 2.**

### M-6 — No positive `Serialize` round-trip pin; the serialization witness asserts only the absent direction

`crates/envoy-config/src/bootstrap.rs:19835-19840` is the entire serialization witness:

```rust
        let b = crate::parse_bootstrap(base).expect("valid");
        let dumped = serde_json::to_string(&b).expect("serialize");
        assert!(
            !dumped.contains("layered_runtime"),
            "an absent layered_runtime must not appear in /config_dump; got {dumped}"
        );
```

Nothing serializes a **populated** `layered_runtime`, and — more pointedly — nothing
proves the load-bearing N-8 distinction survives a serialize→deserialize hop. That
`Some(LayeredRuntime { layers: [] })` emits `{"layers":[]}` rather than being elided rests
entirely on `LayeredRuntime.layers` carrying no `skip_serializing_if`
(`bootstrap.rs:1022-1023`), which is currently unasserted. I read the derives as correct
and believe the behaviour is right; it is the **pin** that is missing, on a named SPEC
deliverable (`SPEC.md:45`).

**The gap is wider than it looks, and the whole tree was censused for it.** There are six
`serde_json::to_string*` call sites in `bootstrap.rs` (`:16453`, `:16528`, `:19836`,
`:20018`, `:20023`, `:20035`). Two of them are real serialize→deserialize round-trip
tests, but they run over **fixture `0008`'s config** (`:20005-20027`) and a **`node:`-only
minimal config** (`:20030-20037`) — **neither carries a `layered_runtime` block**. So the
round-trip machinery exists and this type never enters it. Consequence: changing
`bootstrap.rs:27`'s `#[serde(default, skip_serializing_if = "Option::is_none")]` to
`#[serde(default, skip)]` would compile, keep this test green, and **silently drop the
entire block from `/config_dump`**. *Stated as derived from reading, not measured — I did
not run that mutation (a state-5 session grades and does not mutate the tree).*

Note the interaction: M-3, M-4 and M-7 all live in exactly the gap M-6 leaves open. A
single round-trip test over a populated block would have surfaced at least the float
shape. Same class as `76.2` M-2 / M2-2 — *"not unpinned, only uncompared"* — except that
here the deliverable is not pinned in-process either.

### M-7 — The empty-nested-map cell is an unmeasured extrapolation asserted under a MEASURED citation, and it is banked nowhere

`crates/envoy-config/src/runtime.rs:150-151` (doc) and `:270-276` (test)

```rust
/// entry for either intermediate map. An empty nested map therefore yields
/// nothing at all — it has no leaves.
```
```rust
        // ...and neither does an EMPTY NESTED map, because it has no leaves.
        // SPEC §2 N-4: intermediate maps never produce entries of their own.
        let empty_nested = layer("name: l\nstatic_layer:\n  a.b: {}\n");
        assert!(
            flatten_layer(&empty_nested).is_empty(),
            "an empty nested map has no leaves and so yields no entry"
        );
```

SPEC §2 N-4 (`SPEC.md:169-183`) measured exactly one shape —
`my.nested: {sub_key: v, deeper: {leaf: w}}` — and says nothing about an **empty** nested
map. Whether upstream emits nothing, or an entry `a.b` with value `""` (which would also
bump `num_keys` under N-5), is **UNMEASURED**. Measured on disk: `a.b: {}` appears in no
§2 measurement table, in **no `SPEC.md` §8 NOT-MEASURED entry** (`:447-466`), and in no
ADR-0173 decision.

The doc's own phrasing (*"therefore"*) is honest about the inference; what makes this a
finding is that the **test cites `SPEC §2 N-4` as its authority** for a cell N-4 does not
contain, and the cell is banked nowhere. That is the class `76.2` **M2-3** was graded
Minor for exactly one slice earlier — an invented cell, neither pinned as invented nor
banked. Here it *is* pinned; what is missing is the label saying it is a choice rather
than a measurement.

---

## §4 — Nit

**N-1** — `num_layers()`'s doc contradicts the `layer_names` doc six lines above it on the
one case the phase treats as load-bearing. `runtime.rs:48-49` says *"the count of
CONFIGURED layers"*, but for `layered_runtime: {}` it returns **1** while zero layers are
configured. `runtime.rs:39-41` gets it exactly right. Behaviour correct; doc only.

**N-2** — The validator's doc claims a rule ordering the implementation applies per-layer,
not per-message. `bootstrap.rs:2469-2474` says *"duplicate names LAST because upstream
raises that at a post-PGV stage"*, but the duplicate check at `:2526-2530` sits **inside**
the per-layer loop. For `[{a, static}, {a, static}, {c, no-arm}]` envoy-rust reports
`DuplicateRuntimeLayerName` where upstream (PGV over all layers first) would report the
layer-2 `layer_specifier is required`. Both **reject**, and error text is explicitly
outside the equivalence contract (§7.2, `SPEC.md:65`) — the comment merely overstates the
fidelity.

**N-3** — CF-108-5's code-site record keeps two of the four unconfirmed float cells
ADR-0173 enumerates. `bootstrap.rs:969-972` names `1.0 → "1"` and `1e6 → "1000000"`;
`DECISIONS.md` ADR-0173 DECISION 2 names those plus `-0.0 → "-0"` and
`1e-7 → "0.0000001"`. The carry-forward *is* recorded at the code site, which is what
matters; this is an abbreviation.

**N-4** — `static_layer:` written with an explicit null value (key present, no value)
yields all four arms `None` and rejects as `RuntimeLayerMissingSpecifier`, because
`#[serde(default)]` on `Option<T>` (`bootstrap.rs:1052-1053`) covers only a *missing* key
while an explicit YAML null deserializes to `None`. Upstream's proto3-JSON treatment of
`null` on a **oneof member of message type** is genuinely ambiguous between "field unset"
(same reject) and "oneof set to a default `Struct`" (accept). UNMEASURED on both sides and
unrecorded. Distinct from the already-recorded case of a null *inside* `static_layer`
(`bootstrap.rs:974`, `:19737`).

**N-5** — Static-layer KEY shapes are neither validated nor measured.
`static_layer: {a: {"": 1}}` yields the key `"a."`; `static_layer: {"": 1}` yields `""`.
`validate_layered_runtime` (`bootstrap.rs:2481-2533`) validates layer **names** only and
never inspects a static-layer key. Same bucket as M-1 and absent from SPEC §8.

**N-6** — `flatten_into`'s `scalar =>` catch-all (`runtime.rs:182`) forfeits part of the
exhaustive-`match` forcing function: a future recursive `RuntimeValue` arm would be
silently treated as a leaf here. The guard is only *partly* lost — `stringify`'s match
(`bootstrap.rs:999-1005`) is exhaustive and would still fail to compile — but the same
class is on record from `76.2` CF-76-2 (`if let` forfeiting the forcing function).

**N-7** — Two untested reject compositions: two *unsupported* arms together
(`disk_layer` + `rtds_layer`, which must yield `RuntimeLayerMultipleSpecifiers`, not
`UnsupportedRuntimeLayerType`), and an empty name combined with a missing arm (the name
check must win). Both traverse code paths already exercised at `bootstrap.rs:19928-19933`
and `:19880-19884`; verdicts are unaffected either way.

**N-8** — Avoidable allocation, all at config-parse time. `from_layers`
(`runtime.rs:82-102`) clones every key and every value out of `flattened`, a **local owned
`Vec` dropped at end of scope** — consuming it with `into_iter().enumerate()` would move
both instead. `flatten_into` allocates twice per nested leaf: `format!("{prefix}.{key}")`
at `:179`, then `prefix.to_string()` at `:183` re-allocating the identical bytes.

**N-9** — `RuntimeValue` now sits adjacent in the re-export list (`lib.rs:37`) to
`RuntimeUInt32` and `RuntimeFractionalPercent`, the unrelated `runtime_key` gating types
for `status_code_filter` and CSRF. The type doc (`bootstrap.rs:948-953`) carefully
disambiguates `RuntimeValue` from `JsonFormatValue` but says nothing about these two.

**N-10** — **Six** of SPEC N-9's eleven line citations no longer resolve, drifted by this
diff's own insertions (`+7` at `bootstrap.rs:19`, `+122` at `:933`, `+69` at `:2334`, `+8`
at `:3484`; `+1` at `lib.rs:10`). All eleven were verified correct at the base commit
`879978f`; the five that did not move live below every insertion point or in untouched
files.

| # | SPEC citation | resolves at HEAD | drift |
|---|---|---|---:|
| 2 | `lib.rs:760-765` (`UnsupportedRuntimeKeyedCsrfFilterEnabled`) | `761-766` | +1 |
| 3 | `bootstrap.rs:4657-4678` (`validate_csrf_config`'s reject) | `4863-4884` | +206 |
| 4 | `bootstrap.rs:843-846` (`RuntimeUInt32`'s doc) | `850-853` | +7 |
| 7 | `lib.rs:469-474` (`EmptyStatusCodeFilterRuntimeKey`) | `470-475` | +1 |
| 8 | `lib.rs:752-758` (`UnsupportedNonDeterministicCsrfFilterEnabled`) | `753-759` | +1 |
| 9 | `bootstrap.rs:1368-1372` (`RuntimeFractionalPercent`'s doc) | `1497-1501` | +129 |

Item 3 is the sharpest: `bootstrap.rs:4657` now lands nowhere near
`validate_csrf_config`, which is at `:4863`. The **text** at all eleven sites is
byte-identical (§1 item 9), so the SPEC's *claim* — that 108.1 edits none of them —
remains measurably true. `SPEC.md` is a landed artifact and is **not editable** (D-3.5),
and it anticipated this itself (`:474-476`: *"re-derive every line anchor in §2 by TEXT
before transcribing it … both drift"*). Recorded so a `108.2` session re-anchors by text
rather than following the numbers.

**N-11** — `LayeredRuntime.layers` is the one new `pub` field in the diff without a doc
comment (`bootstrap.rs:1021-1024`). Every other new `pub` item is documented — all five
of `RuntimeLayer`'s fields eight lines below (`:1047`, `:1050`, `:1053`, `:1056`, `:1059`)
and all four of `runtime.rs`'s. The struct-level doc (`:1009-1018`) does carry the
load-bearing empty-vs-absent rationale, so nothing is undocumented in substance. Same
class as banked `76.1` N-2 (`RedirectAction`'s eight undocumented `pub` fields) but a NEW
instance on a NEW type, so it is filed rather than deferred to the bank. `envoy-config`
enables no `missing_docs` lint, so gate (e) is structurally blind to it.

**N-12** — The corpus seed exercises only the ACCEPT path. `layered_runtime.yaml` covers
two layers, every scalar arm, the empty string, two-level nesting and the N-7 precedence
cell — but **no** tracked seed anywhere reaches the three CF-108-1 arms
(`git grep -l 'disk_layer\|rtds_layer\|admin_layer' -- 'crates/*/fuzz/corpus/*'` → **0**
files), any of the four D2 reject rules, the empty-block spellings `layered_runtime: {}` /
`layers: []` (the one distinction the `Option` exists for), or the CF-108-4 `y`/`n`/`on`/
`off` spellings — which are *permitted* in a corpus, since ADR-0173's prohibition is
scoped to fixture `0087`. Nit rather than Minor because a corpus asserts only "no panic",
so a seed cannot detect a wrongful accept or reject; this is coverage *shape*, and it is
the grading banked `76.1` N-9 received for the same phenomenon.

---

## §5 — Deliberate decisions verified, and suspected findings REFUTED — not filed

1. **`RuntimeValue::Map(_) => String::new()` in `stringify` is deliberate and safe**
   (`bootstrap.rs:1004`). Its rationale is documented at `:995-997`, and it is genuinely
   unreachable from `flatten_into`, which matches `Map` first (`runtime.rs:177`). Returning
   `""` rather than panicking is correct under `panic = "abort"` because `108.2` renders
   snapshots.

2. **`RuntimeValue` derives `PartialEq` but not `Eq` — correct**, it holds an `f64`.
   `RuntimeEntry` / `RuntimeSnapshot` (`runtime.rs:25`, `:37`) derive `Eq` — also correct,
   they hold only `String` and `Vec<String>`.

3. **`RuntimeSnapshot` having no production caller is deliberate, not dead code.**
   `runtime.rs:10-14` states it: 108.1 builds the producer and wires neither the
   `RuntimeUInt32` nor the `RuntimeFractionalPercent` consumer, which is *exactly why*
   every "no runtime subsystem" assertion in the tree stays true. `pub mod runtime`
   (`lib.rs:13`) means no dead-code lint fires. **A reviewer meeting the test name
   `runtime_key_is_rtds_inert` inside a phase titled "runtime" would read a contradiction
   — SPEC §2 N-9 pre-empts it, deliberately, and the disposition holds.**

4. **REFUTED — "the arm-order test may only assert the stringified value."** Suspected in
   the dimension brief; false. The variant assertion and the stringification table are
   two separate tests (`bootstrap.rs:19653` vs `:19685-19697`), and under derived
   `PartialEq` a swap genuinely REDs the former.

5. **REFUTED — "the last-non-empty rule may have no discriminating test."** Suspected
   because the phase itself recorded one non-discriminating test; false. **Three** tests
   discriminate against a "last wins" mutation (§1 item 2).

6. **Stack-overflow risk on arbitrary-depth recursion — checked, and bounded.**
   `serde_yaml 0.9.34+deprecated` seeds `remaining_depth: 128` (`src/de.rs:112`, `:133`)
   and both `visit_sequence` and `visit_mapping` wrap in `recursion_check`, which returns
   `RecursionLimitExceeded` rather than recursing; the counter propagates through the
   untagged `Content`-buffering path `RuntimeValue` uses. Separately, `flatten_layer` is
   **not fuzz-reachable at all** — `fuzz_targets/parse_bootstrap.rs` calls only
   `parse_bootstrap`, and only *deserialization* is exercised. I regard the bound as sound
   but state the inference: I did not construct an adversarial anchor/alias input to test
   whether the limit can be bypassed.

7. **No second config-ingestion path can install an unvalidated `layered_runtime`.** The
   RDS warm path (`rds.rs:102-172`) takes a `&Path` plus a route-config name and returns a
   `RouteConfiguration`; the EDS warm path (`eds_reload.rs:97`) returns a
   `ClusterLoadAssignment`. Neither ever sees a `Bootstrap`. `from_bootstrap` does
   deliberately bypass the validator (documented, `runtime.rs:128-130`), and its safety
   rests entirely on every production `Bootstrap` coming from `parse_bootstrap` — worth
   `108.2` keeping in view, since it is `pub`.

8. **An unactionable observation, recorded only so a future reader is not misled:**
   `DECISIONS.md` ADR-0171 describes `final_value` as "last-layer-wins", which SPEC N-7
   later corrected to last-**non-empty**-wins. The implementation follows the corrected
   rule and ADR-0173 supersedes it. A landed ADR is never edited (D-3.5) — there is
   nothing to fix.

---

## §6 — Status of already-banked findings — read BEFORE grading, and NOT re-issued

The `76.1` / `76.2` Minors and Nits are **banked and were deliberately not fixed** (§6.3
— a phase picks its scope, it does not clear a backlog). Re-issuing one as a new finding
is this project's documented failure mode. All three prior reviews were read before any
grading:

| review | verdict | banked |
|---|---|---|
| `76.1/REVIEW.md` | APPROVED | M-1…M-6, N-1…N-11 |
| `76.2/REVIEW.md` (round 1) | CHANGES-REQUESTED — **frozen, never rewritten** | M-1…M-9, N-1…N-9 |
| `76.2/REVIEW-2.md` (round 2) | APPROVED | M2-1…M2-7, N2-1…N2-15 |

**Not one banked item is re-issued here, and the structural reason is decisive: every
banked item is about the *redirect* surface** — `RouteAction` / `RedirectAction`,
`host_redirect` / `port_redirect` / `strip_query`, `BEHAVIOR_CONTRACT.md` §F, fixture
`0086`, the RDS warm path. **The `layered_runtime` surface did not exist before this
sub-phase**, so identity-level overlap is impossible by construction.

Four banked **classes** could plausibly have recurred on a new config surface. Each was
checked explicitly, and none did:

| banked class | origin | result on 108.1 |
|---|---|---|
| A type inserted before a `#[derive]` orphans the doc comment above it | `76.1` M-1 / M-2 | **Does not recur — it was actively defended against.** All three insertion points were read on disk: `JsonFormatValue`'s 11-line doc (`bootstrap.rs:1065-1075`) is intact above its own `#[derive]` at `:1076`; `validate_hash_policy`'s 8-line doc (`:2471-2478`) is intact; `dynamic_clusters`'s doc (`:30-35`) is intact. `PLAN.md:139` names the 76.1 M-1 defect explicitly as the reason for the chosen anchor, and `PROGRESS.md:86-102` records the verification. **This is a banked finding being consumed as a lesson, not re-derived as a defect.** |
| Cardinality tests cannot distinguish "none set" from "more than one set" | `76.1` M-4 | **Does not recur.** Each test asserts a **distinct variant** with a hard `other => panic!` fallthrough (`bootstrap.rs:19920-19922` vs `:19930-19932`), so neither case can satisfy the other's assertion. |
| A warm/reload path re-validates and skips the new validators | `76.1` M-5 / `76.2` M-8 / CF-76-2 | **Does not recur.** No warm path carries a `Bootstrap` (§5 item 7). |
| Widening a returnable error set lands in a caller's `unreachable!()` and aborts | `76.2` I-1 / M2-1 | **Does not recur.** No `ConfigError` catch-all is reachable from the five new variants (§1 item 7). |

**Carry-forwards, status at this review.** `108.1` **opened CF-108-5** (float rendering
beyond the measured `1.5` cell) and **closed CF-108-4** (YAML 1.1 vs 1.2 — decided in
favour of recording, because normalisation is not implementable; ADR-0173 DECISION 1).
**CF-108-1** (the three loudly-rejected arms), **CF-108-2** (`/runtime_modify` absent) and
**CF-108-3** (a nested map containing `numerator` is kept as one text-format key) pass
through unchanged. **No carry-forward is consumed by this slice.**

---

## §7 — Severity dissent, recorded rather than silently resolved

A review must record its own dissent rather than quietly picking a grade.

1. **M-3 (non-finite floats) — Minor vs Issue.** The case for Issue: a config that boots
   emits a `/config_dump` that its own parser rejects, and `/runtime` would serve `"NaN"`
   against an entirely unmeasured upstream. The case for Minor, which carried: no
   accept/reject verdict changes, no fixture or corpus seed reaches it, envoy-rust never
   re-ingests its own `/config_dump`, and the obvious escalation path — `Float(NaN)`
   breaking `PartialEq` reflexivity — was checked and is inert, because `Bootstrap` does
   not derive `PartialEq`. **Graded Minor. If `108.2` puts `/config_dump` or `/runtime`
   on a differential wire without first bounding this, the grade should be revisited.**

2. **M-1 (flattened-key collision) — Minor vs Nit.** The case for Nit: it is an
   unmeasured input shape on an unread surface, and our behaviour may well match upstream.
   The case for Minor, which carried: it **silently discards a value the operator wrote**,
   and the omission is from SPEC §8 — the very list whose purpose is to state what was not
   measured. **Graded Minor**, on the omission rather than on the behaviour.

3. **M-4 (`/config_dump` float shape) — Minor vs an ungraded §5 hypothesis.** The 76.2
   precedent files an unverified claim *about upstream* as an ungraded hypothesis, never
   as a defect. This is a hybrid: our side is measured on disk, upstream's side is inferred
   from a measured adjacent cell. **Graded Minor with the inferential step stated in the
   finding**, so a reader can downgrade it themselves if the inference fails.

4. **M-7 (empty-nested-map extrapolation) — Minor vs Nit.** The reviewing dimension that
   raised it declared its own uncertainty and said reasonable reviewers could grade it a
   Nit, *because the extrapolation is very likely correct*. That is exactly the argument
   the `76.2` M2-3 precedent rejects: correctness of the guess is not the axis — whether
   the cell is labelled a choice and banked is. **Graded Minor**, on the mislabelling
   (`SPEC §2 N-4` cited as the authority for a cell N-4 does not contain), not on the
   behaviour, which is probably right.

5. **N-11 and N-12 are new instances of banked CLASSES, not re-issued banked ITEMS.**
   `76.1` N-2 (undocumented `pub` fields) and `76.1` N-9 (thin corpus coverage) are banked
   against the *redirect* types. N-11 and N-12 are the same phenomena on types created by
   this sub-phase. Filing them is correct; deferring them to the bank as if already
   recorded would not be. Both are graded identically to their banked counterparts.

---

## §8 — Carry-forwards for the state-6 close-out to bank

- **CF-108-5 is OPEN and is now WIDER than ADR-0173 recorded it.** It was opened as a
  *rendering* question (which `f64` Display cells match upstream). M-4 shows it is also a
  *shape* question (JSON number vs JSON string on the `/config_dump` cascade), and M-3
  shows non-finite floats are a third, wholly unbounded axis. **`108.2` must measure all
  three before any float enters fixture `0087`.**
- **CF-108-1, CF-108-2, CF-108-3 remain OPEN and unconsumed.**
- **THREE input shapes on the new surface are absent from SPEC §8's NOT-MEASURED list**
  and should be added to `108.2`'s equivalent: **M-1** (a flattened-key collision inside
  one layer), **M-7** (an empty nested map), and **N-5** (empty or dot-bearing static-layer
  key segments). SPEC §8 lists eight items and none of these is among them. A fourth,
  **N-4** (an explicit-null `static_layer:` arm), belongs in the same list.
- **New this review, for whichever slice next touches the surface:** M-5 (`from_layers`'
  unenforced invariant — relevant the moment `108.2` calls it), M-6 (no positive
  `Serialize` round-trip pin, which is where M-3, M-4 and M-7 all hide), N-11 (an
  undocumented `pub` field) and N-12 (a corpus seed with no reject-path coverage).
- **The 76.1 / 76.2 Minors and Nits stay BANKED and unfixed** (§6.3), as do the
  older-phase carry-forwards, **CF-76-1**, **CF-75-2**, **CF-75-3**, **CF-75-4**,
  **CF-75-5** and **CF-75-6**.
- **A method note worth carrying:** `grep -c` for `#[test]` over a diff returns a
  plausible over-count because a prose comment can quote the attribute (§0.4). The same
  class as the `tests/fixtures/*/` glob returning a believable zero — **disbelieve a
  plausible number as readily as a zero.**

---

## §9 — Assessment

`108.1` is a disciplined foundation slice. It does exactly what its SPEC scoped and
nothing more: every one of the eleven non-goals held, the eleven "no runtime subsystem"
assertions are verifiably byte-unedited, and the four `Bootstrap` struct-literal sites the
plan never mentioned were found, fixed correctly and recorded as a deviation. The two
places where this surface fails **silently** rather than loudly — untagged arm order and
last-non-empty precedence — are precisely the two the phase pinned hardest, each with a
mutation check on record and each with a test that asserts the thing a wrong
implementation would still make look right.

The seven Minors share one shape: they sit in the seam between the **producer** this slice
built and the **consumer** it deliberately did not. Five of them (M-1, M-3, M-4, M-6, M-7)
are about values or shapes that nothing reads today and that `108.2` will read on the
first day it exists. That is the correct place for them to be found, and **none of them is
a defect in what was built — they are gaps in what was recorded about it.** The slice's
own stated discipline is "record, don't silently mishandle", and it met that bar on every
question it *asked* (CF-108-1 through CF-108-5 are all banked at their code sites); the
findings are the questions it did not think to ask.

**Gate (f) is CLOSED. All six §7.5 gates are now GREEN. Sub-phase `108.1` is approved to
land.**

**Next state: §5 state 6 — the close-out** (ROADMAP row `108.1` `planned` → `done`;
parent row `108` stays `in-progress` until sibling `108.2` is done), a **separate
session** per §5.1 and ADR-0127 — a reviewer must not close out what it graded. This
review **fixed nothing**, as ADR-0165 requires.
