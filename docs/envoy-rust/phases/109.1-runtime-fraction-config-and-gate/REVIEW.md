# Sub-phase 109.1 — `RouteMatch.runtime_fraction` config surface, three boot-fatal validators, the store's first typed lookup, the LIVE gate — CODE REVIEW

**Verdict: APPROVED.**

Per `BOOTSTRAP_PROMPT.md` §7.5, an approved `REVIEW.md` closes gate **(f)** — the only
gate still open. Gates (a)-(e) were run and adjudicated by the §5 state-4 verification
session and are recorded, with actual command outputs, at `PROGRESS.md`
`## State-4 verification`. **This review did not re-run them and does not re-adjudicate
them** (§5.1; ADR-0127 — the context that ran the gate must not grade it, and the context
that grades it must not fix it). It re-confirmed CI on the tree under review independently
(§0.3) because that is a fact about the commits, not a re-run of the gate.

**Zero Issues. Five Minors, six Nits.** Not one changes an accept/reject verdict, alters
wire behaviour, weakens a fixture, or leaves a validator unwired. Three Minors are
mutation-survivability gaps in single test rows of an otherwise unusually strong table,
and two are ledger-accuracy gaps. Per §6.3 and ADR-0165 **nothing was fixed by this
session**; the findings are banked for the `109.1` state-6 close-out to carry and for
`109.2` (or a later slice) to weigh. One handed observation was CONFIRMED **and its
handed remedy REFUTED** (M-1) — the refutation is this review's most useful output.

---

## §0 — How this review was conducted

### §0.1 — Scope

The unit of review is the non-`docs/` diff `a460b38..9a7e7f8` (seven task commits
`b30be96` T1 / `cb1cf26` T2 / `09769e1` T3 / `a9e0ed6` T4 / `2d9bbbf` T5 / `3181291` T6 /
`9a7e7f8` T7), measured with `git diff --numstat a460b38 9a7e7f8 -- . ':(exclude)docs/'`
— **15 files, +1880 / −154, net 1726 LoC**:

| file | + | − |
|---|---:|---:|
| `crates/envoy-config/src/runtime.rs` | 444 | 5 |
| `crates/envoy-http2/src/hcm.rs` | 419 | 95 |
| `crates/envoy-http1/src/hcm.rs` | 342 | 35 |
| `crates/envoy-config/src/bootstrap.rs` | 308 | 0 |
| `crates/envoy-config/src/rds.rs` | 152 | 15 |
| `crates/envoy-http1/src/rds_watcher.rs` | 110 | 3 |
| `crates/envoy-config/src/lib.rs` | 46 | 0 |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/runtime_fraction_route.yaml` (NEW) | 36 | 0 |
| `crates/envoy-bin/src/main.rs` | 10 | 0 |
| `crates/envoy-bin/src/runtime_stats.rs` | 6 | 1 |
| five one-line mechanical files (`endpoint.rs`, `instance.rs`, `jwt_authn.rs` ×3-line, `types.rs`, fuzz `.gitignore`) | 6 | 0 |

The only `docs/` change in the range is the ONE-blockquote D7 narrowing in
`BEHAVIOR_CONTRACT.md` (+5/−4 — the sentence spans lines). `109.1/SPEC.md` and `PLAN.md`
are byte-identical across the range (zero diff), as D-3.5 requires. The three state-3
doc narrowings (runtime.rs module doc, runtime_stats.rs module doc, the contract
blockquote) are inside the range and were reviewed as part of it.

### §0.2 — Method

Three read-only review dimensions were fanned out in parallel (the typed-lookup cascade
vs SPEC §1.3; the validator wiring at all three paths + classifier + seam + live gate;
the TDD/git-history/deviation audit) with explicit instructions not to write and not to
run `cargo`. **Every finding below was RE-VERIFIED on disk by the main session**, and
every line number in this document was re-derived at `9a7e7f8`'s tree rather than quoted
from a subagent. Separately, the main session ran **four mutation checks in a scratch
worktree** (`git worktree add --detach` at HEAD `78fe53f`, whose `crates/` tree is
identical to `9a7e7f8`'s), each with an unmutated control from the same tree, a forced
rebuild proven by `Compiling` lines, and a gate on the presence of the `test result:`
line — see §0.5.

### §0.3 — CI re-confirmed independently on the tree under review

Not inherited from the handoff. The seven code commits were pushed together with the
state-3 ledger commit `9331ce3fe845597caae07b86e8bde8742caab77a` (verified docs-only on
top of `9a7e7f8`: STATE.md/STATE_HISTORY.md/PROGRESS.md, zero `crates/` delta), so the
CI run on `9331ce3` is the run on this exact code tree: run **31572355578**,
`conclusion=success`, steps **15**/**13**. Build-job log (selected by NAME `build + test
+ lint`, job id `94036874661`, 408015 bytes — byte count asserted alongside every census
per the empty-md5 trap):

```
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' <log> \
    | awk '{b++; p+=$4; f+=$6} END{print "binaries="b" passed="p" failed="f}'
