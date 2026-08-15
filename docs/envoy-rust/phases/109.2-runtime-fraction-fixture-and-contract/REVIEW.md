# Sub-phase 109.2 — differential fixture `0088-runtime-fraction-route-gating`, the `BEHAVIOR_CONTRACT.md` `## Runtime` consumer subsection, the decided-in 108.2-M-1 correction, and the three banked witness rows — CODE REVIEW

**Verdict: APPROVED-WITH-MINORS.**

Per `BOOTSTRAP_PROMPT.md` §7.5, an approved `REVIEW.md` closes gate **(f)** — the only
gate still open. Gates (a)-(e) were run and adjudicated by the §5 state-4 verification
session and are recorded, with actual command outputs, at `PROGRESS.md` `# §5 state-4
verification`. **This review did not re-run them and does not re-adjudicate them**
(§5.1; ADR-0127 — the context that ran the gate must not grade it, and the context that
grades it must not fix it). It re-confirmed CI on the exact tree under review
independently (§0.3), because that is a fact about the commits rather than a re-run of
the gate.

**Zero Issues. Eight Minors, eleven Nits — every one in PROSE, a CITATION, or fixture
COVERAGE. Not one is in the code, the fixture data, or an assertion.** No accept/reject
verdict changes, no wire behaviour moves, no validator is left unwired, no test is
vacuous, no fixture is weakened. Per §6.3 and ADR-0165 **nothing was fixed by this
session**. **No §5.2 re-entry to state 3 is required** — the verdict is an approval and
gate (f) is CLOSED; every Minor and Nit below is BANKED for the state-6 close-out to
carry and for a later slice to weigh.

The twelve findings the state-4 verifier banked (V-1…V-12) were **re-derived from disk,
not accepted** — §8 disposes of each one by one. **Eleven are CONFIRMED, one (V-6) is
PARTLY CONFIRMED, and one of them — V-1 — carries a consequence clause that is itself
arithmetically FALSE and has already been propagated into `STATE.md`'s Standing-traps
line** (M-8 below). That correction is this review's single most useful output.

---

## §0 — How this review was conducted

### §0.1 — Scope

The unit of review is `e458765..3bbf6bc`: five per-task commits `c2b3207` T1 / `6772632`
T2 / `fcad066` T3 / `8644fa4` T4 / `39e9afc` T5, plus the docs-only `3982c89` (CI record),
`9316681` (state-4 verification) and `3bbf6bc` (CI record). Measured
`git diff --numstat e458765 3bbf6bc` — **12 files, +1579 / −25**:

| file | + | − |
|---|---:|---:|
| `docs/envoy-rust/phases/109.2-…/PROGRESS.md` | 779 | 0 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | 122 | 4 |
| `tests/fixtures/0088-…/envoy.yaml` | 126 | 0 |
| `tests/fixtures/0088-…/envoy-rust.yaml` | 126 | 0 |
| `tests/fixtures/0088-…/expectations.yaml` | 124 | 0 |
| `tests/fixtures/0088-…/README.md` | 111 | 0 |
| `docs/envoy-rust/STATE_HISTORY.md` | 84 | 0 |
| `tests/differential/tests/runtime_fraction_route_gating.rs` | 40 | 0 |
| `docs/envoy-rust/STATE.md` | 27 | 16 |
| `crates/envoy-config/src/runtime.rs` | 22 | 0 |
| `crates/envoy-config/src/bootstrap.rs` | 10 | 3 |
| `crates/envoy-admin/src/endpoint.rs` | 8 | 2 |

`git diff e458765 3bbf6bc -- crates/` contains exactly **THREE** hunks: `runtime.rs
@@ -754,6 +754,28 @@` (Task 1, test data inside `#[cfg(test)]`), `endpoint.rs
@@ -3316,8 +3316,14 @@` (Task 4, `///` lines only) and `bootstrap.rs @@ -1496,9 +1496,16 @@`
(Task 4, `///` lines only). **Zero production code changed.** `109.2/SPEC.md` and
`PLAN.md` are byte-identical across the whole range, as D-3.5 requires; `PLAN.md` was
written by `936f1f5` and never amended.

### §0.2 — Method

Five read-only review dimensions were fanned out in parallel — the fixture's
discriminating power; the contract subsection as a canonical record reconciled against
both source matrices; the deviation ledger's completeness across BOTH `PROGRESS.md`
sections; the three witness rows against the guards `109.1`'s REVIEW said were
unwitnessed; and an independent re-derivation pass over V-1…V-12 — each instructed not to
write, not to run `cargo`, and told that the §7.5 gates are already adjudicated. **Every
finding below was RE-VERIFIED ON DISK by the main session**, and every line number in this
document was re-derived at HEAD rather than quoted from a subagent. Three subagent
findings were **downgraded or rejected** on that re-verification and are recorded as such
in §5 — a subagent finding is a claim.

No `cargo` was run by this session at all. Every "would RED" statement below is a
control-flow trace over the source, stated as such.

### §0.3 — CI re-confirmed independently on the exact tree under review

Not inherited from the handoff. HEAD `3bbf6bc2a5a398341957ed134e261ea5502cac9f` is
docs-only on top of `9316681`, whose `crates/`+`tests/` tree is identical to `8644fa4`'s,
so the CI run on HEAD is the run on this exact code tree: run **31851097678**,
`conclusion=success`, jobs enumerated via the jobs API and selected by NAME —
`build + test + lint` id `94926900156` (steps **15**, runner `GitHub Actions 1000005333`)
and `fuzz` id `94926900111` (steps **13**, runner `GitHub Actions 1000005332`). Build-job
log, **409135** bytes (asserted alongside the census per the empty-file-md5 trap):

```
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' <log> \
    | awk '{b++; p+=$4; f+=$6} END{print "binaries="b" passed="p" failed="f}'
binaries=165 passed=2194 failed=0
$ grep -c 'test result: FAILED' <log>
0
```

Two facts the local run could not establish, both measured here:

- **The new fixture genuinely runs and passes in CI** — log line 1402,
  `test runtime_fraction_route_gating ... ok`, its binary at line 1396.
- **The h2spec gate genuinely EXECUTES in CI.** `grep -c 'h2spec not found'` over the
  whole log = **0**, while `test h2spec_pass_rate_gate ... ok` is present. The gate that
  self-skips on a developer host is not skipping in CI (ADR-0163 confirmed by
  measurement, not inheritance).

### §0.4 — The test-count identity closes exactly

| quantity | value | source |
|---|---:|---|
| CI `passed` before the slice | **2193** | the `9331ce3`/`c3e6177`/`3861981` runs, 164 binaries |
| new `#[test]`/`#[tokio::test]` attributes in the diff | **+1** | the fixture entrypoint only; `git show c2b3207 \| grep -c '^+.*#\[\(tokio::\)\?test\]'` = **0**, so Task 1's three rows moved no count |
| CI `passed` at HEAD | **2194** | run 31851097678, **165** binaries |

**2193 + 1 = 2194**, binaries 164 → 165. The PLAN's own §"CI identity prediction"
(`PLAN.md:44`) is met on the nose. The three witness rows are tuples inside an existing
`vec!` in an existing test fn — the mechanical proof that no test function was added.

### §0.5 — Standing censuses re-derived at HEAD