binaries=164 passed=2193 failed=0
$ grep -c 'test result: FAILED' <log>
0
```

The state-4 head `c3e6177` (docs-only again) independently re-confirmed the same
identity: run **31653334370**, build-job `94302199702`, **164/2193/0** (recorded in
`STATE.md ## Last commit` by `78fe53f`).

### §0.4 — The test-count arithmetic identity CLOSES EXACTLY

| quantity | value | source |
|---|---:|---|
| CI `passed` on base `a460b38` (the state-2 record commit, last run before the implementation) | **2180** | run `31536324684`, build-job log 406643 bytes, binaries=164 failed=0 |
| new `#[test]`/`#[tokio::test]` attributes in the diff | **+13** | 2 runtime.rs (T1), 1 bootstrap.rs (T2), 3 (T3: bootstrap ×2 incl. the LDS-mod placement + jwt), 4 (T5: 3 rds.rs + 1 rds_watcher.rs), 3 (T6: 2 http1 + 1 http2); zero removed |
| CI `passed` on `9331ce3` / `c3e6177` | **2193** | runs `31572355578` / `31653334370`, binaries=164 |

**2180 + 13 = 2193**, binary count unmoved at **164** on both ends. The 13 function
names were enumerated from the diff and reconcile one-for-one with the PROGRESS ledger,
including the recorded name deviation `reparse_rejects_map_shaped_runtime_key`. The
count was adjudicated by LINE (enumerated attributes + fn names), not by a bare
`grep -c` (the §0.4 trap banked at the 108.1 review).

### §0.5 — Mutation evidence, run by this session (scratch worktree, never the main tree)