**88** fixture dirs (highest `0088`, so `0089` is next) / **88** differential test files
(`git ls-files 'tests/fixtures/**' | cut -d/ -f3 | sort -u | wc -l` — the naive
`git ls-files 'tests/fixtures/*/'` is a vacuous glob returning a clean-looking ZERO);
**165** test binaries with `passed + failed = 2194`; **134** `ConfigError` variants, the
enum brace-matched to `lib.rs:75-1105`; **5** fuzz targets across FIVE crates; **21**-line
`known-failures.txt` (ONE real entry, untouched); **3**-entry `HEADER_ALLOW_LIST`, no
`location`; **14** crates (no `envoy-runtime` — ADR-0172 D8); **117** phase directories;
`runtime.rs` **910** lines, `bootstrap.rs` **21950**, `BEHAVIOR_CONTRACT.md` **4045** with
**15** `## ` and **24** `### ` (both held constant by the phase); ROADMAP **113 rows /
111 `done` / 1 `in-progress` / 1 `planned`**, the two non-`done` rows enumerated BY ID
(`109` → `in-progress` at row line 190, `109.2` → `planned` at row line 192) rather than
inferred from a count — **this review flips no cell**; ADR head **ADR-0176**, and
`grep -c '^## ADR-0177'` = **0**, so **ADR-0177 stays UNRESERVED** (this review decides
nothing new). Three ROADMAP family headings still carry ZERO rows, by a heading-slice
census over all **11** `### ` headings: 10 / 5 / 3 / 14 / **0** / **0** / 6 / 29 / 6 /
**0** / 13.

---

## §1 — Strengths