All four target exactly the scrutiny candidates handed from states 3-4 (the KEY ARM, not
a message list; the classifier's abort trap; the Err fallback; the live gate). Controls
first, from the same worktree: `runtime::tests::route_fraction` filter → `ok. 2 passed`,
`reload_warm_rejects_nondeterministic` → `ok. 1 passed`, `honors_runtime_fraction_gate`
→ `ok. 2 passed`.

| mutation | site | expected witness | result |
|---|---|---|---|
| M1: `if v >= 100.0` → `if v > 100.0` | `runtime.rs` key arm | 23-cell table REDs (cells 4/9/12/F2 pin the ==100 boundary) | `FAILED. 1 passed; 1 failed`, 1 `Compiling` |
| M2: Err fallback `numerator != 0` → `== 0` | `route_fraction_passes` | wrapper test REDs | `FAILED. 1 passed; 1 failed`, 1 `Compiling` |
| M3: drop `UnsupportedNonDeterministicRuntimeFraction` from the classifier's `update_rejected` arm | `rds_watcher.rs` | `reload_warm_rejects…` REDs via the `unreachable!()` abort | `FAILED. 0 passed; 1 failed`, 5 `Compiling` |
| M4: invert the gate (`&& !runtime.…` → `&& runtime.…`) | `route_matches` | both T6 gate tests RED | `FAILED. 0 passed; 2 failed`, 1 `Compiling` |

Every mutation run carries a `test result:` line (none is a compile error masquerading
as a RED) and a non-zero `Compiling` count (no stale-binary false verdict). The worktree
was verified clean and removed. **M3 is the important one: the 76.2 I-1 abort-trap class
is not merely closed in code — the test genuinely fires on the omission.**

### §0.6 — Standing censuses re-derived at this commit

**87** fixture dirs (highest `0087`) / **87** differential test files / **164** test
binaries; **134** `ConfigError` variants; fuzz `.gitignore` **69** lines / **66** `!`
lines / **66** tracked corpus files (`git ls-files`, incl. the T2 seed); **21**-line
`known-failures.txt` (ONE real entry, untouched); **14** crates (no `envoy-runtime` —
ADR-0172 D8); **117** phase directories; ROADMAP **113 rows / 110 done / 1 in-progress
(parent 109) / 2 planned**; ADR head **ADR-0176**, next free **ADR-0177** (this review
decided nothing new — it stays UNRESERVED); `ci.yml` untouched by the range (no new fuzz
target ⇒ none owed).

---

## §1 — Strengths

1. **The cascade is a faithful, ORDER-CORRECT transcription of SPEC §1.3, and the one
   ordering that fails silently is pinned.** `route_fraction_gate`
   (`runtime.rs:163-211`) runs the MapShapedKey prefix check BEFORE the scalar lookup,
   which is what makes the D3 conservative reject (scalar-in-later-layer +
   map-in-earlier-layer) actually conservative. The test row `gate.k: 100` beside
   `gate.k.foo: 1` expecting `MapShapedKey` (`runtime.rs:813-816`) REDs under the
   swapped order, and the sibling negative control (`gate.k2` is NOT a `gate.k` map,
   `runtime.rs:826-834`) pins both the trailing dot and the "iff" direction.

2. **The BTreeMap `range(prefix..)` + `starts_with` idiom is provably sound, not
   accidentally sound.** For byte-lexicographic `String` order: any entry `X >= "K."`
   not starting with `"K."` must exceed `"K."` at some index `i < len("K.")`, and every
   `"K."`-prefixed entry `Y` shares those first bytes with `"K."`, so `X > Y` — hence if
   any prefixed entry exists, the FIRST entry `>= "K."` is one of them. One `O(log n)`
   probe replaces a scan, with no false negatives possible. The consulted key itself
   (`"gate.k" < "gate.k."`) is correctly outside the range.

3. **The reload-classifier abort trap (76.2 I-1, the highest-consequence class on
   record) is closed and CROSS-DERIVED.** The review independently derived
   `reparse_and_select_route_config`'s returnable set by reading its body and every
   callee transitively (`parse_rds_file` → `RdsFileError`/`RdsParseError`;
   `RdsRouteConfigNotFound`; `validate_route_runtime_fraction` → the three new variants;
   `UnknownCluster`; `validate_redirect_oneofs` → the two Redirect conflicts) — NINE
   exactly, matching the classifier arms at `rds_watcher.rs:203-244` and its updated
   comment, with the jwt variant's exclusion argued in place (unreturnable — RDS route
   configs carry no jwt rules). The classifier test asserts all five contract points
   (Err variant / `update_rejected==1` / `update_failure==0` / live-table `Arc::ptr_eq` /
   no abort) plus the full counter identity, and §0.5 M3 proves it fires on the exact
   omission the trap is about.

4. **No production path can carry an empty snapshot, and there is no second
   route-matching path to bypass.** Every `RuntimeSnapshot::default()` in the workspace
   sits in `#[cfg(test)]` code except `from_bootstrap`'s own absent-`layered_runtime`
   arm (`runtime.rs:137`, the landed 108.1 semantics). `main.rs:63` builds the snapshot
   ONCE (after `load_dynamic_resources`, mirroring the post-merge validation view) and
   `Arc::clone`s it at the only three production `from_config` sites (`main.rs:492/554/
   632`; the fourth caller is inside `endpoint.rs`'s test mod). `route_matches` is
   called only from `resolve_route_in`/`build_response_in`, which are reached only via
   the public wrappers (both passing `&config.runtime`) and the keep-alive loop's two
   direct sites (both `&config.runtime`). H2's every diff hunk starts inside `mod tests`
   — zero production edits, and H2 production contains no local route iteration or
   prefix comparison: `resolve_route(&config.inner, …)` is the only path, exactly the D4
   design.

5. **Validation-path coverage is real at all three paths, with the ordering right.**
   `validate()` runs `validate_layered_runtime` (`bootstrap.rs:3698`) BEFORE building
   the snapshot (`bootstrap.rs:3704`), so no invalid layer shape reaches
   `from_bootstrap` (which is total anyway); an `rds:`-configured HCM carries no inline
   routes and is re-validated post-merge through the SAME `validate()` from
   `load_dynamic_resources`; the RDS warm path validates every route of every vh BEFORE
   the action match (`rds.rs:157-162`) against the store's boot snapshot
   (`rds_watcher.rs:190`) — a warm config can never install a gate the byte-identical
   boot config would reject.

6. **The RouteMatch consumer census closes.** Exactly two `RouteMatch`-typed fields
   exist in the tree (`Route.r#match`, `RequirementRule.r#match`); the jwt consumer is
   boot-fatal on presence (CF-109-3, `bootstrap.rs:4816-4821`) rather than silently
   inert, and the hand-copied matcher `route_match_matches` is byte-untouched. `Route`'s
   hand-written impls delegate to the derived `RouteMatch` impls
   (`map.next_value::<RouteMatch>()` / `serialize_entry("match", …)`), so the field
   rides the config_dump cascade with no hand-impl edit — verified, not assumed.

7. **TDD evidence is coherent across all seven commits, and the deviations ledger is
   complete on everything semantic.** Per-commit `--stat` shapes match PROGRESS
   file-for-file; the five recorded deviations are all real on disk (the `codec_type`
   fix — including the committed fuzz seed repair in `09769e1`; the LDS-test placement;
   the code-built snapshot layers where no serde_yaml dev-dep exists; the W-1
   misclassification with the compiler's E0063 list as authority; the trailing-comma
   repair). The T5 RED is the RIGHT shape — a behavioral `got Ok(())` after the compile
   shims, not a compile error counted as a RED. The landed T6 `build_response`
   assertions are STRONGER than the PLAN's (`assert_eq!` on the exact body where the
   PLAN sketched `contains(`) — a weak-assertion pattern was removed in transcription,
   not introduced.

8. **The fuzz seed is genuinely tracked and genuinely parses.** `git ls-files` returns
   66 corpus files including `runtime_fraction_route.yaml`; the `.gitignore` negation
   sits before `artifacts/`; the seed carries the `codec_type: HTTP1` repair (the PLAN's
   own yaml omitted a required field — the `plan-md-example-code-trips-clippy` memory
   class, caught and recorded at T3, exactly what a parse-check owes).