1. **The three banked witness rows GENUINELY close the three guards, and the CORRECTED
   M-1 remedy was used rather than the refuted one.** Traced analytically, twice
   independently, against `runtime.rs:167` / `:181` / `:203`:
   - **M-1** (`runtime.rs:761-766`) — snapshot `{".dotted": "1"}`, `rf(100, Hundred,
     Some(""))` → `Always`. Unmutated: the `.filter(|k| !k.is_empty())` at `:167` rejects
     the empty key, the whole block is skipped, the default `100 == Hundred.value()`
     gives `Always`. Mutated (filter deleted): `key = ""`, `prefix = "."`, and
     `entries.range("."..).next()` is `".dotted"`, which `starts_with(".")` → **`Err(
     MapShapedKey)`** at `:175` → RED. This diverges at the PREFIX arm, which is exactly
     why it works where the 109.1-refuted "diverging default" did not: it never needs
     `entries.get("")` to hit.
   - **M-2** (`:767-772`) — `one("inf")`, `rf(0, Hundred, Some("gate.k"))` → `Never`.
     `"inf".parse::<f64>()` is `Ok(INFINITY)` (the pinned toolchain's own `FromStr for
     f64` grammar: `Float ::= Sign? ( 'inf' | 'infinity' | 'nan' | Number )`), so
     unmutated the `is_finite()` conjunct at `:181` fails and the default-0 arm gives
     `Never`; mutated, `INFINITY >= 100.0` returns `Always` → RED. Cross-checked that
     this row is the UNIQUE discriminator: the pre-existing `one("inf")` + default-100
     row is masked, and the `NaN` row is masked in every direction because every NaN
     comparison is false.
   - **M-3** (`:773-778`) — `empty`, `rf(1_000_000, Million, None)` → `Always`. Unmutated
     `1_000_000 == Million.value()` (`bootstrap.rs:1341`) gives `Always`; mutated to
     `numerator == 100` it becomes `Err(NondeterministicDefault)` → RED. No pre-existing
     row reaches the default-Always arm with a non-`Hundred` denominator.

   All three REDs are asserted by the PROGRESS mutation table with a real `test result:`
   line and a non-zero `Compiling envoy-config` count each, and the traces agree with all
   three recorded panics.

2. **The byte-identical YAML pair was ACHIEVED by refuting the SPEC twice, from
   measurement.** `109.2/SPEC.md` §2 D1 prescribed `node:` + `admin` with
   `{{ADMIN_PORT}}`. Both are wrong, and the PLAN-write proved it rather than reasoning
   about it: `{{ADMIN_PORT}}` is driver-gated by `driver_needs_admin_port`, whose
   `matches!` omits `Http1ProbeList`, and `render_yaml` leaves an unmatched token
   untouched — a literal `{{ADMIN_PORT}}` would reach the parser as an address; and
   `node: { id: x, cluster: y }` is boot-fatal upstream because YAML 1.1 booleanizes the
   unquoted `y` into a protobuf string field. Dropping `node:` is precisely what makes
   one file serve both sides. Re-derived here: `cmp` silent, **126** lines each, md5
   `d205936b0390260855f19258dd02f51a`, and a `cmp` sweep over all 88 pairs finds exactly
   **two** byte-identical ones (`0027-xds-file-based-lds` and `0088`) — so the README's
   "the SECOND such pair" is a re-measured per-fixture claim, not an inherited one.

3. **The fixture asserts an ABSOLUTE oracle on BOTH sides, not merely cross-proxy
   equality.** `run_http1_probe_list_arm` (`tests/differential/src/lib.rs:5437-5548`)
   checks, per probe: cross-proxy status equality; `expected_status` against upstream AND
   subject separately; the cross-proxy body rule; `expected_body` byte-exact against
   upstream AND subject separately; then header set-equality modulo the 3-entry allow
   list. A "both sides wrong the same way" regression is caught, and the fixture doubles
   as a characterization of upstream v1.33.0.

4. **`p9` is a genuinely load-bearing cell, and it is discriminating in both directions
   of failure.** A `0/MILLION` default gated by the runtime value `100` separates
   "integer value is the numerator over HUNDRED" from "over the default's denominator".
   Under the wrong reading envoy-rust's own cascade hits `0 < v < 100` →
   `NondeterministicValue` → boot-fatal, so the subject never becomes accept-ready and
   the fixture REDs at startup; if the wrong reading instead reached
   `route_fraction_passes`'s `Err` arm, `default_value.numerator == 0` gives `false` →
   `CATCH` → RED. Both roads RED. It is the only fixture in the corpus using a non-
   `HUNDRED` denominator, and — as `expectations.yaml`'s own comment says — under the
   wrong reading this is a ~10⁻⁴ event per request that **no 0/100 fixture could ever
   catch**.

5. **The 23-cell contract table reconciles EXACTLY against both source matrices, cell by
   cell.** Rows 1-13 ↔ `109/SPEC.md` §1.1 and rows B1-S1 ↔ `109.1/SPEC.md` §1.2: identical
   inputs, identical counts (including cell 5's `GATED 27 / FALLBACK 33 over n=60`, cell
   9's `40/40`, F4's `GATED 1/40`), and a `reading` column that is verbatim source
   phrasing throughout. Zero fabricated cells, zero omissions, no `edge:` row promoted to
   measured, and the positive claim — "These 23 rows are the MEASURED contract and
   nothing else is" — holds exactly (23 `cell …`-labelled rows in `runtime.rs`, counted
   by label). The X-5 instruction to transcribe from the SOURCES rather than from
   `109.2/SPEC.md`'s summary was followed, and it is why the table is clean.

6. **The Task-4 correction is accurate at all four sites, and the near-miss was proven
   byte-identical rather than eyeballed.** `BEHAVIOR_CONTRACT.md:1379` and `:3181-3188`
   now record the ASYMMETRY (envoy-rust 405s non-GET by the 06.1/08 house convention;
   upstream v1.33.0 serves `POST`/`DELETE /runtime` with 200 and the full body);
   `endpoint.rs:3316-3326` correctly re-scopes the test doc to "envoy-rust's OWN
   dispatch" without touching the test body or its assertions;
   `bootstrap.rs:1497-1508` narrows the stale `RuntimeFractionalPercent` claim. Re-run at
   HEAD, `git grep -n -e 'GET-only on BOTH' -e 'GET-only (POST' -e 'allow: GET\`
   bilaterally'` returns **ZERO** hits. The CF-108-2 `/runtime_modify` sentences — a
   different endpoint with the OPPOSITE asymmetry — are undamaged, and their
   byte-identity was proven in Python rather than by reading `+`/`-` lines, which is the
   right instrument for that class.

7. **All three recorded deviations are real, correctly diagnosed, and each substitutes a
   STRICTLY STRONGER check.** DEVIATION 1 (the mutation worktree seeded rather than
   created at bare `HEAD`) — the premise holds, Step 4 precedes Step 5's commit so `HEAD`
   was `e458765` and the rows under test would have been absent; the substitute (copy in,
   assert md5 equality with the main tree) is stronger, and the landed blob's md5
   `61b18068d2af02171a12a3a35a028313` reproduces. DEVIATION 2 (`git checkout --` is a
   NO-OP on a still-untracked fixture, and `git diff --stat` is empty for the wrong
   reason) — reproduced independently: applying V1's mutation to `envoy.yaml` yields md5
   `ddcc8e79f8ec612b8a2227960c82167c`, bit-for-bit the value PROGRESS records, and the
   md5-equality revert adjudication does not depend on tracked state. DEVIATION 3 (the
   `runtime_modify` COUNT check is the wrong instrument, and the PLAN is self-inconsistent
   because its own Step 1 requires adding the control citation) — the four current
   mentions are exactly `:1379`, `:3186`, `:3210`, `:3351` as enumerated.

8. **The vacuity mutations are the right ones, and the GREEN one was recorded as a
   finding rather than skipped.** V1 (`override_layer`'s `gate.layered: 0` → `100`) kills
   first-layer-wins, override-ignored and max-wins folds; V2b (p9's key → an absent key)
   proves p9's witness comes from the CONSULTED value and not the default. V2a came back
   GREEN, exactly as the PLAN predicted, and PROGRESS says so and interprets it — a GREEN
   mutation is a finding, not a non-event.

9. **The suspiciously-fast green was audited, and the audit found the INSTRUMENT at
   fault.** A ~1 s cross-proxy green invites the silent-skip suspicion; the first
   `docker ps` poll reported zero containers because `--format '{{.ImageID}}'` names a
   field that does not exist, so all 40 lines were template errors and the emptiness read
   as a clean census. Re-run with a valid format, the upstream container is plainly there
   (7 sightings, port-mapped). This is the "a probe that fails to execute returns a
   believable zero" class caught live, and recording it is worth more than the green it
   was checking.

10. **The state-4 section is provably APPEND-ONLY.** `git show 9316681 --numstat` on
    `PROGRESS.md` is `329 0` — zero deletions — and
    `diff <(git show 39e9afc:PROGRESS.md) <(git show 3bbf6bc:PROGRESS.md | head -450)`
    produces no output (both md5 `405ec3c374060ae61d9a8d396605698c`). Not one state-3 line
    was edited by the session that graded it.

11. **Every `Do NOT touch` invariant holds.** `tests/differential/src/lib.rs` unmodified
    (this fixture needed zero harness change, as X-1 predicted); no existing fixture
    edited; `ci.yml`, `ENVOY_TARGET.md`, `rust-toolchain.toml`, `ROADMAP.md` and
    `DECISIONS.md` untouched; `git diff --name-only e458765 3bbf6bc -- docs/envoy-rust/
    phases/` returns only `109.2/PROGRESS.md`, so no landed artifact was edited. The
    `runtime.rs` insertion sits inside `#[cfg(test)]`.

---

## §2 — Issues (Must Fix)

**None.**

No finding changes an accept/reject verdict, alters wire behaviour, introduces a
reachable panic or abort, weakens or vacates a fixture, leaves a validator unwired, or
touches production code. **No §5.2 re-entry to state 3 is required.**

---

## §3 — Minor

### M-1 — the `RuntimeFractionalPercent` doc's cascade catch-all is FALSE, and it is the one NEW inaccurate claim this phase introduced

`crates/envoy-config/src/bootstrap.rs:1506-1508` (landed by Task 4, step 4 — the step the
PLAN itself flagged as an ADDITION to SPEC D4):

> … the ROUTE consumer HONORS `runtime_key` … under the deterministic 109.1 cascade
> (`v == 0` → never, `v >= 100` → always, `0 < v < 100` → boot-fatal per CF-109-1,
> **everything else → `default_value`**).

Measured against `route_fraction_gate` (`runtime.rs:167-210`), "everything else" is not
`default_value`. Two arms are missing, and both are boot-fatal:

- a consulted key with ANY `K.`-prefixed snapshot entry → `Err(MapShapedKey)` (`:175`),
  boot-fatal per **CF-109-2**;
- a `default_value` that is neither `0` nor `== denominator.value()` →
  `Err(NondeterministicDefault)` (`:206`), boot-fatal.

The sentence therefore states, on the type's own doc, exactly the silent-fallback posture
CF-109-2 exists to prevent — an implementer arriving at `RuntimeFractionalPercent` from
`RouteMatch.runtime_fraction` reads that a map-shaped consulted key quietly falls back to
the default. **Mitigation, and why this is Minor rather than an Issue:** the sentence
names `RuntimeSnapshot::route_fraction_gate`, whose own doc comment
(`runtime.rs:148-162`) enumerates all three error classes correctly and is the
authoritative record; and the function's signature is
`Result<FractionGate, FractionGateError>`, so the type system refutes the catch-all at the
first call site. Zero behavioural effect. Cheapest fix: replace "everything else →
`default_value`" with "an absent, unparseable, non-finite or negative value →
`default_value`; a map-shaped consulted key or a non-deterministic `default_value` →
boot-fatal" — ~2 lines, in any future task that legitimately edits `bootstrap.rs`.

### M-2 — CF-109-2's CONSERVATIVE over-reject leg is missing from the canonical record

`docs/envoy-rust/BEHAVIOR_CONTRACT.md:3293-3300` describes CF-109-2 entirely in terms of
map-shaped values: "a map-shaped value at (or beside) a CONSULTED key is boot-fatal …
because … a plain lookup would silently fall back to the default where upstream HONOURS
the map for routing (cells 7/8)".

The implemented rule is broader, and `109.1/SPEC.md` D3 (`:182-196`) records the breadth
explicitly:

> `K` scalar in a later layer + map in an earlier one ALSO leaves `K.`-prefixed entries →
> conservatively caught (upstream last-wins would honor the scalar — **a recorded,
> slightly-conservative reject-direction divergence inside CF-109-2**); a literal dotted
> SIBLING key (`K.foo` beside scalar `K`) → conservatively caught, same recording.

The landed unit table pins the second case (`runtime.rs:835`: `"edge: scalar K beside
literal dotted sibling K.foo -> conservatively fatal (recorded)"`, over a snapshot with
`gate.k: 100` and `gate.k.foo: 1` and **no map anywhere**). So a plain scalar configuration
with a dotted sibling key is boot-fatal here while upstream serves the route — a
reject-direction divergence with no map in it — and `BEHAVIOR_CONTRACT.md`, the file whose
job is to record exactly this class, does not carry it. A zero-context session reading the
contract learns that map-shaped values reject; it does not learn that `gate.k: 100` beside
`gate.k.foo: 1` refuses to boot. ~2 sentences inside the existing CF-109-2 bullet.

### M-3 — the fixture witnesses the unparseable→default reading in ONE direction, twelve lines after the contract says one direction is not enough

`tests/fixtures/0088-…/expectations.yaml` probe `p7` (`/p-unparseable`, `default_value
100/HUNDRED`, `gate.abc = abc`, expecting `P7-GATED`) is cell 10 only. Cell **11**
(`0/HUNDRED` + `"abc"` → FALLBACK) is measured upstream at 30/30 (`109/SPEC.md:49`, whose
own annotation reads "**confirms default-used in BOTH directions**"), is deterministic,
boots clean, and is absent from the fixture.

Concretely, p7 excludes "unparseable → 0" (that would answer `CATCH`) but **cannot**
exclude "unparseable → always pass, default ignored" — with a 100/HUNDRED default the two
readings are the same observable body, and p7's observable is byte-identical to p1's. That
is precisely the argument the contract makes at `:3279-3281`, about this very reading:

> an unparseable value falls back to `default_value` in **BOTH** directions (cells 10/11 —
> a single-direction probe is equally consistent with "unparseable → 0").

The fixture it points at is a single-direction probe. Contrast p1/p2 and p3/p4, which each
land both directions — the fixture's own convention, not followed here. **Mitigation:**
cell 11 IS pinned in-process (`runtime.rs:646-651`), so the behaviour is not unpinned,
only undifferentiated. Cheapest fix: an 11th route + probe, `default_value {0, HUNDRED}`,
`runtime_key: gate.abc2 = xyz`, expecting `CATCH` — ~11 lines of YAML plus one probe.

### M-4 — `109.2/SPEC.md` §1 states that BOOL values are boot-fatal; measured FALSE — and five fixture-able deterministic cells were excluded on that premise

`docs/envoy-rust/phases/109.2-runtime-fraction-fixture-and-contract/SPEC.md:46-48`:

> All values integer or string — map-shaped, fractional, **bool** and non-integral-float
> values are boot-fatal after 109.1 (CF-109-1/2) and are witnessed by 109.1's in-process
> reject tests, NOT here.

Bools are boot-fatal under neither carry-forward. `RuntimeValue::Bool` stringifies to
`"true"`/`"false"` (`bootstrap.rs:1000`), which fails the `f64` parse at `runtime.rs:180`
and falls through to the default — no error. The landed unit table asserts exactly that:
`runtime.rs:665-682` pins B1/B2/B3 as `Ok(Always)`/`Ok(Never)`/`Ok(Always)`, and the
contract's own rows `:3239-3241` carry them as GATED/FALLBACK/GATED 40/40. CF-109-1 covers
only `0 < v < 100` (`runtime.rs:189-193`).

Consequence, and why this is more than a wording slip: cells **B1, B2, B3, N1, N2** (and
**11**, per M-3) are deterministic, boot-clean, upstream-measured and fixture-able, and
all were left out — so the cascade's `v < 0` fall-through arm (`runtime.rs:195`) has
**ZERO differential coverage**. An implementation spelling that arm `v <= 0 → Never`
instead of falling through to the default would be caught by no fixture in the corpus.
(It is caught in-process by the N1/N2 rows.) The fixture's own `README.md:44-49` does NOT
repeat the bool error and is accurate as written — the defect is in the SPEC, a landed
artifact, so the record is this finding.

### M-5 — "method-restricts NO read-only admin endpoint" is a universal generalised from two endpoints, and the landed text dropped the datum that supported it

Landed at two sites, `BEHAVIOR_CONTRACT.md:1379` and `:3184-3186`, both under a section
preamble (`:3164-3166`) promising "Everything here is MEASURED … unless marked otherwise",
and both carrying the label `MEASURED`. The underlying measurement (108.2 REVIEW M-1) is
four probes: `POST /runtime` → 200, `DELETE /runtime` → 200, `POST /config_dump` → 200,
and the discriminating control `GET /runtime_modify` → 405. That is two of the eleven
read-only admin endpoints the contract's own table at `:1356-1379` enumerates — and the
`POST /config_dump` datum, the ONLY evidence for the "tree-wide" / "NO read-only endpoint"
generalisation, is the one item both landed sentences omit. `109.2/SPEC.md:82-88` carried
it; it was not transcribed. The correction itself is right and valuable; the universal
now stands beside same-endpoint evidence only. Fix: restore the `POST /config_dump` datum,
or narrow the claim to the endpoints actually probed.

### M-6 — the state-3 EXIT gate is recorded on exit codes alone, and substitutes earlier tasks' counts for its own

`PLAN.md:19` (Global Constraint) and `:628` (Task 5 Step 1) both require it: "Gate the
build on a non-zero `Compiling` count and clippy on a non-zero `Checking` count … exit 0
alone is not evidence." `PROGRESS.md:326-327` records `exit 0` for both and nothing else,
then `:335-338` substitutes *earlier tasks'* counts (13/13 at T1 and T4, 1/1 at T2).

That substitution does not carry: an earlier task's `Compiling` count says nothing about
whether THIS run compiled anything, and Task 4 had just built the tree, which makes this
run precisely the fully-cached case the constraint exists to catch. Neither evidenced nor
waived. **Mitigation:** the substitution is stated openly rather than hidden, this is the
state-3 exit bar and not the §7.5 adjudication, and the state-4 session defeated the
cached-no-op causally (an mtime-only `touch` of `crates/envoy-config/src/lib.rs` forcing a
real 14-crate dirty set in both caches). Same class as V-12, on the more load-bearing
task.

### M-7 — a SECOND stale-at-publication figure, unbanked: "lists ten files"

`PROGRESS.md:383-384`:

> **Neither tail member sits on this phase's surface.** `git diff --name-only e458765
> HEAD` lists ten files and NONE is `crates/envoy-bin/tests/xds_eds_hot_reload.rs` or
> anything in `envoy-http2`.

Measured: `git diff --name-only e458765 8644fa4 | wc -l` = **10**;
`… e458765 39e9afc | wc -l` = **12** — and `39e9afc` is the commit that publishes the
sentence. Identical class to V-1 (a figure taken at one SHA and re-attributed to a later
one), on the same page, and NOT among the twelve banked. The *argument* survives intact —
the two extra files are `STATE.md` and `STATE_HISTORY.md`, neither a test file nor in
`envoy-http2` — so only the count is wrong, not the conclusion.

### M-8 — V-1's own CONSEQUENCE clause is arithmetically FALSE, and it has already been propagated into `STATE.md`'s Standing-traps line

This is the most important correction in this review, because the defective sentence is no
longer confined to a landed `PROGRESS.md`: it is live in the state ledger a future session
inherits.

V-1 (`PROGRESS.md:651-653`) and, verbatim, `STATE.md`'s `**Standing traps**` line say:

> The `excluding docs/` figure **562** IS right and is stable across all three …, so the
> `+33%` verdict survives only for the nodocs comparison — whole-tree it is ≈+58%.

Measured, against the PLAN's own projection of **≈745** (`PLAN.md:56`, which explicitly
INCLUDES docs — T3 ≈80, T4 ≈15, and ≈120 of `PROGRESS.md` appends at `:50/:53/:55`):

| comparator | net LoC | vs 745 |
|---|---:|---:|
| whole-tree at `8644fa4` (where the +33% was taken) | 992 | **+33%** |
| whole-tree at `39e9afc` (the publishing commit) | 1175 | **+58%** |
| whole-tree at HEAD `3bbf6bc` | 1554 | +109% |
| excluding `docs/` | 562 | **−25%** |
| like-for-like: whole-tree at `39e9afc` less the `STATE.md`/`STATE_HISTORY.md` protocol edits the PLAN never budgeted | 1130 | **+52%** |

So **+33% IS the whole-tree comparison** — it is 992/745 — and the nodocs figure is 25%
**below** the projection, not +33% above it. V-1 inverted which comparator supports which
number. The honest statement is: the citation was stale (V-1's core claim, CONFIRMED),
and the overrun at the publishing commit is **+58%** whole-tree, **+52%** like-for-like
against a projection that budgeted docs but not the state ledger, and the "excluding
`docs/`" figure does not compare to ≈745 at all. The `calibrate-loc-estimate-against-
landed-phases` record therefore reads +52%…+58%, in line with 76.1's +50% and 109.1's
+46% — not the +33% that has been carried forward four times.

**This session does not fix it** (ADR-0165). What it does do, as its own protocol duty, is
decline to restate the false clause: the superseded traps-line text is archived verbatim
to `STATE_HISTORY.md` per ADR-0035, and the new traps line carries the corrected
arithmetic.

---

## §4 — Nit

**N-1** — **probe `p10` cannot report a failure.** Measured: no gated route's prefix is a
prefix of `/p-catch`, so p10 exercises exactly route 10 (`envoy.yaml:101-104`) — the same
route p2 already reaches and asserts, and the driver `bail!`s at the FIRST failing probe,
so p2 fails first in every world where p10 would. Harmless as documentation of intent, but
it is not a test and should not be counted toward "ten cells witnessed" (the contract's
`:3311-3312` correctly calls it "an ungated control", which is right).

**N-2** — **the distinct-`path:` citation is a category error.** `README.md:13` and
`expectations.yaml:10` cite `BEHAVIOR_CONTRACT.md` "Why every probe carries a DISTINCT
`path:`". Re-derived at `:2926-2940`: that rule is explicitly scoped to the
`http1_access_log_byte_exact` driver — "there is no per-probe assertion and no
expected-line field" — and `:2940` says it "binds EVERY `http1_access_log_byte_exact`
fixture". `Http1ProbeList` has per-probe assertions and names the failing probe in every
`bail!`. Distinct paths ARE load-bearing here, for a better reason: each gated route must
be independently reachable. The inherited citation would mislead a later author into
thinking the rule is generic.

**N-3** — **CF-109-3's unblock condition has no source and is narrower than necessary.**
`BEHAVIOR_CONTRACT.md:3304-3305`: "*Unblocked by* unifying the two matchers." Neither
`109/SPEC.md` §6 (`:313-315`) nor ADR-0175 nor ADR-0176 states any unblock for CF-109-3.
Teaching `route_match_matches` to consult the snapshot would unblock it without unifying
anything.

**N-4** — **"has no PRNG anywhere in the tree" landed unqualified.**
`BEHAVIOR_CONTRACT.md:3290-3291`. The source qualifies it (`109/SPEC.md:80-82`): the only
`rand` hit is a **test-only** `aws_lc_rs::rand` and `fastrand` is transitive-dev-only via
`tempfile`. Re-measured: `git grep -n 'rand::' -- crates/` returns exactly one hit,
`crates/envoy-jwt/src/test_support.rs:55`. True in substance, unqualified in the record.

**N-5** — **Task 2 Step 8 (the README) is wholly unevidenced in the ledger.**
`PLAN.md:524` prescribes a nine-topic README; 111 lines landed; `grep -n README
PROGRESS.md` returns a single hit, `:409`, a numstat row in Task 5's LoC table. Reading
the landed README, **all nine topics are present** — the defect is ledger-only, and the
artifact is one of the strongest in the phase.

**N-6** — **no per-task commit SHA appears in the state-3 session summary.**
`PLAN.md:640` requires "per-task commits" in it. Full SHA-token census of
`PROGRESS.md` lines 1-452: `e458765`, `9331ce3`, `c3e6177`, `3861981` — none of them a
task commit; the word "commit" does not occur in the Task-5 section. Recoverable from
`git log` (every message is prefixed `phase 109.2 task N:`), so this costs a reader one
command.

**N-7** — **the fixture README's "Cold ≈ 8 s" is an inherited estimate.**
`README.md:96` carries the PLAN's pre-flight figure (`PLAN.md:511`); this session's own
measurement is **1.28 s** cold (`PROGRESS.md:130`), and state 4 measured 1.07-1.11 s. An
inherited number in a landed artifact, on a phase whose own doctrine is "re-derive, never
inherit".

**N-8** — **the test fn's doc comment now under-describes its own table.**
`runtime.rs:585-588` describes the table as "EVERY measured cell of §1.1 … and §1.2 …,
plus the §1.3/§7 derived edges. One measured cell = one table row." The three new rows are
neither measured cells nor §1.3/§7 derived edges — they are a third, unnamed category
(implementation-guard witnesses). The inline block comment at `:757-760` covers them
locally; the fn-level doc a reader lands on first does not.

**N-9** — **the three new rows do not follow the table's derived-row labelling
convention** (the code-side twin of V-8). Measured across the 28 `ok_cells` rows: 17
prefixed `cell <id>:` (upstream-measured), 8 prefixed `edge:` (derived), and 3 prefixed
`M-1:`/`M-2:`/`M-3:` (neither). File-wide: 23 `cell …` rows, **11** `edge:` rows. A future
session censusing derived rows by `edge:` undercounts by 3. Separately, `M-1`/`M-2`/`M-3`
are round-scoped review IDs used as durable labels — this review has its own `M-1`, and a
failing row's panic prints only `M-1: …` without the `109.1` anchor that the block comment
supplies.

**N-10** — **the layered-precedence witness is one-directional.** p8 pins base `100` /
override `0` → Never, which kills first-layer-wins, override-ignored and max-wins folds.
A min-wins or "any-layer-zero-is-sticky" fold would also produce `0` and pass. **Largely
mitigated:** the merge is type-agnostic and string-based, and fixture `0087` already pins
it differentially in the other direction at the observer (`shared.key`: base `from_base`,
override `from_override` → `final_value: "from_override"`, plus `empty.in.override`
pinning last-NON-EMPTY-wins). The mirror numeric key is also not in the 23-cell matrix, so
adding it would require a fresh upstream measurement, not just a fixture edit.

**N-11** — **p5 is discriminating, but not for the rule it names.** It catches an
implementation that rejects or clamps `numerator > denominator`, treats `200` as invalid,
or classifies it nondeterministic. It does **not** pin the `>= 100` threshold itself:
because every `0 < v < 100` value is boot-fatal under CF-109-1, an implementation written
`v > 0.0 → Always` passes all ten probes. The threshold is inherently unwitnessable in a
differential fixture while CF-109-1 stands — worth saying rather than claiming `v >= 100`
is differentially pinned. (`runtime.rs`'s cells 4/9/12/F2 pin it in-process, and the 109.1
review's M1 mutation proved they RED on the boundary.)

---

## §5 — Severity dissent and subagent findings REJECTED on re-verification

Recorded rather than silently resolved, because a review dimension's claim is a claim.

1. **REJECTED (downgraded to a note): "only one of the two `route_matches` call sites is
   differentially witnessed."** One dimension graded Minor that `resolve_route_in` is
   unwitnessed because a `direct_response` body is produced by `build_response_in` only.
   Re-verified: both callers invoke the SAME function — `hcm.rs:2039`
   (`.position(|r| route_matches(…))`) and `:2106` (`.find(|r| route_matches(…))`) — and
   the gate lives inside `route_matches` at `:2205-2209`. A divergence between the two
   sites is not reachable without an edit that duplicates the gate, which is exactly the
   seam the 109.1 review certified as correct. Not a finding.

2. **DOWNGRADED: the layered-precedence gap** (N-10 above) was graded Minor by one
   dimension. Carried as a Nit: the merge is one type-agnostic code path already pinned
   differentially by `0087`, and the mirror cell does not exist in the measured matrix.

3. **DOWNGRADED: V-3** (the deviation ledger's completeness claim) was banked Minor and
   one dimension argued for Nit, on the ground that the denial sentence is immediately
   followed by an enumerated scope-creep list (`lib.rs`, fixtures, `HEADER_ALLOW_LIST`,
   `known-failures.txt`, `ci.yml`, ROADMAP, ADR, REVIEW) and reads as scoped to those
   classes, while the commit subject announces the state advance. **Minor CARRIED** — see
   §8. The sentence as written is unqualified ("Nothing outside the PLAN's named files was
   touched"), and the standing discipline is that the ledger is provably complete.

4. **V-5 / V-6 / V-10 RE-FRAMED, not rejected** — the defect is real but the CHARGE was
   misplaced. See §8; this materially changes where a future fix belongs.

---

## §6 — Deliberate decisions verified

1. **Placing the fixture's admin block at a literal `port_value: 0` and omitting `node:`**
   — both measured, both recorded in the README with the upstream error text quoted, and
   together they are what makes the two YAMLs byte-identical. The SPEC was refuted by
   running it; that is the right order.
2. **Excluding the `edge:` rows from the contract's 23** — correct, stated in the text,
   and the reason given ("claiming them as measured would be a doc claim inherited as a
   census") is the right one. V-8/N-9 are about the completeness of the exclusion RULE,
   never about the exclusion.
3. **Declining 109.1 M-5** (the two glued `envoy-http2/src/hcm.rs` literals) — honest and
   verified: `git diff --name-only e458765 3bbf6bc -- crates/envoy-http2/` is **0** files,
   and both literals are still on disk at `:1925` and `:2045`, glued at 25-space
   indentation exactly as banked. The bank's own condition is "hand-fixable by any future
   task that edits that file", and `109.2` edits no file in that crate.
4. **No ADR fired** — correct. §6.1 does not re-fire (the split already happened at
   ADR-0176), no mid-execution trigger fired, and no ambiguity was resolved that D-3.5
   would require recording. **ADR-0177 stays UNRESERVED.**
5. **The fixture is CLUSTER-FREE and backend-free** — no `{{BACKEND_IP}}`, so it spawns no
   backend, is outside the `192.168.65.2` host-RED class, and is fully verifiable on a
   developer host. Verified: no such marker in either YAML.
6. **`route_fraction_passes`'s `Err` arm still does not panic** — untouched by this slice,
   and the 76.2 I-1 reasoning stands.

---

## §7 — Status of already-banked findings — read BEFORE grading, NOT re-issued

`109.1/REVIEW.md` (APPROVED; M-1…M-5, N-1…N-6 banked) was read in full before any grading,
along with its §7 and §8. `108.2/REVIEW.md`'s M-1 disposition and the 108.1 / 76.1 / 76.2
families were checked as classes. **No banked item is re-issued here.**

| banked item | origin | status after `109.2` |
|---|---|---|
| M-1 remedy CORRECTION (a discriminating empty-key pin needs a `""` or `.`-prefixed snapshot entry, NOT a diverging default) | `109.1/REVIEW.md` §8 | **CONSUMED CORRECTLY** — Task 1 used the `.`-prefixed snapshot; the refuted diverging-default version was not landed. Verified by trace (§1.1). |
| the three-row witness patch (M-1/M-2/M-3) | `109.1/REVIEW.md` §3 + §8 | **CONSUMED AND CLOSED** — all three guards now genuinely RED on their mutation. |
| M-4, the LoC-calibration record | `109.1/REVIEW.md` §3 | **CONSUMED AS INPUT** to the §6.1 verdict — and now **re-measured and corrected** by M-8: the true overrun is +52%…+58%, not +33%. |
| M-5, the two glued `envoy-http2` literals | `109.1/REVIEW.md` §3 + §8 | **DECLINED, honestly** — verified unfixed and unreachable from this slice (§6.3). Stays banked. |
| `109.1` N-1…N-6 | `109.1/REVIEW.md` §4 | **STAY BANKED, untouched.** None recurs. |
| 108.2 M-1, the measured-false bilateral-405 claim | `108.2/REVIEW.md`, decided-IN by ADR-0176 D5 | **CONSUMED — Task 4**, accurately at all four sites, with M-5 above as the one residue. |
| 108.2 M-2 + N-1…N-6; the 108.1 / 76.1 / 76.2 families | various | **STAY BANKED, UNFIXED** (§6.3; ADR-0165). |

Class check on the recurring failure modes:

| banked class | result on `109.2` |
|---|---|
| A test whose claimed discrimination power is false (108.2 M-2; 109.1 M-1/M-2/M-3) | **Does not recur in the unit table** — the three new rows are the fix and they discriminate. **Recurs once at fixture level** (M-3, p7's single direction) and once as a probe that cannot fail (N-1). |
| A doc claim that is an inherited census | **Recurs** — M-5, N-4, N-7, V-4, V-7. |
| A widened returnable error set landing in a caller's `unreachable!()` (76.2 I-1) | **Does not recur** — no error set widened; `route_fraction_passes` untouched. |
| A PLAN literal that fails its own gate | **Does not recur** — every YAML and Rust literal was pre-flighted and extracted programmatically from `PLAN.md`'s fenced blocks rather than retyped, which is why the transcription is byte-exact. |
| A warm/reload path skipping the new validators | **Not applicable** — this slice adds no validator. |

**Carry-forwards, status at this review:** CF-109-1 (WIDENED), CF-109-2, CF-109-3 remain
**OPEN** — this slice lands no honouring side. CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6 pass
through untouched. **No carry-forward is consumed by this slice.**

---

## §8 — Disposition of the twelve state-4 banked findings (V-1…V-12)

Every one was re-derived from disk by this session. Disposition is one of **SCHEDULED**
(named for a future slice), **DECLINED** (with the reason), or **CORRECTED** (the finding
itself is wrong or misframed and the correction is the disposition). **None sends the
phase back to §5 state 3.**

| # | verdict on re-derivation | disposition |
|---|---|---|
| **V-1** — the net-LoC citation is self-falsifying at the commit that carries it | **CONFIRMED (core), CONSEQUENCE CLAUSE REFUTED.** `git diff --numstat e458765 <sha>`: `8644fa4` +1001/−9 = **992**; `39e9afc` +1200/−25 = **1175**; `3982c89` **1177**; `9316681` **1552**; HEAD **1554**; nodocs **562** at all five. `git show <sha>:STATE.md \| grep -c '+1001'` = 0 / **3** / **3** / 1 / 1 — so the sentence was published at **TWO** commits, not one, and the two hits at HEAD are the trap text quoting itself. | **CORRECTED and SCHEDULED.** The stale-citation core is confirmed and stands as the record. Its arithmetic consequence is false and is corrected in full at **M-8**; the corrected figures (+58% whole-tree at the publishing commit, +52% like-for-like, −25% nodocs) enter `STATE.md` with this session's own traps line, and the false clause is archived verbatim rather than restated. |
| **V-2** — a PLAN byte-identity invariant broken and unrecorded | **CONFIRMED, both halves.** `git show fcad066:…` vs `git show 8644fa4:…`: the needle ``200 `application/json`; body is exactly two\ntop-level keys:`` occurs **1×** old, **0×** new; the reflowed single-line form 0× old, **1×** new. The table half **HELD** — byte-identical. `PLAN.md:586` carries the requirement with no line drift. | **SCHEDULED as a doctrine record, no code action.** The edit is editorially correct (removing "on both sides" left "200 …" subject-less); the defect is the missing DEVIATION-4 label. The landed `PROGRESS.md` is uneditable (D-3.5), so this finding IS the record. Standing lesson for the next PLAN-write: a byte-identity invariant on a clause that a sibling step rewrites is self-conflicting and should be stated as "the table" alone. |
| **V-3** — the ledger's completeness claim falsified by its own commit | **CONFIRMED.** `git show --numstat 39e9afc` → `19 16 STATE.md`, `42 0 STATE_HISTORY.md`; `grep -c 'STATE' PLAN.md` = **0**, and case-insensitively **0** for `state.md`/`state_history`; none of the PLAN's `git add` lines (`:160, :531, :567, :608, :645`) names either file. | **DECLINED as a change, CARRIED as a record. Minor grade carried over a Nit dissent (§5.3).** The advance is BOOTSTRAP §5 session protocol, outside the PLAN's authority, announced in the commit subject; an independent whole-diff audit finds **no fifth unnamed file** — every other touched path is PLAN-named. The remedy belongs in the PLAN template ("the state-advance edits are protocol and are exempt"), not in this phase. |
| **V-4** — the contract asserts unmeasured upstream behaviour for CF-109-3 | **CONFIRMED.** `:3284-3285` "Each is boot-fatal here **where upstream accepts**", universally quantified. `grep -n jwt` over both source matrices returns only design prose — **no probe cell**; `109/SPEC.md` §8 (NOT MEASURED) does not list it either. | **SCHEDULED** for the next slice touching the `## Runtime` section: mark the CF-109-3 leg as upstream-asserted-not-measured, or measure it. Note the claim is **INHERITED** — `109/SPEC.md:313-314` already asserts "upstream honors it there" with no probe. Fixing only the contract leaves the SPEC saying it. |
| **V-5** — cascade step 2 folds unmeasured classes into a "one row per measured cell" enumeration | **CONFIRMED, but the CHARGE is misplaced.** `:3266-3267` is a near-VERBATIM transcription of `109.1/SPEC.md:93-94`, the source X-5 required Task 3 to transcribe from. The genuine 109.2-introduced defect is narrower: the transcription dropped the adjacent NOT-MEASURED disclaimer at `109.1/SPEC.md:102-105` (`"1e6"`, `"NaN"`/`"inf"`, `"-0.0"`). | **RE-FRAMED and SCHEDULED.** Regraded **Nit**. Action for a future slice: restore the dropped NOT-MEASURED sentence, one line. A reviewer acting on V-5 as written would edit the contract and leave the SPEC unchanged. |
| **V-6** — the CF-109-1 bullet contradicts its own F3/S1 rows | **PARTLY CONFIRMED, and UNDER-SCOPED.** Rows F3 (`:3244`) and S1 (`:3248`) record FALLBACK 40/40, F3 with the explicit hedge "0.5% sampling and truncate-to-0 are indistinguishable at n=40"; sampling is measured only at cell 5 (27/33, n=60) and F4 (1/40). But "contradicts" overstates — F3/S1 are the same parse class as the measured-sampling F4, so the bullet is a derivation stated flatly, not a false statement. And the identical claim also sits at **`:3263-3264`** (cascade step 1), inherited verbatim from `109.1/SPEC.md:90-91`. **Two sites, not one.** | **RE-FRAMED and SCHEDULED.** Regraded **Nit**. Any future edit must hit BOTH sites and should hedge as "the same parse class as the measured F4" rather than asserting per-cell sampling. |
| **V-7** — the probe-count preamble is wrong for cell 5 | **CONFIRMED, both halves.** `:3219-3221` says "30 probes each"; `109/SPEC.md:43` and the contract's own `:3230` give cell 5 as **n=60**, and `109/SPEC.md:47` gives cell 9 as **40/40 at the pick** — so its "re-run" attribution has no 30-probe original. `PLAN.md:42` had it right ("30-40 probes each"); the transcription narrowed it to a false universal. | **SCHEDULED**, one-word fix ("30-40 probes each"), for any slice touching the section. |
| **V-8** — the "`edge:`-only" characterisation is incomplete about rows THIS phase added | **CONFIRMED.** Measured in `runtime.rs`: **11** `edge:` rows and **3** unlabelled `M-1`/`M-2`/`M-3` rows (`git show c2b3207 \| grep -c '^+.*"M-'` = 3), all equally upstream-unmeasured. The contract's POSITIVE claim is exact — 23 `cell …` rows, no more. | **SCHEDULED** together with **N-9** (the code-side twin): relabel the three rows `edge: M-1 …` when a future slice next edits `runtime.rs`, which fixes both halves in one edit. |
| **V-9** — the fixture-partition paragraph reads exhaustive but covers 15 of 23 | **CONFIRMED, both halves.** `:3307-3315` (V-9's `:3311-3316` drifts by one — `:3316` is blank): 9 pinned + 4 nondeterministic + 2 reject = **15**; unaccounted and all deterministic/boot-clean: **11, B1, B2, B3, F1, F2, N1, N2** = 8. 15 + 8 = 23. "the jwt surface" is indeed grouped among "cells" though no jwt cell exists. | **SCHEDULED**, and **WIDENED by M-3 and M-4**: the missing eight are not merely unaccounted in prose, six of them are FIXTURE-ABLE and one of the gaps (cell 11) leaves the contract's own load-bearing both-directions reading differentially unwitnessed. The prose fix and the fixture fix are the same finding seen from two sides. |
| **V-10** — "upstream also accepts `>`" has no cell | **CONFIRMED.** Enumerated every probe: numerators ∈ {0, 100}, denominators ∈ {HUNDRED, MILLION} — **no cell sets a `default_value` numerator greater than its denominator**. Cell 12 probes a runtime *value* of 200, a different quantity. | **RE-FRAMED and DECLINED as a 109.2 defect.** The clause is verbatim-inherited from `109.1/SPEC.md:98-99` → parent D2(a) (`109/SPEC.md:194-196`) → the CSRF/fault house rule, and Task 3's mandate was to transcribe. Stays banked against whichever slice re-measures `selects_deterministic` upstream. |
| **V-11** — CF-109-1's unblock condition is narrower than the ledger's | **CONFIRMED, and slightly understated.** `:3292` gives "a phase that lands per-request sampling"; `109/SPEC.md:307-309` gives "a PRNG ADR + contract-relaxation ADR (shared with the non-deterministic-LB candidate)" and `DECISIONS.md:2447` (ADR-0175 D5) adds the "§7.2" qualifier. The contract drops **both** ADR gates, not just the second. | **SCHEDULED**, one clause, alongside **N-3** (CF-109-3's unblock, which has no source at all). |
| **V-12** — no task-boundary gate recorded for Task 3 | **CONFIRMED.** `PLAN.md:19` requires build/clippy/`fmt --check` at every task boundary. `PROGRESS.md` carries `### Task 1 gate` (`:93`), `### Task 2 gate` (`:183`) and Task 4's inline gate (`:282-284`); the Task 3 section (`:197-244`) records only its structural verification and a `113 0` numstat. | **DECLINED as a change, CARRIED as a record**, and **WIDENED by M-6**: Task 5's gate has the more serious version of the same gap (exit codes only, earlier tasks' counts substituted). Task 3 is docs-only and Task 5's sweep covers the tree afterwards, so nothing is unverified in substance. |

**Two census recipes the state-4 section corrected forward — both re-derived TRUE at
HEAD** and restated here so they do not lapse: `grep -c '^## ADR-'` on `DECISIONS.md`
returns **173** while `grep -c '^## ADR-[0-9]\{4\}'` returns **172**, the extra being the
schema template `## ADR-NNNN: <title>` at `DECISIONS.md:10` (numbering is sparse — do NOT
reconcile 172 against a head of 0176); and the bare string `ADR-0177` DOES occur once, at
`DECISIONS.md:2426`, as prose inside ADR-0176's `**Consequences.**` paragraph, so only
`grep -c '^## ADR-0177'` = **0** answers "has it fired?".

**The three affirmative clearances the state-4 session claimed were spot-checked and ALL
HOLD** — a false clearance is worse than a missed nit: the 23-cell reconciliation
(checked cell-by-cell against both sources, not by count); "no banked-but-unscheduled
finding was silently fixed" (12 files, exactly three `crates/` hunks, each attributable to
a planned task, both doc hunks read in full); and the honesty of the 109.1 M-5 decline
(zero `envoy-http2` files touched; both glued literals still on disk at the exact banked
line numbers).

---

## §9 — Carry-forwards for the state-6 close-out to bank

- **M-8 supersedes V-1's arithmetic.** The `calibrate-loc-estimate-against-landed-phases`
  record for `109.2` is **+58%** whole-tree at the publishing commit and **+52%**
  like-for-like against a projection that budgeted docs but not the state ledger — NOT
  +33%, and the nodocs 562 is 25% BELOW the projection. Any session quoting the +33%
  figure is quoting a comparator error. The standing lesson gains a second leg: re-run a
  numstat AT the commit that will contain the sentence, **and** check that the
  denominator you divide by covers the same file set.
- **The three prose fixes that belong to the next slice touching `## Runtime`** (V-4 the
  CF-109-3 quantifier, V-5's dropped NOT-MEASURED sentence, V-6's two sampling sites,
  V-7's "30-40 probes each", V-11 + N-3's unblock conditions, N-4's PRNG qualifier, plus
  **M-2**'s CF-109-2 conservative-reject leg and **M-5**'s dropped `config_dump` datum) —
  roughly 12 lines in one file, all in one section.
- **The two code-doc fixes**: **M-1** (`bootstrap.rs`'s false cascade catch-all, ~2 lines)
  and **V-8/N-9** (relabel the three `M-*` rows `edge: M-* …` in `runtime.rs`, ~3 lines,
  which also closes N-8's third-category gap in the fn doc).
- **The fixture extension**, for whichever slice next touches `0088` or adds a runtime
  fixture: **M-3**'s cell-11 mirror probe (~11 lines) closes the both-directions
  unparseable reading differentially; **M-4** notes that cells B1-B3 and N1/N2 are
  fixture-able too, and that the cascade's `v < 0` arm currently has zero differential
  coverage. **M-4 also records that `109.2/SPEC.md:46` is measured-false on bools** — a
  landed artifact, so this finding is the correction.
- **M-6/V-12 for the next PLAN-write**: a task-boundary gate recorded as "exit 0" is not
  the prescribed evidence, and substituting an earlier task's `Compiling` count is not a
  substitute. Either measure the count or waive the check explicitly.
- **N-2**: the distinct-`path:` rule in `BEHAVIOR_CONTRACT.md` is scoped to
  `http1_access_log_byte_exact`; two `0088` artifacts cite it as generic. Fix the citation
  or generalise the rule, but not silently.
- **109.1 M-5 stays banked and unfixed** (the two glued `envoy-http2/src/hcm.rs:1925/:2045`
  literals; rustfmt will not reflow them, so `fmt --check` is blind forever). The 108.2
  banked set (M-2, N-1…N-6), the `109.1` N-1…N-6 set, and the 108.1 / 76.1 / 76.2 families
  stay banked (§6.3).
- **CF-109-1 (WIDENED) / CF-109-2 / CF-109-3 stay OPEN**, honouring sides unbuilt.
  CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6 pass through untouched.

---

## §10 — Assessment

`109.2` is a small, disciplined slice that does the two things its SPEC exists for and does
them well: it lands the first differential witness of `runtime_fraction` in the corpus,
and it turns a 23-cell measurement that lived in two phase SPECs into a canonical contract
record. The fixture is genuinely non-vacuous — p2, p3, p4, p6, p8 and above all p9 each
kill a distinct plausible wrong implementation, the driver checks an absolute oracle on
both sides rather than only cross-proxy equality, and the two vacuity mutations target the
right things. The three witness rows close the exact three guards the previous round
found unwitnessed, using the CORRECTED remedy rather than the one that round refuted —
which is the strongest possible evidence that the banked-findings mechanism works.

The defect profile is unusually consistent: **zero findings in the code, zero in the
fixture data, zero in any assertion — every one is in prose, a citation, or fixture
coverage.** Three shapes recur. First, a claim asserted more broadly than its measurement
(M-5, V-4, V-7, N-4). Second, a figure taken at one commit and republished at another
(V-1, M-7) — and once, in V-1 itself, a corrective finding whose own arithmetic inverts
which comparator supports which number (M-8). Third, an enumeration that reads exhaustive
and is not (V-9, and its fixture-side consequence M-3/M-4).

The most valuable single output of this review is **M-8**: the state-4 verifier caught a
stale LoC citation correctly, then drew a conclusion from it that is arithmetically
backwards, and that conclusion is already live in `STATE.md`'s Standing-traps line where
the next session inherits it. A banked trap that teaches the wrong lesson is worse than no
trap. The second most valuable is **M-1** — the only NEW false claim the phase introduced,
sitting in shipping-code doc on precisely the surface the phase exists to document.
Neither changes a byte of behaviour, and neither is worth three more sessions to fix.

**Gate (f) is CLOSED. All six §7.5 gates are now GREEN. Sub-phase `109.2` is approved to
land.**

**Next state: §5 state 6 — the close-out**, in which ROADMAP rows `109.2` AND parent `109`
flip to `done` TOGETHER (the 76.2/108.2 two-row precedent — assert each row's own starting
status: `109.2` is `planned`, `109` is `in-progress`), the `### Sub-phase 109.2 §5 state-5
code review` Notes subsection is RETIRED to `STATE_HISTORY.md`, and **no ADR and no new
Notes subsection is added** (both are measured precedents). It is a **separate session**
per §5.1 and ADR-0127 — a reviewer must not close out what it graded — and the next-phase
PICK is a separate session again after that. This review **fixed nothing**, as ADR-0165
requires.