9. **Every SPEC non-goal held.** No fixture touched (empty diffstat over `tests/`), no
   `expectations.yaml`, no `## Runtime` contract subsection (the ONE blockquote
   narrowing is D7's, and nothing else in `BEHAVIOR_CONTRACT.md` moved), no M-1
   correction, `runtime_key_is_rtds_inert` and `validate_csrf_config` md5-identical
   across the range, `HEADER_ALLOW_LIST` untouched, `ENVOY_TARGET.md` /
   `rust-toolchain.toml` / `ci.yml` untouched.

---

## §2 — Issues (Must Fix)

**None.**

No finding changes an accept/reject verdict, alters wire behaviour, introduces a
reachable panic or abort, weakens a fixture, or leaves a validator unwired.

---

## §3 — Minor

### M-1 — The `"empty runtime_key"` edge row is non-discriminating — CONFIRMING the state-4 handoff — and the HANDED REMEDY IS ALSO NON-DISCRIMINATING; do not bank it as the fix

`crates/envoy-config/src/runtime.rs:751-756` (the row) and `:167` (the rule it fails to
pin — `.filter(|k| !k.is_empty())`).

The row (`one("0")`, `rf(0, Hundred, Some(""))` → `Never`) passes whether or not the
empty key is treated as consulted: under the mutation that deletes the `is_empty`
filter, the "consulted" path computes `prefix = "."` (no entry matches), misses
`entries.get("")`, and falls to the same default → `Never` either way. Confirmed by
control-flow trace, independently by two review dimensions and the main session.

**The refutation that matters:** the state-4 handoff prescribed "a discriminating pin
needs a diverging default (numerator 100)". FALSE — with `rf(100, Hundred, Some(""))`
over snapshot `gate.k: 0`, the unfiltered path STILL misses `get("")` and returns
`Always`, identical to the filtered path. The two readings differ observably only when
the snapshot itself contains an entry literally named `""` (filtered → default;
unfiltered → the `""` entry's value parses) or an entry starting with `"."` (filtered →
default; unfiltered → `MapShapedKey`). Both snapshots are buildable via the existing
`snap()` test helper, which bypasses the boot validators. A future pin must use one of
those; the diverging-default remedy would land a second non-discriminating row while
reading as a fix.

**Why Minor:** the pinned RULE is real and correct (upstream-unmeasured edge, recorded
as the absent-like reading in the PLAN); only this one row's witness power is weak, on
an edge reachable only through exotic snapshots. Behaviour unaffected.

### M-2 — The `is_finite` guard is entirely unpinned: BOTH non-finite rows survive its deletion

`crates/envoy-config/src/runtime.rs:181` (the guard); `:715-726` (the two rows).

Delete `&& v.is_finite()` and both edge rows still pass: `NaN` fails all three
comparisons (`== 0.0`, `>= 100.0`, `> 0.0` — every NaN comparison is false) and falls
through to its expected default under ANY default choice; the `inf` row pairs the
spelling with default **100**, the one direction that masks the mutation (`inf >= 100.0`
→ `Always` = the expected value). The discriminating combination is `one("inf")` with
default **0** (guarded → `Never` via default; unguarded → `Always`), and it matters
because SPEC §1.3 step 2 explicitly assigns non-finite spellings to the default arm.
Same mutation-survivability class as M-1; behaviour correct, witness absent.

### M-3 — The default-Always arm's `denominator.value()` consultation is unpinned: a hard-coded `numerator == 100` survives the entire 23-cell table

`crates/envoy-config/src/runtime.rs:203`.

Every row that reaches the default-Always arm uses `rf(100, Hundred, …)`; cell 9's
MILLION default is on the Never side (numerator 0) and short-circuits at the key anyway.
Mutating `p.numerator == p.denominator.value()` to `p.numerator == 100` passes every Ok
row and both `NondeterministicDefault` rows (150 ≠ 100 → still Err). One row —
`rf(1_000_000, Million, None) → Always` — would pin the consultation. Same class as
M-1/M-2. (The three findings together say one thing: the table's 23 measured cells are
exhaustively pinned, but three of the cascade's GUARDS are witnessed only from their
masked side. A future slice touching `runtime.rs` should add the three discriminating
rows — ~6 lines total.)

### M-4 — PROGRESS's session-summary LoC sentence is contradicted by measurement

`PROGRESS.md` session summary ("running net LoC ≈ the PLAN's ≈1180 projection") vs
measured `git diff --numstat a460b38..9a7e7f8`: **+1885/−158 = net +1727** (crates-only
+1880/−154 = 1726), i.e. **+46%** over the ≈1180 projection and above SPEC §8's 800-1140
band. Per-task decomposition of the overrun: T1 436 vs ≈290, T3 309 vs ≈250, **T4 302 vs
≈115** (the call-site fan-out's multi-line style and rustfmt reflows were unpriced), T5
241 vs ≈125, T6 252 vs ≈205; T2 and T7 on target. No process consequence — §6.1
thresholds are plan-time estimates and the only mid-execution trigger (a task's
sub-steps passing ~10 items) never fired, so ADR-0177 staying unreserved was CORRECT —
but the ledger sentence is false as written, and the landed PROGRESS is uneditable
(D-3.5), so this finding is the record. The `calibrate-loc-estimate-against-landed-phases`
memory is confirmed a third time (76.1 +50%, now 109.1 +46%, both concentrated in
test/mechanical halves).

### M-5 — Two fan-out literal sites landed non-canonical, contradicting the ledger's "cargo fmt --all canonicalized" — and `fmt --check` is structurally blind to them

`crates/envoy-http2/src/hcm.rs:1925` and `:2045` (both inside `#[cfg(test)]`):

```
                         runtime_fraction: None, },
```

— the field glued onto the `RouteMatch` closing brace at off-by-3 indentation,
workspace-unique (grep = exactly 2 hits). Semantically correct (the field IS inside the
braces; the workspace compiles and CI is 2193/0). `cargo fmt --all -- --check` passes
because rustfmt declines to reformat these overlong nested literals, so no gate ever
catches it — the same "the gate is blind to this class" shape as the banked
`fmt-only-check-needs-a-whole-file-compare` lesson. Graded Minor on the
UNRECORDED-DEVIATION axis (T2's ledger says the fan-out was canonicalized; these two
sites were not), not on the cosmetics; the wiring dimension graded the cosmetics Nit —
dissent recorded in §6.

---

## §4 — Nit

**N-1** — `route_fraction_passes`'s doc says the fallback follows "the `default_value`'s
**sign**" (`runtime.rs:218`); `numerator` is `u32` (`bootstrap.rs:1310`) — non-zero-ness
is meant. Doc wording only.

**N-2** — The wrapper test exercises the `Err(_)` fallback only through the
`NondeterministicValue` class (`runtime.rs:866-887`). The arm is a single `Err(_)`, so
this is adequate; a `MapShapedKey`-class row would cost two lines and complete the set.

**N-3** — A 17-significant-digit spelling like `"99.99999999999999"` rounds to exactly
`100.0` at f64 parse and gates `Always` where the cascade's boot-fatal `0 < v < 100` arm
notionally applies. Inherent to the SPEC-endorsed single-parse design (§1.3: "a single
`f64` parse covers integers exactly"), upstream-unmeasured, unreachable from any measured
cell — recorded so the boundary is a documented choice, not a surprise.

**N-4** — `FractionGate`/`FractionGateError` landed BELOW `impl RuntimeSnapshot`
(`runtime.rs:234/:243`) where PLAN Task 1 step 3 says "types above it". PROGRESS
describes the landed placement without flagging it as a deviation. Zero semantic effect;
listed because the deviations ledger is otherwise complete.

**N-5** — The three `rds.rs` reject tests assert variant-only (`{ .. }`, `rds.rs` tests
at the `reparse_rejects_*` fns), so the `rds:<path>` listener-context convention is
unpinned there (the bootstrap-path counterparts do pin `key == "gate.k"`). Variant
discrimination is what the classifier consumes, so the contract that matters is tested.

**N-6** — `route_fraction_passes` returns `true` for a `NondeterministicDefault` of
150/HUNDRED — which happens to MATCH upstream for the `>` sub-case (upstream accepts
`numerator > denominator` as always-pass, parent D2(a)), and is an always-pass
approximation for the sampled `0 < n < d` sub-case. VALIDATED-UNREACHABLE at all three
paths, deliberately non-panicking (the 76.2 I-1 lesson), documented at the code site.
Recorded for completeness; no change requested.

---

## §5 — Deliberate decisions verified, and the handed scrutiny candidates adjudicated

1. **The Err fallback deliberately does NOT `unreachable!()`** (`runtime.rs:213-225`) —
   correct under `panic = "abort"`; a wrong-route answer beats a process death, and the
   fallback is upstream-consistent for the one sub-case with a measured upstream answer.
   The state-4 handoff asked this be scrutinized: scrutinized, and §0.5 M2 proves the
   fallback expression itself is pinned.

2. **The classifier test's "third outcome class" (a GREEN exposing a weak assertion)
   did not occur** — §0.5 M3 REDs, so the test's assertions are load-bearing, and the
   test asserts all five contract points plus the counter identity.

3. **The 23-cell table's key arm is genuinely pinned** — §0.5 M1 REDs on the `>= 100`
   boundary mutation (cells 4/9/12/F2 all pin ==100 as Always). The handed "aim at the
   KEY ARM" scrutiny is satisfied; the residual unpinned guards are M-1/M-2/M-3, none of
   which is the key arm.

4. **Gate placement inside `route_matches` (rather than at each call site) is the right
   seam** — both production call sites and every test call site inherit it atomically;
   §0.5 M4 shows both T6 tests RED on inversion, and the H2 witness exercises the exact
   production call path on a `from_config`-built inner.

5. **`rds:<path>` as the listener context in the RDS validator** — the
   `validate_redirect_oneofs` convention, recorded in PROGRESS T5; verified consistent.

6. **The empty-runtime_key ABSENT-LIKE reading** (`filter(|k| !k.is_empty())`) is a
   recorded choice on an upstream-unmeasured edge (PLAN Task 1 doc: "the absent-like
   reading, recorded in the PLAN") — the choice stands; only its witness is weak (M-1).

7. **F1/F2's reliance on `RuntimeValue::stringify` Display-rendering** (YAML `0.0` →
   `"0"`) matches SPEC §1.2 mitigating fact 1 and the landed 108.1 behaviour —
   verified, the rows are honest about the mechanism in their labels.

---

## §6 — Severity dissent, recorded rather than silently resolved

1. **M-1 — Important vs Minor.** The cascade dimension graded it Important "solely so
   the main session does not bank the insufficient remedy as a fix", while grading the
   test gap itself Minor. The main session adjudicates: the FINDING on disk is the gap
   (Minor — exotic-snapshot edge, correct behaviour, recorded choice); the remedy
   refutation is captured in the finding text and in the state-6 carry-forward, which is
   what the Important flag was for. **Graded Minor with the refutation made loud.**

2. **M-5 — Minor vs Nit.** The wiring dimension graded the two glued literals Nit
   (cosmetic, test-only); the process dimension graded Minor (an unrecorded deviation
   from an explicit ledger claim, invisible to every gate). **Minor carried, on the
   ledger axis** — the project's standing discipline is that the deviations ledger is
   provably complete, and this is the one entry it missed.

3. **M-4 — Minor vs Nit.** Considered Nit (the number harms nothing; the split decision
   it feeds was correct anyway). Minor carried: the sentence is a measured-false claim
   in an evidence ledger, and the third confirmation of a systematic estimation bias is
   exactly what the calibration memory exists to accumulate.

---

## §7 — Status of already-banked findings — read BEFORE grading, NOT re-issued

`108.1/REVIEW.md` (APPROVED; M-1…M-7, N-1…N-12 banked), `108.2/REVIEW.md` (APPROVED;
M-1 DISPOSED — decided-IN, rides in 109.2 per ADR-0176 DECISION 5; M-2 + N-1…N-6
banked), and the 76.1/76.2 banked families were read before grading. **No banked item is
re-issued here.** Identity-level overlap is impossible for most (the redirect and
admin-/runtime-endpoint surfaces); the classes were checked:

| banked class | origin | result on 109.1 |
|---|---|---|
| A test whose claimed discrimination power is false | 108.2 M-2 | **Recurs as a class, on new code** — M-1/M-2/M-3 are new instances on rows/guards created by this sub-phase; filed per the 108.1 §7.5 precedent (a new instance on a new type is filed, not deferred). |
| Widened returnable error set lands in a caller's `unreachable!()` | 76.2 I-1 / banked memory | **Does not recur — actively closed AND mutation-witnessed** (§0.5 M3). |
| Doc-comment orphaning by mechanical insertion | 76.1 M-1/M-2 | **Does not recur** — 122 `///` lines added, 0 removed, field docs attached to fields; checked per-commit. |
| A warm/reload path skips the new validators | 76.1 M-5 / CF-76-2 | **Does not recur** — the RDS path validates BEFORE the action match against the boot snapshot; the classifier extension landed test-first. |
| PLAN literal yaml/code fails its own gate | 76.2 (3×), memory `plan-md-example-code-trips-clippy` | **Recurred and was caught in-flight** — the `codec_type` omission (T3 deviation (a), including the committed seed repair). Recorded, not silent. |

**Carry-forwards, status at this review:** CF-109-1 (WIDENED), CF-109-2, CF-109-3 remain
OPEN — this slice lands their REJECT sides; the honoring sides stay future work.
CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6 pass through untouched. No carry-forward is
consumed by this slice.

---

## §8 — Carry-forwards for the state-6 close-out to bank

- **M-1's remedy correction supersedes the state-4 handoff's**: a discriminating
  empty-key pin needs a snapshot with a literal `""` entry or a `.`-prefixed entry —
  NOT a diverging default. Whichever slice adds the row (109.2 or later) must use the
  corrected snapshot; the diverging-default version would be a second vacuous row.
- **The three-row witness patch** (M-1 corrected snapshot; `inf` + default-0 for M-2;
  `1_000_000/MILLION` no-key Always for M-3) is ~6 lines in `runtime.rs`'s existing
  table, natural for 109.2's test half or any later `runtime.rs` touch.
- **M-5's two glued literals** (`envoy-http2/src/hcm.rs:1925/:2045`) can be hand-fixed
  by any future task that edits that file — rustfmt will not do it.
- **M-4 stands as the LoC-calibration record** for the next PLAN-write: T4-class
  mechanical call-site fan-outs cost ~3× their naive one-line-per-site price.
- The 108.2 banked set (M-2, N-1…N-6) and older families stay banked and unfixed
  (§6.3); 108.2 M-1 stays DISPOSED (rides in 109.2).

---

## §9 — Assessment

109.1 is a disciplined consumer slice that closes its highest-consequence risk loudly.
The two places this surface could fail silently — the reload classifier's abort arm and
an empty snapshot slipping into production — are exactly the two the phase defended
hardest: the classifier landed test-FIRST with the widening after it, this review's M3
mutation proves the test fires on the precise omission 76.2 died from, and the
production snapshot threading has no default() anywhere outside test code. The cascade
is a byte-faithful transcription of a measured 23-cell contract, order-correct where
ordering is load-bearing, with a provably sound prefix probe.

The five Minors share one shape: none is a defect in what the gate DOES — three are
guards witnessed only from their masked side (the table pins every measured cell but
not every guard), and two are ledger sentences that measurement contradicts. The most
valuable single output of this review is the M-1 remedy refutation: the state-4 handoff's
prescribed fix would have landed a second non-discriminating row dressed as a repair.

**Gate (f) is CLOSED. All six §7.5 gates are now GREEN. Sub-phase `109.1` is approved
to land.**

**Next state: §5 state 6 — the close-out** (ROADMAP row `109.1` `planned`→…→`done`;
parent row `109` stays `in-progress` until sibling `109.2` is done), a **separate
session** per §5.1 and ADR-0127 — a reviewer must not close out what it graded. This
review **fixed nothing**, as ADR-0165 requires.
