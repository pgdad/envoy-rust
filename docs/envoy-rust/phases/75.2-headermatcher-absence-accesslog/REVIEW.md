# Sub-phase 75.2 — `HeaderMatcher` absence semantics: the ACCESS-LOG-path differential witnesses + the contract bank — §5 state-5 CODE-REVIEW

> **What this document is.** The `REVIEW.md` for sub-phase **75.2**, produced by the
> §5 state-5 code review (`superpowers:requesting-code-review`). It is gate **(f)**
> of the `BOOTSTRAP_PROMPT.md` §7.5 phase-done gate; gates (a)–(e) were run and
> recorded GREEN at state-4 and are NOT re-run here.
>
> **Written for a stranger with zero prior context (D-3.4).** Every finding below
> was RE-VERIFIED on disk by the main session; nothing is reported on a subagent's
> word alone. Every line number was re-derived by TEXT ANCHOR at
> `HEAD == 1f05c2d58b615e49d769463a1602180f32f05e68`, never inherited from a
> document.

---

## §0. Scope, method, and what was reviewed

**Git range reviewed:** `3f0ec89..1f05c2d` (base = the sub-phase-75.1 close-out;
head = the state-4 verification's second commit). 22 paths, **+4117 / −42**.

**The review surface is DOCUMENTS AND FIXTURES, not Rust behavior.** 75.2 changed
no `crates/` behavior — its only two `crates/` edits are comment-only. I re-proved
this independently rather than inheriting it:

```
$ git diff 3f0ec89..1f05c2d -- crates/ | grep -E '^[+-]' | grep -v '^[+-][+-]' \
    | grep -vE '^[+-][[:space:]]*(///|//)'
(empty)
```

**Method.** Six READ-ONLY review dimensions were fanned out to subagents
(`superpowers:dispatching-parallel-agents`); every finding they returned was then
RE-VERIFIED on disk by the main session, and three were DROPPED or downgraded on
re-verification (§7). The main session additionally ran its own MEASURED mutation
experiment in a scratch `git worktree` (§2), which is the load-bearing new evidence
in this review.

**Cold-start state confirmed before anything else:** `git status --porcelain`
clean, branch `main`, `HEAD` and `origin/main` both at
`1f05c2d58b615e49d769463a1602180f32f05e68`, re-checked with `git fetch origin
--prune`. CI on that FULL 40-char SHA is run **`30253564426`** — `completed` /
`success`, both jobs at FULL step counts **15** (`build + test + lint`) and **13**
(`fuzz`), so no runner-starvation signature.

---

## §1. Verdict

## **CHANGES-REQUESTED — 0 Critical, 4 Important, 8 Minor, 7 Nit. Re-entry is §5.2 state-3, NOT state-4.**

The sub-phase does what it set out to do. Both fixtures are green cross-proxy,
both are load-bearing for the specific pre-75.1 → post-75.1 transition they were
chartered to witness, the new `### Phase 75` contract block is accurate against the
engine cell-for-cell, the M74-31 four-site correction is complete with zero missed
live sites and zero append-only violations, and all five in-scope sub-phase-75.1
review findings are genuinely CLOSED. This is careful work.

It is nonetheless **not** approvable as it stands, for two independent reasons:

1. **A MEASURED test gap in the phase's primary deliverable (I-1).** Both new
   fixtures render an IDENTICAL log line from every probe, so the driver's
   `(count == 1) ∧ (lines byte-equal)` assertion cannot attribute the surviving
   line to a probe. I measured, in a scratch worktree, that `0085` — the fixture
   whose entire stated purpose is to witness a `present_match` **polarity** —
   stays GREEN under a polarity-INVERTED engine, and that `0084` stays GREEN under
   an engine that drops the `invert_match` XOR entirely. Both mutations turn
   in-process assertions RED (7 and 4 respectively), so they are real semantic
   breakages, not build artifacts. The fix is ~6 lines across 4 files.

2. **Three factual defects the phase introduced into `BEHAVIOR_CONTRACT.md`
   (I-2, I-3, I-4)** — the project's canonical, authoritative reference under
   doctrine D-3.3. One of them is a self-contradiction the phase created and that
   its own **ADR-0162 already licensed fixing**.

None of these is a code-correctness defect. The engine is right, the rule is right,
and nothing shipped is behaviorally wrong. The re-entry is documentation plus a
cheap fixture strengthening.

---

## §2. MEASURED evidence produced by THIS review (scratch `git worktree`)

The state-5 discipline is that a green gate proves the code does what its tests
ask, not that the tests ask the right question. This section is the measurement
that answers that.

### §2.1 Setup and controls

A scratch worktree was created DETACHED at HEAD (memories
`mutation-checks-collide-with-parallel-subagents`,
`worktree-subagents-get-stale-base`), so no parallel agent's `git checkout` could
silently revert a mutation:

```
$ git worktree add --detach <scratch>/rev75-mut 1f05c2d58b615e49d769463a1602180f32f05e68
Preparing worktree (detached HEAD 1f05c2d)
$ git rev-parse HEAD
1f05c2d58b615e49d769463a1602180f32f05e68
$ git status --porcelain
(clean)
```

**UNMUTATED control, from that same worktree** — required, because a RED that never
reached an assertion is not evidence (memory `mutation-red-needs-unmutated-control`):

```
running 1 test
test headermatcher_absence_accesslog_present_polarity ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.24s
```

### §2.2 Mutation P — POLARITY INVERSION. `0085` stays GREEN.

`crates/envoy-config/src/matcher.rs:50`, `v.is_some() == *want_present` →
`v.is_some() != *want_present`. This inverts exactly the rule `0085` exists to
witness.

```
$ cargo build -p envoy-bin 2>&1 | grep -c 'Compiling envoy-config'
1

$ cargo test -p envoy-config --lib
    matcher::tests::absence_semantics_matrix_matches_measured_upstream
    matcher::tests::invert_match_inverts_present_match_result
    matcher::tests::present_match_false_matches_when_absent
    matcher::tests::present_match_false_requires_the_header_to_be_absent
    matcher::tests::present_match_true_returns_false_when_absent
    matcher::tests::present_match_true_returns_true_when_present
    matcher::tests::pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream
test result: FAILED. 642 passed; 7 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

$ cargo test -p differential --test headermatcher_absence_accesslog_present_polarity
test headermatcher_absence_accesslog_present_polarity ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.14s
```

**SEVEN in-process assertions RED, and fixture `0085` GREEN.** The `Compiling
envoy-config` count of 1 proves the run was not served from a stale binary
(memory `mutation-check-needs-forced-rebuild`).

Why it passes: probe 1 (`x-a: v`, want `false`) flips DROP → KEEP and probe 2
(absent) flips KEEP → DROP. envoy-rust still writes exactly ONE line; upstream
still writes exactly ONE line; both probes render `STATUS=200 PATH=/x`, so the
whole-line byte compare succeeds on textually identical lines produced by
DIFFERENT requests.

### §2.3 Mutation X — DROP THE `invert_match` XOR. `0084` stays GREEN.

`crates/envoy-config/src/matcher.rs:69`, `mode_result ^ self.invert_match` →
`mode_result` (with `let _ = self.invert_match;` to keep it compiling).

```
$ cargo build -p envoy-bin 2>&1 | grep -c 'Compiling envoy-config'
1
$ cargo test -p envoy-config --lib
test result: FAILED. 645 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
$ cargo test -p differential --test headermatcher_absence_accesslog
test headermatcher_absence_accesslog ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.13s
```

**FOUR in-process assertions RED, and fixture `0084` GREEN.** Probe 1 (absent)
still DROPS via the `return false` short-circuit; probe 2 (`x-a: v`) flips DROP →
KEEP; probe 3 (`x-a: zzz`) flips KEEP → DROP. One line each side, textually
identical, GREEN.

### §2.4 Restored control — the mutations were genuinely present and genuinely reverted

```
$ git checkout -- crates/envoy-config/src/matcher.rs
$ git status --porcelain
(clean)
$ cargo build -p envoy-bin 2>&1 | grep -c 'Compiling envoy-config'
1
test result: ok. 649 passed; 0 failed; ...        # envoy-config --lib
test result: ok. 1 passed; 0 failed; ...          # 0084
test result: ok. 1 passed; 0 failed; ...          # 0085
```

`649 passed` restored from `642`/`645`. The scratch worktree was then removed with
`git worktree remove --force`; **only my own** worktree was removed — the four
`.claude/worktrees/agent-*` belonging to the parallel workstream were left
untouched, and the main tree was re-confirmed clean at `1f05c2d`.

### §2.5 What this does and does NOT show

- It does **NOT** contradict ADR-0162 or the state-3 record. Those measured the
  *pre-75.1 revert* (`(_, None) => return false` → `(_, None) => false`), which
  makes `0084` RED with `envoy_rust=2, envoy=1`, and the `PresentMatch`
  always-true revert, which makes `0085` RED. Both remain true and I did not
  re-run them. The fixtures ARE load-bearing for the transition they were built
  for.
- It **does** show the fixtures are blind to a whole adjacent class — any
  regression that moves a kept line from one probe to another while preserving the
  count. That class includes the exact polarity inversion `0085` is named for.

---

## §3. The TWO handed-forward observations — both ADJUDICATED

`STATE.md` handed exactly two observations to this review, neither fixed. Both are
adjudicated here, as required.

### §3.1 Observation 1 — the phase-72 `**§C` "three" vs the MEASURED "four": **REAL. Finding I-4.**

Located BY TEXT ANCHOR (`grep -n "in-process guards RED"`, a single hit),
`docs/envoy-rust/BEHAVIOR_CONTRACT.md:2407`:

> `75.1 PLAN-write and its implementation, and turns three in-process guards RED`

The measured number is **four**, and this is not a "was three then, four now"
scoping question — it was four at 75.1 too, recorded in **75.1's own PROGRESS.md**:

```
docs/envoy-rust/phases/75.1-.../PROGRESS.md:278
test result: FAILED. 56 passed; 4 failed; 0 ignored; 0 measured; 589 filtered out
    matcher::tests::absence_semantics_matrix_matches_measured_upstream
    matcher::tests::invert_match_inverts_present_match_result
    matcher::tests::present_match_false_matches_when_absent
    matcher::tests::pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream
```

and, at `:321`, explicitly: *"**`4 failed`, not the plan's `3`**"* — the `3` came
from a pre-flight measured **before** Task 1 added the fourth guard. I confirmed
all four guards already existed at the 75.1 task-3 mutation check
(`git log -S <name> -- crates/envoy-config/src/matcher.rs`: every one is present
by `723bc3b`, task 2, which precedes task 3's `4717b3b`).

**Adjudication: the correction is warranted, in scope, and already ADR-covered.**
`BEHAVIOR_CONTRACT.md` is a LIVE document — `BOOTSTRAP_PROMPT.md:226` (§4.1
invariant 5) prescribes updating it in place, and append-only is stated
*explicitly and only* for `ROADMAP.md` (invariant 2) and `DECISIONS.md`
(invariant 4). History confirms routine in-place rewriting (`a845b6d` at 67 ins /
23 del). And **ADR-0162's own title** already reads *"…plus the §D guard count
corrected THREE → FOUR on a fresh measurement"* — the ADR fired, but only the NEW
Phase 75 §D received the corrected count; the older phase-72 site was not swept.
The result is that the canonical contract now states **two different numbers for
the same measured fact, 402 lines apart** (`:2407` "three" vs `:2809` "**FOUR**"),
and the phase itself created that contradiction. See I-4.

### §3.2 Observation 2 — the h2spec conformance gate: **HALF CONFIRMED, HALF REFUTED. Not a 75.2 defect; opens CF-75-3.**

The state-4 record suspected the gate may be VACUOUS on two grounds. I adjudicate
them separately, because they do not stand or fall together.

**Ground 1 — the LOCAL silent self-skip: CONFIRMED.**
`tests/conformance/h2spec/tests/h2spec_runner.rs:20-32`:

```rust
    if let Err(e) = outcome {
        if e.to_string().contains("h2spec not found") {
            eprintln!("h2spec_runner: {} — skipping locally", e);
            return;
        }
        panic!("h2spec gate failed: {e:#}");
    }
```

The branch returns SUCCESS (not `#[ignore]`, not a failure), and `cargo test`
discards the message. On a host without the binary — this one — the crate's
`3 passed` is two pure string-parser unit tests plus one no-op. That is a real
observability defect.

**Ground 2 — the implausible CI duration: REFUTED. The suite genuinely runs in CI.**
Three independent lines of evidence:

1. **The structural proof.** `h2spec_pass_rate_gate` can report `ok` by exactly
   two paths: the skip, or `run_h2spec_gate()` returning `Ok(())`. The latter
   requires passing **gate (c)** at `h2spec_runner.rs:134-142` — *"every test in
   `known-failures.txt` must actually fail"*. `known-failures.txt` has exactly ONE
   real entry (21 lines = 20 comment/blank + `3.5/2`). So a CI `ok` is positive
   proof that h2spec ran AND reported `3.5/2` failing. The skip path cannot
   satisfy it.
2. **The CI log on HEAD, verbatim** (run `30253564426`, job `89936882077`) — and
   the skip message appears NOWHERE in it:
   ```
   09:28:39.2446748  Running tests/h2spec_runner.rs (target/debug/deps/h2spec_runner-73485d2bad653f8a)
   09:28:39.2530418  test tests::parse_h2spec_output_extracts_section_failure_ids ... ok
   09:28:39.4109107  test h2spec_pass_rate_gate ... ok
   ```
3. **The timing argument inverts once calibrated.** A genuine skip costs **0.01 s**
   — measured and recorded verbatim by the phase that introduced it
   (`docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md:851-861`,
   `finished in 0.01s`). The gate on HEAD took ~158 ms, 13–18× that floor, and
   h2spec itself self-reports `Finished in 0.2086 seconds` for its 146-case suite
   in the one CI run that failed and therefore dumped its stdout (run
   `25294002788`). **0.15 s is the signature of a real run, not of a skip.**

CI provisioning is in the SAME job and BEFORE the test step (`ci.yml:43-49` install
h2spec 2.6.0 under `set -euo pipefail` with a `--version` assertion; `ci.yml:67`
`cargo test --workspace`), so there is no ordering hole either.

**One genuinely NEW adjacent finding, which I verified myself.** `ci.yml:67` is
`cargo test --workspace` with **no `--no-fail-fast`**, so on a RED CI run cargo
aborts at the first failing binary and the conformance gate may never execute at
all. Sampled independently:

```
$ # run 29862045509, job 88740929412 → h2spec_pass_rate_gate lines: 0
$ # run 29216408216, job 86713092404 → h2spec_pass_rate_gate lines: 0
```

On a red CI run, conformance status is **unknown**, not green.

**Adjudication.** PRE-EXISTING since `5914b14` (2026-05-03) and wholly unrelated
to 75.2, which touched no `ci.yml`, no `tests/conformance/**`, no
`known-failures.txt`, no HTTP/2 or codec code (verified from
`git diff --name-status 3f0ec89..1f05c2d`). **Do NOT widen 75.2 into it.** Gate (c)
rests on the criterion `PLAN.md` states — *"unchanged"* — which IS satisfied:
`known-failures.txt` byte-identical at 21 lines, no conformance surface touched,
CI green on this SHA. Banked as new carry-forward **CF-75-3** (§9), owner = its own
phase. **The state-4 record's stronger claim — that the gate "may be vacuous" in
CI — is REFUTED and must not be propagated forward.** ADR-0163 records this.

---

## §4. Independent re-verification of the phase's own claims

Everything below was re-derived by the main session at HEAD. The state-4 gate was
**not** re-run as this session's state.

| Claim (from the state-4 record) | Re-derived independently | Verdict |
|---|---|---|
| CI `2105 passed / 0 failed` over **162** binaries | `gh run view --job 89936882077 --log` → 162 `test result:` lines, `passed` sum **2105**, `failed` sum **0** | **HOLDS** |
| `2100 + 5 = 2105 = CI passed` cross-check | arithmetic holds against the above | **HOLDS** |
| CI both jobs at FULL step counts 15 / 13 | `gh run view 30253564426 --json jobs` → 15 and 13 | **HOLDS** |
| **85** fixture dirs | `git ls-files 'tests/fixtures/*' \| cut -d/ -f3 \| sort -u \| grep -c '^[0-9]'` → **85** | **HOLDS** |
| **85** differential test files | `git ls-files 'tests/differential/tests/*.rs' \| wc -l` → **85** | **HOLDS** |
| **5** fuzz targets / **63** seeds | `git ls-files` → **5** / **63** | **HOLDS** |
| `known-failures.txt` **21** lines, not trimmed | `wc -l` → **21**; absent from the range's `--name-status` | **HOLDS** |
| `BEHAVIOR_CONTRACT.md` **3560** lines | `wc -l` → **3560** | **HOLDS** |
| ROADMAP **104** rows / **102** `done` / 2 `in-progress` (`75`, `75.2`) | split on `' \| '`, status is field **4** → 104 / 102, exactly `75` and `75.2` | **HOLDS** |
| ROADMAP row `75.2` edit is status-cell-only, 6 cells preserved | full `git diff` of the file: 1 line changed, `planned` → `in-progress` | **HOLDS** |
| Only two `crates/` edits, comment-only | filtered diff prints EMPTY (§0) | **HOLDS** |
| Ledger head **ADR-0162**, next **ADR-0163** | `git diff` adds exactly ADR-0161 + ADR-0162; `DECISIONS.md` 40 ins / 0 del | **HOLDS** |
| Per-side config divergence = the four sanctioned deltas, both fixtures | `diff -u envoy.yaml envoy-rust.yaml` on each: drop `admin:`, `0.0.0.0`→`127.0.0.1`, drop `generate_request_id: false`, repoint `path:`. The `header_filter` body is byte-identical on both sides. | **HOLDS** |
| Engine rule matches contract §A cell-for-cell | read `crates/envoy-config/src/matcher.rs:39-70` directly; the absent arm `(_, None) => return false` sits AFTER the `PresentMatch` arm and BEFORE every value arm, `return` bypassing the closing XOR | **HOLDS** |
| No append-only violation | no file under `phases/74-*`, `phases/75-*`, `phases/75.1-*` in the range; `DECISIONS.md` 40/0, `STATE_HISTORY.md` 110/0; `74/REVIEW.md:1269`'s append-only "FIVE" left alone | **HOLDS** |
| M74-31 corrected at all FOUR live sites, zero missed | independent sweep over the 654-file live pathspec; every hit adjudicated as fixed / descriptive-not-a-site / legitimately append-only | **HOLDS** |
| `0081` not weakened | filtered diff of its `expectations.yaml` (non-comment lines) prints EMPTY; 3 probes / 2 kept / order intact; no `on_header_missing` added | **HOLDS** |

**One inherited figure did NOT hold, and it is the state-4 session's own
self-description** — see N-4.

---

## §5. Findings

### §5.1 Critical

**None.** No correctness defect, no security concern, no broken functionality. The
engine is unchanged and correct; both fixtures are green; CI is green on HEAD.

### §5.2 Important (MUST-FIX before this sub-phase can close)

---

**I-1 — Both new fixtures are blind to any regression that MOVES a kept line
between probes. MEASURED. The remedy is ~6 lines.**

**Sites:** `tests/fixtures/0084-headermatcher-absence-accesslog/expectations.yaml`
(all three probes `path: /x`);
`tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity/expectations.yaml`
(both probes `path: /x`); both `envoy.yaml`/`envoy-rust.yaml` route tables
(`match: { path: "/x" }`).

**What is wrong.** The log format is
`"STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n"` and every probe uses `path: /x`, so
every probe renders the byte-identical line `STATUS=200 PATH=/x`. The driver
asserts only (a) each side's file holds exactly `expected_logged_count(probes)`
lines and (b) the lines are byte-identical cross-proxy. With `expected_logged_count
== 1` on both fixtures, the assertion collapses to *"each side has exactly one
line, and the two lines are equal"* — which **cannot attribute the surviving line
to a probe**.

**Why it matters.** MEASURED in §2, not argued: `0085` stays GREEN under a
polarity-INVERTED engine (7 in-process assertions RED), and `0084` stays GREEN
under an engine with the `invert_match` XOR removed (4 in-process assertions RED).
`0085` is the fixture whose whole stated purpose is to witness a `present_match`
POLARITY, and it cannot detect a polarity inversion. `BEHAVIOR_CONTRACT.md:2848`
calls these the **"Authoritative fixtures"** for the rule, which overclaims what
they actually pin.

A secondary consequence at `0084/README.md:37-38`:

> `Probe 2 is the control that proves the matcher is live at all, so probe 1's`
> `silence is attributable to the ABSENCE rule and not to a dead filter.`

This is backwards for the XOR-drop class. With probe 2 REMOVED, that regression
would give envoy-rust 0 lines against an expected 1 → **RED**. Probe 2 is what
converts that RED into a GREEN. Its stated purpose is also already discharged by
the count alone: an always-log filter yields 3 lines, an always-drop yields 0.

**How to fix.** Give each probe a distinct `path:` (e.g. `/absent`, `/valmatch`,
`/valmiss` on `0084`; `/present`, `/absent` on `0085`) and widen the route to
`match: { prefix: "/" }`. `%REQ(:PATH)%` is already in the format string and
`:path` is allow-listed, so the kept line becomes self-identifying and both masked
classes go RED at zero extra runtime cost. Note the state-2 recon itself used
distinct paths per request (`SPEC.md:76-77`: `/absent`, `/valmatch`, `/valmiss`,
`/empty`) — the fixtures collapsed them to `/x` by copying the `0078` stencil.
Update both READMEs' expected-line blocks and `BEHAVIOR_CONTRACT.md` §G accordingly.

**Calibrating this fairly.** The fixtures DO meet the §6 differential surface they
were chartered for — the pre-75.1 → post-75.1 transition is genuinely witnessed
(ADR-0162's measured RED). Both masked classes are caught elsewhere: in-process by
7 and 4 assertions respectively, and cross-proxy on the ROUTE path by `0083`, whose
`http1_probe_list` driver asserts each of its 22 probes individually. The
limitation is inherited from the `0078` house stencil, not invented here. It is
Important rather than Critical because nothing shipped is wrong — but it is a real
gap between what the artifacts claim and what they pin, and the fix is cheap.

---

**I-2 — `BEHAVIOR_CONTRACT.md:2781`: the §C caption asserts a blanket parity that
two of its own nine rows contradict.**

> `pre-75.1 in-tree behavior; every cell now matches the upstream column.`

Rows **s5** (`:2789`) and **s6** (`:2790`) carry `*(boot-fatal)*` in the envoy-rust
column and are still boot-fatal today — they are the OPEN CF-72-2 reject-direction
gaps, as those same rows' own verdict cells say. "Every cell now matches the
upstream column" is false for 2 of 9 rows.

**Why it matters.** The contract is authoritative (`BOOTSTRAP_PROMPT.md:226`), and
a reader skimming the caption concludes CF-72-2 is closed when the phase's own
`§D` record three hundred lines earlier says it is banked-not-fixed.

**Fix.** Scope the sentence, e.g. *"every cell that RUNS on both proxies now
matches the upstream column; s5/s6 remain boot-fatal here — the open CF-72-2
reject-direction gaps."*

---

**I-3 — `BEHAVIOR_CONTRACT.md:2446-2451`: the new `contains_match` bullet cites the
WRONG source site, and endorses a rationale the same bullet's own measurement
refutes.**

The bullet reads:

> `3. **The top-level \`contains_match\` arm** — a THIRD member, NEW at phase 75.`
> `   Upstream accepts it (with a deprecation warning); envoy-rust rejects it as an`
> `   unknown field. It is reachable in-tree only as \`string_match: { contains: … }\`,`
> `   BY DESIGN — see the \`HeaderMatcher\` deserializer in`
> `   \`crates/envoy-config/src/bootstrap.rs\`, which documents the v1.33.0 rationale`
> `   for admitting \`contains\` only through \`StringMatcher\`.`

Two problems, both provable from disk alone:

1. **Wrong site.** The `HeaderMatcher` deserializer documents nothing about
   v1.33.0 — it merely omits `contains_match` from its key lists. The rationale
   actually lives on `StringMatcherMode::Contains`,
   `crates/envoy-config/src/bootstrap.rs:2982-2985`. (The PLAN cited
   `bootstrap.rs:2976-2979`; already drifted by 6 lines.)
2. **The endorsed rationale is measured-FALSE.** That comment reads:

   ```
   /// no top-level HeaderMatcherMode::ContainsMatch (Envoy v1.33.0 only
   /// supports Contains via the modern string_match field; SPEC §6 signpost 8).
   ```

   The same contract bullet, three lines earlier, states the MEASURED fact that
   upstream v1.33.0 **does** accept top-level `contains_match`, with a deprecation
   warning. So the contract says *"BY DESIGN — see [comment X]"* where comment X
   asserts the opposite of what the contract just measured.

**Why it matters.** The bullet's own substantive claim is correct; the problem is
that it routes a future implementer to a stale in-source justification that
contradicts it. This is exactly the measured-not-assumed failure mode the project
guards against. **Fix:** re-point at `bootstrap.rs` `StringMatcherMode::Contains`
(line-number-free), and either correct or explicitly supersede the in-source
comment's "only supports Contains via `string_match`" claim.

---

**I-4 — `BEHAVIOR_CONTRACT.md:2407` says "three in-process guards RED" where
`:2809` in the SAME FILE says "**FOUR**" about the SAME mutation. The phase created
this contradiction, and ADR-0162 already licensed the fix.**

Full adjudication and evidence in §3.1. In short: four is the measured figure, on
two independent runs (75.1's own `PROGRESS.md:278` `56 passed; 4 failed`, and
75.2's `PROGRESS.md:311-316` `645 passed; 4 failed` against a `649 passed`
control). `BEHAVIOR_CONTRACT.md` is LIVE. ADR-0162's title already records the
`THREE → FOUR` correction — it was applied to the new Phase 75 §D but the older
phase-72 site was not swept.

**Secondary hazard at the same site.** The "Pinned in-process by …" list at
`:2414-2418` names FOUR tests, but it is a **different** set from the four the
mutation reddens: it includes
`pv4_value_matcher_absent_plus_invert_dropped_is_parity_with_upstream` (which the
hoist does NOT break) and omits `present_match_false_matches_when_absent` (which it
does). A reader who sees "three … RED" directly above a four-name list will
reconcile them into the wrong subset.

**Fix.** `three` → `four` at `:2407`, plus a clause making clear the RED set is not
the pinning set, cross-referencing Phase 75 §D (which already does this correctly,
naming all four RED tests explicitly).

---

### §5.3 Minor

**M-1 — Both new READMEs claim CF-75-2 is BANKED in `BEHAVIOR_CONTRACT.md`. It is
not there at all.**
`tests/fixtures/0084-headermatcher-absence-accesslog/README.md:136-139` ("All three
are BANKED in `BEHAVIOR_CONTRACT.md`, not fixed here.") and
`tests/fixtures/0085-.../README.md:159-167` ("The three carry-forwards are BANKED
…"). But `grep -c "CF-75-2" docs/envoy-rust/BEHAVIOR_CONTRACT.md` → **0**, and the
duplicate-header comma-join rule appears nowhere in that file. The project's own
ledger is accurate — `STATE.md:117` lists CF-72-2 and CF-75-1 as banked and CF-75-2
separately as merely a live carry-forward "needs its own phase". Originates at
`PLAN.md:401`. **Fix:** say two are banked and CF-75-2 is an open carry-forward
recorded in `STATE.md`, not in the contract.

**M-2 — `0085`'s README and entrypoint claim the D2 cell had "NO behavioral test
anywhere in the tree" before phase 75. It had two, and a LIVE comment landed by the
sibling sub-phase says so.**
`tests/fixtures/0085-.../README.md:43` and
`tests/differential/tests/headermatcher_absence_accesslog_present_polarity.rs:25`.
The pre-75.1 tree carried `present_match_false_returns_true_when_present` and
`present_match_false_returns_true_when_absent`
(`git show f68b160^:crates/envoy-config/src/matcher.rs:342,348`) — they existed and
they ASSERTED the divergence, which is a materially different and more interesting
situation than "no test." `crates/envoy-config/src/matcher.rs:366-368` says exactly
that today: *"Before phase 75.1 this test asserted the opposite … and was the
in-tree test that PINNED divergence D2."* Inherited from the frozen parent
`75/SPEC.md:252-253`, but restated in two NEW live files. `0084` makes no such
claim. **Fix:** "before phase 75 the only in-tree tests of this cell ASSERTED the
divergence, and there was no cross-proxy witness anywhere."

**M-3 — `BEHAVIOR_CONTRACT.md:2849` gives `0083` as "~24 probes"; it has 22, and
`:2419` of the same file already says 22.**
`grep -c '^    - name: p' tests/fixtures/0083-headermatcher-absence-parity/expectations.yaml`
→ **22**, matching `75.1/PROGRESS.md:893` ("22 probes across 8 matchers"). `~24` is
a pre-implementation estimate inherited from `75.1/SPEC.md` and `ROADMAP.md:200`.
Small, but it makes the canonical contract self-inconsistent 430 lines apart when
the correct figure was already in the file.

**M-4 — `BEHAVIOR_CONTRACT.md:2713-2714`, the one M74-31 rewrite that still
over-attributes a consequence to probe placement.**

> `What placing it SECOND buys is` / `that the two kept lines are byte-DISTINCT in a pinned ORDER`

Byte-distinctness (`M=-` vs `M=1`) is a property of the rendered lines and holds
regardless of probe order; what placement SECOND actually buys is only the specific
ORDER. This is the same *shape* as the defect being corrected, though far weaker
(the outcome asserted is still true). The three sibling sites got it exactly right
— e.g. `tests/fixtures/0081-accesslog-metadata-filter/README.md:108-110` ("it pins
the LINE ORDER").

**M-5 — the state-3 mutation record's quoted `grep -n` line numbers do not
reconcile with the single-line edits it describes.**
`PROGRESS.md:333` and `:356`. At the mutation worktree's base commit `3b44510` the
two mutated lines sit at **50** and **54**:

```
$ git show 3b44510:crates/envoy-config/src/matcher.rs | grep -n 'None) =>\|want_present), v)'
50:            (HeaderMatcherMode::PresentMatch(want_present), v) => v.is_some() == *want_present,
54:            (_, None) => return false,
```

Mutation A2 is described as *deleting the keyword `return`* — a pure in-place edit
that cannot move its own line — yet the record quotes it at **`57:`** (+3).
Mutation B's replacement arm is quoted at **`53:`** where the arm it replaces is at
**50** (+3). Mutation A1's quoted `46:` IS consistent with a pure move to the top
of a `match` opening at 45, so the mutator demonstrably does not add marker lines
in general.

**Why it matters.** The uniform +3 is jointly consistent with A2 and B having been
present in the file SIMULTANEOUSLY, which would contradict `PROGRESS.md:258-260`
("The worktree was restored to pristine between mutations"). **The load-bearing
conclusion survives either reading, and I verified that from source:** B touches
only the `PresentMatch` arm, which `0084` never exercises (it uses `exact_match`);
A2 touches only the `(_, None)` arm, which `0085` never reaches (the
`(PresentMatch(want), v)` arm matches ANY `v`, including `None`, and is matched
first). So each fixture's RED is attributable to exactly one mutation either way.
But the record IS the evidence, and here it does not reconcile with its own
narrative. The worktree was removed, so this cannot be resolved from disk. **Fix:**
a one-line disclosure of what the mutated file actually looked like.

**M-6 — one of the five gate-(b) REDs is adjudicated without its own failure
text.** `PROGRESS.md:~1015-1042`. Panic text is quoted for
`access_log_h2_rcd_upstream_reset`, `access_log_h2_uc_upstream_reset`,
`access_log_rf_upstream_reset` and `admin_config_dump_server_info` — four of five.
**`access_log_rcd_upstream_reset` has only its isolation verdict line
(`test result: FAILED. 0 passed; 1 failed`), never a `stdout` block.** I confirmed
this by extracting every `---- … stdout ----` header in that section: the fourth
`access_log_*_upstream_reset` binary has none. It is therefore adjudicated by
family membership and name — precisely the pattern-match the plan's own discipline
forbids (`PLAN.md:1596`, "**Read the failure TEXT**"). Mitigating: it is a real
binary, named verbatim in the plan's documented flake list, it fails
deterministically in isolation (which IS the environmental signature for this
family), and it is GREEN in CI on this SHA. Minor for those reasons — but a
five-RED gate should ship five texts.

**M-7 — `ROADMAP.md` row `75.2` still carries the "five-site" figure this
sub-phase itself refuted. REVIEWER DECISION: LEAVE IT.** The row's summary cell
reads *"CONSUMES M74-31 by correcting the five-site kept-LAST non-sequitur"*, while
ADR-0161 correction C4 refuted FIVE as FOUR by live sweep, Task 7 fixed exactly
four, and `PROGRESS.md:519-524` says four. The row is still `in-progress`, so it
has not frozen yet — but `ROADMAP.md` is append-only under `BOOTSTRAP_PROMPT.md`
§4.1 invariant 2 ("only update status and sub-phases columns"), and the state-6
close-out flips the status cell ONLY. **I am recording the explicit decision so the
close-out does not have to re-litigate it: do NOT rewrite the summary cell.** The
refutation is durably recorded in ADR-0161, in `PLAN.md:47` and in
`PROGRESS.md:519-524`; that is where a future reader will find it, and the
append-only rule is worth more than the cosmetic fix. Listed as a finding only so
the discrepancy is not later mistaken for an oversight.

**M-8 — the state-4 commit rewrote the `### Doctrine reminders` §5.1 bullet WITHOUT
first relocating its prior text, which ADR-0035 and ADR-0160 both require. Found by
this review's own delta-based relocation check; ALREADY REPAIRED, losslessly.**

`BOOTSTRAP_PROMPT.md` §4.1 invariant 9 and ADR-0160 require that an enduring
`STATE.md` block superseded IN PLACE have its prior text relocated **verbatim** to
`STATE_HISTORY.md` **before** being rewritten. The house pattern for this specific
bullet is an appended `## Relocated from STATE.md \`## Notes\` — the superseded
\`### Doctrine reminders\` §5.1 bullet (<transition>)` section at EOF; there are
**25** such prior relocations in the archive, so the practice is well established.

Measured:

```
$ A=$(git show 2ae5f46:docs/envoy-rust/STATE.md | grep '^- `BOOTSTRAP_PROMPT.md` §5.1')
$ B=$(git show 1f05c2d:docs/envoy-rust/STATE.md | grep '^- `BOOTSTRAP_PROMPT.md` §5.1')
$ [ "$A" = "$B" ] && echo IDENTICAL || echo DIFFER
DIFFER
```

— the bullet WAS rewritten across the state-4 commits, yet the state-3 wording is
absent from `STATE_HISTORY.md` as of `1f05c2d`. It was the single residual in this
review's delta check (12 of 13 superseded lines relocated; that one orphaned).

**Why it matters.** It is a small, silent loss of exactly the narrative ADR-0035
exists to preserve, and it is self-concealing: the next session's delta check
compares against ITS OWN baseline, so an orphan never resurfaces on its own.

**Already repaired by this review, and the repair deletes nothing.** This session
appended TWO archive sections at EOF — the orphaned state-3 → state-4 bullet
(byte-identical to `git show 2ae5f46:docs/envoy-rust/STATE.md`, with an explicit
note that it was relocated LATE and why) and the state-4 → state-5 bullet this
session itself supersedes. `STATE_HISTORY.md` shows **39 insertions / 0 deletions**
and this review's delta check now reports every superseded line preserved.
**No landed commit was rewritten and nothing retroactive was edited** — restoring
an omitted relocation is additive, and is what ADR-0035 prescribes. Recorded as a
finding so the lapse is visible rather than quietly patched, and so future
state-mutating sessions run the delta check against the FULL superseded set,
including the doctrine bullet.

---

### §5.4 Nit

**N-1 — `0084/README.md:91` and `0085/README.md:104`: the `generate_request_id`
divergence is explained by consequence, not cause.** Both say *"upstream defaults
it on; envoy-rust does not emit request-ids here."* The actual mechanic is that
envoy-rust's parser **rejects** the field: the HCM config struct carries
`#[serde(deny_unknown_fields)]` and has no such field, so writing it on the rust
side would be BOOT-FATAL, not inert. A reader could infer it is optional and add it
to a future fixture. Inherited verbatim from `0078/README.md:59`; lineage-wide
phrasing, not a 75.2 slip.

**N-2 — `BEHAVIOR_CONTRACT.md:2740`: "a 13-probe … ROUTE matrix (7 matcher modes ×
invert polarity × {…})" is a loose factorization** (7 × 2 = 14) and, sitting
directly under a sentence naming fixture `0083`, invites the reader to think it
describes `0083` (22 probes) rather than the state-0/state-2 recon matrix it
actually describes. Inherited verbatim from `:2372` and `ROADMAP.md:199`.

**N-3 — `BEHAVIOR_CONTRACT.md:2478-2481`: CF-75-1's scope note says the residual
divergence is "confined to the PRESENT-value cells, both polarities."** The
present-but-EMPTY-value cell is parity on both proxies (the §G table's own third
row). The parenthetical "(the middle row above)" disambiguates, so this is
presentation only.

**N-4 — the state-4 record's `STATE.md` self-description is stale at HEAD, and it
is the one inherited figure that did NOT re-derive. Record-only.**
`STATE.md:29` states *"NET RESULT: 12 471 → 12 633 characters — this rewrite GREW
the line by 162 characters"*. Measured across the two state-4 commits:

```
2ae5f46 (state-3 head):  12471   ← matches the record's "from"
406c379 (state-4):       12633   ← matches the record's "to"; the record was TRUE when written
1f05c2d (the follow-up): 12833   ← +200 MORE, never restated
```

The follow-up commit `1f05c2d` — whose message says *"Also corrects this phase's
PLAN.md from 1632 to 1631 lines. **No other content changes**"* — in fact added
~160 characters of NEW standing-trap guidance to that line. The `_Historical_`
block is a statement about a specific commit and was accurate at that commit, so
it should be RELOCATED VERBATIM per ADR-0035 rather than retroactively edited; the
commit message is immutable. This review therefore states the CURRENT measured
figure — **12 833 characters** — forward, and takes no retroactive action. Same
disposition class as 75.1's N-3.

**N-5 — two distinct carry-forwards are both named `M-1`.** 75.1's review finding
`M-1` (closed by this phase) and an older unrelated `M-1` that `STATE.md:117` still
lists among live carry-forwards, in the same sentence that says "75.1's
M-1/M-2/M-3 + N-1/N-2 are now CLOSED". The collision predates 75.2
(`75.1/REVIEW.md:478` inherits it) so it is not a defect of this diff, but the two
are indistinguishable by name. Worth disambiguating whenever the older one is next
touched.

**N-6 — `PLAN.md` states "8 tasks" against a plan that defines nine.** `PLAN.md:45`,
`:91` and `:93` all say 8 ("8 tasks against the ~25 gate"), but the plan defines
`### Task 1` … `### Task 9` (lines 159/464/573/835/936/1113/1234/1378/1541), and
the PLAN-write commit `1bf256a` itself says "9 TDD-ordered tasks". Excluding Task 9
(`PROGRESS.md` itself) from the *LoC* table is defensible; carrying that exclusion
into the *task-count* gate is not. No practical consequence — 8 or 9 both clear the
~25 gate comfortably.

**N-7 — `PROGRESS.md` elides COMMAND text, a third elision class its header does
not declare.** The header (`PROGRESS.md:8-15`) declares "exactly two mechanical
exceptions" and both concern *output*, closing with "Nothing else is elided". But
several quoted *command lines* are abbreviated with `…` — `:423`, `:614`, and most
consequentially `:679` (`git diff --shortstat 1bf256aa… (excluding this
PROGRESS.md)`), whose parenthetical understates the exclusion: reproducing its
`17 files / 1026 / 28` also requires excluding `STATE.md` and `STATE_HISTORY.md`.
That is most likely a benign temporal artifact — the command was run before the
ledger edits existed — but a reader cannot tell from the record. Strictly cosmetic
against a document that is otherwise unusually faithful; noted because the header's
claim is slightly stronger than the document delivers.

---

## §6. Strengths

Accurate praise, so the rest of the feedback is trustworthy:

- **The `### Phase 75` contract block is genuinely excellent.** §A's stated rule
  matches `crates/envoy-config/src/matcher.rs:39-70` cell for cell — all four
  `present_match` cells, all six value modes × {absent, present}, and the
  empty-value-counts-as-present claim. §C is a faithful, error-free transcription
  of the nine-sink MEASURED table in `SPEC.md` §2.3. §B is derivable from §A. §E's
  four-cell Trap-A table is correct against both engines. All eight subsections
  §A–§H are present and substantive (12–26 lines each), correctly placed in
  ascending phase order and properly closed by `---`.
- **The block cites NO `path:line` numbers at all** — only bare paths. Given this
  repo's chronic citation drift (which bit twice mid-session at state-3), that is a
  deliberate and correct choice, and it is the right pattern for the project to
  keep.
- **§D is the standout.** It records not just the guard but the *fact that the
  guard is in-process only* — "this arm ORDER is guarded ONLY in-process, so the
  differential fixtures cannot catch a regression in it" — and then explicitly
  warns that the two mutations are DISTINCT and hit different cells. That is a
  future session's trap disarmed in advance, and it is exactly the kind of thing
  that is invisible unless someone bothers to write it down.
- **ADR-0162 is a model of the doctrine working.** The state-3 session discovered
  the PLAN's own specified mutation did not witness the fixture it was supposed to,
  measured the correct one, corrected the guard count on a fresh measurement, and
  fired an ADR — instead of quietly using the mutation that "worked". The
  `PROGRESS.md` mutation section reads the failure TEXT, discards a
  `not accept-ready within 10s` RED as a startup-race flake that never reached an
  assertion, re-runs it, and records an unmutated control from the same worktree.
  That is the standard this project sets, met.
- **The M74-31 consumption is complete and disciplined.** All four live sites
  corrected, zero missed (verified by a sweep over a 654-file live pathspec), the
  inherited "FIVE" figure refuted to FOUR by live measurement and recorded in
  ADR-0161, and the append-only "FIVE" at `74/REVIEW.md:1269` correctly left alone.
  Critically, the two NEW fixtures do not propagate the non-sequitur — all six new
  artifacts phrase the causality correctly.
- **Task 8 closed all five in-scope 75.1 findings properly**, and N-2 was closed
  *better* than the review suggested (pointing at the unique `### Phase 75` heading
  rather than the ambiguous "§C", of which the file has nine). M-1's fix was
  deliberately made line-number-free per the review's own suggestion — the right
  lesson learned from a citation class that had gone stale three times.
- **The `§6.2` empirical reconciliation is exemplary.** Six corrections to the SPEC,
  each measured, each recorded in ADR-0161, including refuting the SPEC's own
  "five-site" figure and DECLINING two optional extras with stated reasons rather
  than silently dropping them.
- **Per-side config hygiene is exact.** Both fixtures differ by precisely the four
  sanctioned deltas; the `header_filter` body, log format, route table, `node:` and
  `codec_type` are byte-identical across sides.
- **The record matches the tree on every structural claim that could be
  re-derived.** All 9 planned tasks landed, one commit each; no task commit touched
  a file outside `PLAN.md`'s `## File Structure`; the only four files outside it
  (`DECISIONS.md`, `ROADMAP.md`, `STATE.md`, `STATE_HISTORY.md`) are touched solely
  by the lifecycle-ledger commits, never by a task commit. Every quoted `diff -u`
  block reproduces byte-identically; the mutation REDs' panic locations
  (`headermatcher_absence_accesslog.rs:63:10`, `..._present_polarity.rs:61:10`)
  land exactly on the `.expect("fixture green")` lines in 64- and 62-line files.
  **The §6.1 size gate is honest** — ~760 projected against ~998 landed on the
  plan's own like-for-like basis, a disclosed ~31% overshoot still ~33% under the
  ~1500 gate. Every census figure re-derives exactly.
- **Two host-flake traps were caught rather than papered over**, and two live
  defects were SELF-DISCLOSED rather than hidden (`PROGRESS.md:731-737`,
  `:1290-1301` — the phase-72 "three" and the h2spec observation are both this
  phase's own reporting). A session that surfaces its own open problems is doing
  the job the doctrine asks of it; I-4 exists because the sweep was incomplete, not
  because it was concealed.
- **Gate (c) was the one that could have been overclaimed, and was not.** The
  state-4 record proved with `--nocapture` that the local h2spec gate self-skips,
  explicitly refused to read the local green as a conformance pass, and rested the
  verdict on the criterion the plan actually states. That is the correct call, and
  §3.2 vindicates it.
- **Nothing was weakened.** `0081` untouched except comments (proven by a filtered
  diff printing empty), `known-failures.txt` not trimmed, no `on_header_missing`
  added to `0081`/`0082`, no append-only ledger edited, no fixture probe or
  `expect_logged` value changed.

---

## §7. Reported but DROPPED or DOWNGRADED after re-verification

Recorded so they are not re-raised:

1. **"The h2spec gate is vacuous in CI."** Carried forward from state-4 as a
   suspicion. **REFUTED** on three independent lines of evidence (§3.2). Only the
   LOCAL half stands, and it becomes CF-75-3 rather than a 75.2 finding.
2. **"The fixtures are not load-bearing."** Considered and rejected. They ARE
   load-bearing for the pre-75.1 → post-75.1 transition; ADR-0162's measured RED
   (`envoy_rust=2, envoy=1`) is genuine, and I confirmed the mechanism directly
   from the engine source. I-1 is a narrower and different claim.
3. **"The `crates/` edits changed behavior."** Disproved by a filtered diff that
   prints empty (§0).
4. **"The declined third P1 fixture is a coverage hole."** The decline was
   explicitly adjudicated at the PLAN-write (`PLAN.md:64`) with stated reasons, P1
   is pinned in-process by two named tests and cross-proxy on the route path by
   `0083`, and it is documented as §D of the contract block. **Sound; not a
   finding.**
5. **"`0083` should have been re-run."** Out of scope — 75.1 is `done` and FROZEN,
   and `0083` was `1 passed` in both state-4 sweeps.

---

## §8. What this review did NOT do

- It did **NOT** re-run the §7.5 gate as its state. Gates (a)–(e) are recorded
  GREEN at state-4 with every command output quoted verbatim; §4 re-derives the
  headline numbers only.
- It did **NOT** re-open, re-verify, or re-grade sub-phase **75.1**, nor edit any
  `75.1/` artifact or the FROZEN parent `75/SPEC.md`.
- It did **NOT** flip ROADMAP row `75.2` or parent row `75` — both belong to the
  state-6 close-out, which this verdict defers.
- It did **NOT** fix any finding. Every fix belongs to the §5.2 state-3 re-entry.
- It did **NOT** create a `stop` file. The mission is not complete.
- It changed no `crates/` code, no fixture, no test and no `ci.yml`; its only
  tree-mutating act was a scratch `git worktree`, removed, with the main tree
  re-confirmed clean at `1f05c2d`.

---

## §9. Carry-forward disposition after this review

**NEW, opened by this review:**

- **CF-75-3** — **the h2spec conformance gate's LOCAL silent self-skip, plus CI's
  missing `--no-fail-fast`.** `h2spec_runner.rs:20-32` returns SUCCESS when the
  binary is absent and `cargo test` swallows the notice, so a local `3 passed` is
  two parser unit tests plus a no-op; and `ci.yml:67` (`cargo test --workspace`,
  no `--no-fail-fast`) means a RED CI run may abort before the conformance binary
  runs at all — verified on two sampled failed runs, neither of which contains any
  `h2spec_pass_rate_gate` line. **The gate is NOT vacuous in CI** (§3.2). Owner =
  its own phase, scoped as local-developer-feedback hardening (make the skip loud,
  `#[ignore]`d, or env-var-gated) plus the `--no-fail-fast` change. PRE-EXISTING
  since `5914b14` (2026-05-03). ADR-0163.

**CLOSED by this sub-phase (confirmed on disk):** **M74-31** (four live sites, zero
missed); the sub-phase-75.1 review's **M-1, M-2, M-3, N-1, N-2**. **M-5** and
**N-3** correctly need no fix (landed historical artifacts; editing them
retroactively would violate D-3.5); **N-4** is correctly record-only.

**BANKED, not fixed (confirmed present in `BEHAVIOR_CONTRACT.md`):** **CF-72-2**
(now three members, phase-72 `**§D`) and **CF-75-1** (phase-72 `**§G`). Owner = a
future `HeaderMatcher` wire-shape-parity phase, which should decide them together.

**OPEN and explicitly OUT OF SCOPE:** **CF-75-2** (upstream comma-joins duplicate
header values before value matching; envoy-rust matches only the FIRST occurrence).
MEASURED, PRE-EXISTING, spans all six value modes across all five consumers, needs
its own phase. Note M-1 above: it is recorded in `STATE.md`, **not** in
`BEHAVIOR_CONTRACT.md`, contrary to what both new READMEs say.

**Untouched, carried forward:** **M71-6** (DECLINED at the PLAN-write, ADR-0161,
still open), CF-74-1/2/3/4/6, CF-73-1, N73-R2, M73-R1/M73-R2, M71-3, M71-7/8,
M70-R4/R9, M69-A..I, CF-69-1/2/3/5, M68-1, M-1 (the older one — see N-5),
CF-67-3/5/6/7, M74-3..M74-14, M74-16, M74-17/18/20/21/22/26/27/28/29, M74-30 and
M74-32..M74-39, the older Minors in `67.3/SPEC.md` §10, and the
HTTP-filters-family (1)–(4) in `STATE_HISTORY.md`.

**Joining the list from this review:** I-1 through I-4 and M-1 through M-6 are
MUST-FIX / SHOULD-FIX for the §5.2 re-entry, not carry-forwards. **M-7 is a
recorded reviewer DECISION to take no action** (append-only), and **M-8 was REPAIRED by this
review itself**. N-1 through N-7 are
record-only or lineage-wide phrasing; **N-4, N-5 and M-7 are explicitly NOT to be
retroactively "fixed"**.

**Why ADR-0163 fired.** A genuinely new decision arose that a future session must
not have to re-derive: the state-4 record's claim that the h2spec gate "may be
VACUOUS" is REFUTED for CI on measured evidence, and only its local half survives,
banked as CF-75-3. Recording this is load-bearing — the unrefuted claim would
otherwise propagate as a standing trap and could motivate a wasted phase. No other
ADR fired: the eight findings are defects to fix, not decisions to record.

---

## §10. Next state

**§5.2 STATE-3 RE-ENTRY for sub-phase 75.2** — a **SEPARATE session** per §5.1 and
ADR-0127. Per `BOOTSTRAP_PROMPT.md` §5.2 the re-entry point is **step 3, NOT step
4**: it is resuming implementation under TDD, not merely re-verifying.

That session must:

1. Fix **I-1** — give each probe in `0084` and `0085` a distinct `path:`, widen
   both route tables to `match: { prefix: "/" }`, and update both READMEs' and
   `BEHAVIOR_CONTRACT.md` §G's expected-line text. **TDD applies:** re-run the two
   mutations of §2.2/§2.3 in a scratch worktree and show they now go **RED** —
   that is the RED evidence for this fix, and it is the whole point of the change.
2. Fix **I-2**, **I-3**, **I-4** and **M-1**–**M-4** — all documentation, all in
   LIVE documents.
3. Close **M-5** and **M-6** by DISCLOSURE in `PROGRESS.md`, not by re-running
   anything: for M-5 state what the mutated file actually looked like (or that it
   can no longer be determined, the worktree having been removed); for M-6 either
   quote the missing failure text from a fresh isolation run of
   `access_log_rcd_upstream_reset` or state explicitly that it was adjudicated by
   family membership.
4. Take **no action** on **M-7** (recorded decision) or on **M-8** (already repaired
   by this review; `STATE_HISTORY.md` is whole again) — the decision is recorded above; `ROADMAP.md` is
   append-only and the refutation lives in ADR-0161.
5. Leave **N-1**–**N-7** alone, or fix only N-1/N-2/N-3/N-6/N-7 opportunistically.
   **N-4 and N-5 must NOT be retroactively edited.**
6. Append to `PROGRESS.md`; fire an ADR only if a genuinely new decision arises.

It must **NOT** flip ROADMAP row `75.2` or parent row `75`, re-run the §7.5 gate
(that is the state-4 RE-VERIFICATION, a separate session after the re-entry),
re-open sub-phase 75.1, edit any `75.1/` artifact or the frozen `75/SPEC.md`, widen
75.2 into CF-75-3 or CF-75-2, or create a `stop` file.

After the re-entry lands, the sequence is: **state-4 RE-VERIFICATION** → **state-5
RE-REVIEW** → **state-6 CLOSE-OUT** (at which ROADMAP row `75.2` AND parent row
`75` both flip `done`). Each is its own session.

Ledger head after this review: **ADR-0163**; next available **ADR-0164**.

---

# §5 state-5 RE-REVIEW — the phase's CURRENT verdict

> **What this section is.** The §5 **state-5 RE-REVIEW** for sub-phase 75.2, run in
> a SEPARATE session from the §5.2 state-3 re-entry and from the §5 state-4
> RE-VERIFICATION that preceded it (§5.1; ADR-0127 — the context that wrote or
> gated an artifact must not grade it). Written for a stranger with zero prior
> context (D-3.4).
>
> **The earlier `# Sub-phase 75.2 … §5 state-5 CODE-REVIEW` section at the head of
> this file is HISTORY and is left exactly as landed** (append-only, D-3.5). Its
> CHANGES-REQUESTED verdict was discharged: `PROGRESS.md`'s
> `# §5.2 state-3 RE-ENTRY` section closes its §10 charter, and `PROGRESS.md`'s
> `# §5 state-4 RE-VERIFICATION` section re-ran the whole §7.5 gate over the
> result. **The verdict below supersedes it and is the phase's current one.**
>
> **Session start state (verified on disk, not trusted from the handoff):**
> `git status --porcelain` clean; branch `main`; `HEAD` ==
> `1da066280f20c29ad1b320ce9a05de8821f4f06d`; `git fetch origin --prune` showing
> `origin/main` at the SAME SHA. CI on that FULL 40-char SHA confirmed GREEN — run
> **`30294110868`**, `completed`/`success`, both jobs at FULL step counts **15**
> (`build + test + lint`) and **13** (`fuzz`), so no runner-starvation signature
> (`steps: 0` + `runner_name: ""`).
>
> **State detection:** `SPEC.md`, `PLAN.md`, `PROGRESS.md` AND `REVIEW.md` all
> exist; `REVIEW.md`'s verdict is CHANGES-REQUESTED, but `PROGRESS.md` carries BOTH
> the re-entry section that closes its charter AND the state-4 RE-VERIFICATION
> section that re-gated the result — so the re-entry is SPENT, gates (a)–(e) are
> landed GREEN, and **gate (f) is the only open gate**. §5 state 5 exactly.

## §R0. Scope, method, and what was reviewed

**Reviewed:** (i) the two new differential fixtures `0084` /
`0085` and their entrypoints as they stand at HEAD; (ii) the §5.2 re-entry's twelve
fix files (on disk since `da06059`); (iii) the `BEHAVIOR_CONTRACT.md` Phase 75
block this sub-phase authored; (iv) whether the state-4 RE-VERIFICATION's gate
adjudication — **ADR-0164** above all — is sound and was applied honestly.

**Method.** Four READ-ONLY review dimensions were fanned out as subagents
(`superpowers:dispatching-parallel-agents`), each given full zero-context
instructions and forbidden to write or to run `cargo`. **Every finding they
returned was RE-VERIFIED on disk by this session before being recorded here, and
two were CORRECTED downward in the process** (see §R5). The decisive measurements
were made by this session ITSELF in its own scratch `git worktree`, created
`--detach` at HEAD (memories `mutation-checks-collide-with-parallel-subagents`,
`worktree-subagents-get-stale-base`). **The §7.5 gate was NOT re-run** — it is
landed and GREEN, and re-running it is not this session's job.

**Not reviewed / deliberately out of scope:** sub-phase 75.1 (landed, `done`, and
the PREMISE of 75.2), the frozen parent `75/SPEC.md`, any `75.1/` artifact,
CF-75-3 and CF-75-2 (both open and owned by their own future phases).

## §R1. Verdict

## **CHANGES-REQUESTED — 0 Critical, 2 Important, 7 Minor, 7 Nit. Re-entry is §5.2 step 3, NOT step 4.**

**The engineering is sound and the gate is genuinely green.** No code is wrong, no
fixture is weakened, no gate is falsely green, and the ADR-0164 judgment call this
review was pointed at is **correct**. The blocking issue is elsewhere: this
sub-phase's own contribution to `BEHAVIOR_CONTRACT.md` — the project's canonical
equivalence reference (`BOOTSTRAP_PROMPT.md` §4.1 invariant 5) — carries a claim
labelled **MEASURED** that this review **refutes by measurement**, and the practical
conclusion it draws from that claim is the *opposite* of the truth. A future
refactorer reading §D is told their differential suite cannot protect them, when in
fact two fixtures catch the exact regression §D warns about. That is a must-fix in a
document whose whole purpose is to be trusted without re-derivation.

Everything else is Minor or Nit: factual slips, an over-narrow verification recipe,
and one genuine coverage carry-forward.

## §R2. MEASURED evidence produced by THIS review (scratch `git worktree`)

Worktree created `--detach` at HEAD `1da0662`, `git status --porcelain` clean at
creation, with its own `CARGO_TARGET_DIR` so it shares no cargo lock and no build
artifacts with the main tree. Removed with `git worktree remove --force` at the end;
**only this session's own worktree was removed** — the four
`.claude/worktrees/agent-*` belonging to the parallel workstream were left
untouched and verified present afterwards by `git worktree list`.

### §R2.1 Unmutated control FIRST

```
$ cargo build -p envoy-bin
   Compiling envoy-bin v0.0.0 (…/wt-state5/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.12s
$ cargo test -p differential --test headermatcher_absence_accesslog
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.19s
$ cargo test -p differential --test headermatcher_absence_accesslog_present_polarity
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.17s
```

`0 filtered out` is asserted explicitly — `0 passed; N filtered out` exits 0 and is a
FALSE green (memory `cargo-test-p-name-false-green-filtered-out`).

### §R2.2 Mutations P and X — the re-entry's TDD RED, INDEPENDENTLY REPRODUCED

**Mutation P — polarity inversion.** `matcher.rs`, `v.is_some() == *want_present` →
`!=`. **Mutation X — drop the XOR.** `mode_result ^ self.invert_match` →
`let _ = self.invert_match; mode_result`. Each was applied alone, `envoy-config` and
`envoy-bin` were confirmed to RECOMPILE (`Compiling envoy-config` / `Compiling
envoy-bin` present on every build — memory `mutation-check-needs-forced-rebuild`),
and the mutation was **re-grepped as still present after the run**.

```
--- 0085 under mutation P ---
fixture green: access log byte-exact mismatch: line 0 not byte-identical: envoy="STATUS=200 PATH=/absent" envoy-rust="STATUS=200 PATH=/present"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.14s

--- 0084 under mutation X ---
fixture green: access log byte-exact mismatch: line 0 not byte-identical: envoy="STATUS=200 PATH=/valmiss" envoy-rust="STATUS=200 PATH=/valmatch"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.11s
```

**Both reproduce the re-entry's recorded failure text CHARACTER FOR CHARACTER**
(`PROGRESS.md:1446`, `:1474`). One line on each side in both cases, so the COUNT
assertion still passed and only the byte compare failed — which is precisely the
regression class the pre-I-1 shared `path: /x` could never catch. **The I-1 fix is
load-bearing and the re-entry's TDD RED is real, not narrative.**

### §R2.3 Mutation H — the arm-ORDER hoist. **This REFUTES `BEHAVIOR_CONTRACT.md` §D.**

`(_, None) => return false` moved to the TOP of the `match` (keeping `return`) —
the mutation §D describes.

```
--- in-process ---
test result: FAILED. 645 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out
--- 0084 ---
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.27s
--- 0085 ---
fixture green: envoy-rust emitted 0 access-log lines but 1 were expected to be logged; lines: []
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 18.21s
--- 0083 (ROUTE path) ---
fixture passes: probe p07-absent-keeps-GUARD
Caused by:
    byte-exact body mismatch
      upstream: [112, 48, 55, 61, 77, 65, 84, 67, 72]      ("p07=MATCH")
      subject:  [112, 48, 55, 61, 78, 79, 77, 65, 84, 67, 72]  ("p07=NOMATCH")
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.18s
--- restored control, SAME worktree ---
test result: ok. 649 passed; 0 failed  (envoy-config --lib)
test headermatcher_absence_accesslog_present_polarity ... ok
test headermatcher_absence_parity_fixture ... ok
```

**§D's `645 passed; 4 failed` and its `0084` GREEN are both CONFIRMED. Its
`0085` GREEN is FALSE, and so is the conclusion drawn from it.** The mechanism is
plain from the rule: with `(_, None)` hoisted, an ABSENT header returns `false` for
*every* mode — including `present_match: false`, whose absent cell must MATCH. That
is `0085` probe 2 (`/absent`, `expect_logged: true`), so envoy-rust emits zero lines
against upstream's one and the COUNT assertion fails. It is also `0083` p07 and p12.
The 18.21 s runtime is the `ACCESS_LOG_FLUSH_WAIT` timeout waiting for a line that
never comes — consistent with, and corroborating, the zero-line count.

Both REDs quote a **real assertion**, and both have an unmutated control from the
same worktree (memory `mutation-red-needs-unmutated-control`).

### §R2.4 The comment-only claim, verified by a STRONGER method

The state-4 recipe (`PROGRESS.md:2424`) runs over `29055a5..HEAD`. Over the TRUE
sub-phase span — base `3f0ec89` (75.1's landing) — the same recipe prints SIX
non-empty lines, because `grep -vE '^[+-]\s*///'` strips only DOC comments and lets
`//` line comments through. So this session proved the claim a different way:

```
$ for f in $(git diff --name-only 3f0ec89..HEAD -- crates/); do
    diff <(git show 3f0ec89:$f | grep -vE '^\s*(///|//!|//)' | sed 's/[[:space:]]*$//') \
         <(git show HEAD:$f    | grep -vE '^\s*(///|//!|//)' | sed 's/[[:space:]]*$//')
  done
SAME (comment-stripped): crates/envoy-config/src/bootstrap.rs
SAME (comment-stripped): crates/envoy-config/src/matcher.rs
```

**The claim HOLDS over the full phase: sub-phase 75.2 changed ZERO executable
`crates/` lines.** The recipe, not the claim, is the defect (finding I-2). Note the
recipe errs CONSERVATIVELY — it can false-FAIL on a `//` comment change but cannot
false-PASS on a code change, since a code line never begins with `///`.

## §R3. ADR-0164 — the judgment call this review was pointed at: **SOUND**

The handoff named this the highest-value thing to grade, so it is graded on its own
terms rather than on whether the gate says GREEN.

**The call is correct, and it is under-argued rather than over-argued.** Verified
independently, on disk:

- **Leg (iv) holds, and is decisive.** `http1_router_upstream_fixture` reads
  `tests/fixtures/0008-http1-router-upstream`; `xds_file_based_rds_fixture` reads
  `tests/fixtures/0028-xds-file-based-rds`. Neither fixture contains
  `header_filter`, `present_match`, `invert_match` or `exact_match`. And 75.2's
  entire non-docs footprint is: two `crates/` files (comment-only, §R2.4), fixtures
  `0081` (comment-only — its `expectations.yaml` diff filtered of `#` lines is
  EMPTY), `0084`, `0085`, and three test entrypoints. **Neither RED binary is in
  that set.** A 75.2 regression could not express itself through either.
- **The CI cross-check is real.** Re-derived independently on the FULL 40-char SHA:
  run `30294110868` on HEAD is `162 binaries / passed=2105 / failed=0`, and the log
  contains **zero** `test result: FAILED` lines.
- **ADR-0164 scopes itself correctly.** It states plainly that the two binaries are
  not in `PLAN.md`'s list rather than glossing it; it says explicitly *"it licenses
  no general 'if it passes in isolation it is a flake' rule"*; and it records three
  rejected alternatives with reasons, satisfying D-3.5.
- **The stable-core / floating-tail decomposition is well-founded** and the
  discriminator really is in `PLAN.md:1602`, quoted byte-exactly.

**One caveat, recorded as Nit N-7 rather than as a defect:** the ADR's Consequences
paragraph promotes the four-part test into a standing rule for *all* future
gate-(b) adjudications, and outside this phase legs (i)–(iii) are weaker than they
look — a load-dependent regression (one that slows envoy-rust's boot under parallel
load) would never reach an assertion, would pass in isolation, and would be absent
from some sweeps. Leg (iv) is what carries the test, and here it is carried by a
measured EMPTY executable diff, not by inspection. The ADR should bind leg (iv) to
that measurement.

## §R4. Findings

### §R4.1 Critical

**None.**

### §R4.2 Important (MUST-FIX before this sub-phase can close)

#### I-1 — `BEHAVIOR_CONTRACT.md` §D states a MEASURED claim that measurement refutes, and draws the opposite of the true conclusion

`docs/envoy-rust/BEHAVIOR_CONTRACT.md:2831-2839`:

> `MEASURED at 75.2: it turns FOUR in-process assertions RED (…) while leaving every value-mode assertion green **and both access-log fixtures `0084`/`0085` GREEN**. That last fact is the point: this arm ORDER is guarded ONLY in-process, so the differential fixtures cannot catch a regression in it. **Any future refactor of the arm ORDER must preserve it.**`

**MEASURED by this review (§R2.3): `0085` goes RED under that mutation, and so does
`0083`.** The `FOUR in-process RED` half and the `0084` GREEN half are both correct;
the `0085` GREEN half is false, and the generalization from two access-log fixtures
to *"the differential fixtures"* is false twice over.

The root error is in the sentence above it, at `:2842-2843`: *"Hoisting the arm
(keeping `return`) breaks **P1**, leaves **D1** correct."* Hoisting breaks P1
(`present_match: true` + invert + absent) **and also the `present_match: false` ×
absent MATCH cell** — which is exactly `0085` probe 2 and `0083` p12. §D names only
the first.

**Why it matters.** `BEHAVIOR_CONTRACT.md` is the canonical reference
(`BOOTSTRAP_PROMPT.md` §4.1 invariant 5: a divergence between contract and observed
behavior is resolved by fixing one or the other, *never* left silently). §D is
precisely the warning a future refactorer reads before touching the arm order.
Telling them the differential suite cannot catch a regression here invites them to
read a green sweep as licence, and to dismiss a `0083`/`0085` RED as unrelated
noise. It also contradicts this sub-phase's own
`tests/fixtures/0085-…/README.md:179-183`, which correctly says the P1 guard is
pinned *"in-process … **and cross-proxy on the route path by `0083`**"*.

**Fix.** Correct §D to the measured behavior: the hoist breaks BOTH the P1 cell and
the `present_match: false` × absent cell; it leaves `0084` GREEN (which uses
`exact_match`, a value mode the hoist does not change) but REDs `0085` and `0083`.
Then state the true coverage conclusion — *the arm ORDER is guarded in-process AND
cross-proxy, by `0083` p07/p12 and by `0085` probe 2; the one fixture that cannot
catch it is `0084`.* Quote this review's §R2.3 output as the measurement.
**`STATE.md:28`'s Standing-traps clause (a) carries the same false text and must be
corrected in the same session.** `DECISIONS.md:2462` (ADR-0162) also carries it —
that one is append-only and must be left as landed; the correction is stated
forward, not retro-edited.

#### I-2 — the comment-only proof is scoped to the tail, and its filter is `///`-only, so the quoted recipe FAILS over the span its own prose claims

`docs/envoy-rust/phases/75.2-headermatcher-absence-accesslog/PROGRESS.md:2417-2432`.
The heading and lead sentence make a **whole-sub-phase** claim — *"75.2 changed no
`crates/` behaviour: its three `crates/` edits (two at state-3, one at the §5.2
re-entry) are ALL comment-only"* — while the evidence is
`git diff -U0 29055a5..HEAD`, which covers only the re-entry. Run the identical
pipeline over the real phase base `3f0ec89..HEAD` and it prints six non-empty lines,
because the filter strips `///` but not `//`.

The record IS careful in one place (`:2433`, *"No executable `crates/` line changed
**since the state-5 head**"*), but the claim actually propagated forward is the
whole-phase one, and `STATE.md:28` carries the recipe with **no base at all**:
*"re-prove it with a filtered diff that must print EMPTY."* A future session
choosing any base earlier than `29055a5` gets a non-empty result and a spurious
alarm.

**The claim itself is TRUE** — proven in §R2.4 by comment-stripped whole-file
compare over the full span. The recipe is the defect.

**Fix.** Re-anchor on `3f0ec89..HEAD`, widen the filter to
`'^[+-]\s*(///|//!|//)'`, and state the base explicitly wherever the recipe is
propagated. Better still, cite the whole-file comment-stripped compare as primary
evidence (memory `fmt-only-check-needs-whole-file-compare` makes the same point
about fmt-only claims) and keep the diff filter as a corroborator.

### §R4.3 Minor

#### M-1 — `command_operator.rs` was never edited by this sub-phase

`PROGRESS.md:1798` — *"The three `crates/` files this sub-phase touched"*, followed
by a `touch` of `bootstrap.rs`, `matcher.rs` and
`crates/envoy-accesslog/src/command_operator.rs`. Measured:
`git log --oneline 3f0ec89..HEAD -- crates/envoy-accesslog/src/command_operator.rs`
is **empty**. 75.2 edited exactly **two** `crates/` files, in two commits (`939a14c`
touching both, `da06059` touching `bootstrap.rs`) — which is three *edits* across
*two files*, and `:2419`'s *"three `crates/` edits"* is right under that reading.
The `touch` is harmless (it only widens the forced rebuild) but the sentence is
false. Fix: *"the two `crates/` files 75.2 edited, plus `command_operator.rs` to
widen the forced rebuild."*

#### M-2 — internally inconsistent count of caught-wrong censuses, inside one commit

`PROGRESS.md:2447` says *"Inherited censuses have been caught wrong **five** times on
this phase"*; `STATE.md:28`, landed in the **same commit `1da0662`**, says *"**SIX**
have now been caught"* and enumerates six. The state-4 section's own gate-(d)
correction is itself one of them, which argues for six. Fix: reconcile to one figure.

#### M-3 — the `awk -F'|' $5` trap narrative names seven rows; only two actually misclassify

`PROGRESS.md:2467-2471` (and `STATE.md:28`) — *"A naive `awk -F'|' $5` pass
FALSE-REPORTS rows `36`/`38`/`39`/`52`/`54`/`66`/`70` as not-done."* Measured: all
seven carry unescaped `|`, but in rows 36/38/39/52/54 the stray pipes sit AFTER the
status column, so `$5` still resolves to `done`. **Only rows 66 and 70 shift**, and
the naive census yields `done=100`, not 95. The corrective conclusion (split on
`' | '`, status in field 4, 104/102) is correct and unaffected. Fix: say *"rows 66
and 70 are misclassified; 36/38/39/52/54 also carry unescaped pipes, but only after
the status column."* **Do NOT "fix" any of the seven rows — `ROADMAP.md` is
append-only.**

#### M-4 — the CI-total recipe cannot see a failing CI binary, so its `failed=0` is true by construction

`PROGRESS.md:2027`:

```
gh run view 30264483379 --log | grep -o 'test result: ok\. [0-9]* passed; [0-9]* failed' | awk '{p+=$4; f+=$6} …'
```

The pattern is hard-anchored on `test result: ok\.`, so a `test result: FAILED.`
line is discarded before `awk` sees it: `failed=0` is a property of the regex, and
the `binaries:` count would silently fall below 162 rather than flag anything. It
did not bite — this review re-derived the log independently and all 162 result lines
are `ok.` — but the recipe is not self-validating, and it is the one the whole
`local passed + failed == CI passed` identity leans on. Fix:
`grep -oE 'test result: (ok|FAILED)\. …'` and assert the binary count equals 162
separately.

#### M-5 — three sibling access-log fixtures carry the exact I-1 blindness; §G's new rule is scoped "future" only

`BEHAVIOR_CONTRACT.md:2900` closes the new §G with *"**Apply the same discipline to
any future `http1_access_log_byte_exact` fixture.**"* Measured census of the
existing ones (probe paths vs the rendered format):

| fixture | probe paths | format discriminator | blind? |
|---|---|---|---|
| `0076` | `/log`, `/nolog` | path | no |
| `0077` | `/direct`, `/nowhere` | path | no |
| `0078` | `/x`, `/x` | none — `STATUS=…  PATH=…` only | **YES** |
| `0079` | `/x`, `/x` | none | **YES** |
| `0080` | `/x`, `/x`, `/x` | none | **YES** |
| `0081` | `/x` ×3 | `M=%DYNAMIC_METADATA%` renders `-` / `1` / `2` | no |
| `0082` | `/x`, `/x` | `M=` renders `-` vs `1` | no |
| `0040` | `/`, `/` | `ua=` / `xff=` differ per probe | no |

`0078` is the sharpest case and the direct stencil `0084` was copied from: two
probes, `expected_logged_count == 1`, both rendering `STATUS=200 PATH=/x`, so a
verdict-inverting regression in the very engine `0084`/`0085` exist to protect swaps
which probe survives, leaves the count at 1, and passes GREEN. **All three of the
blind fixtures exercise `header_filter`** — `0078` directly, `0079`/`0080` as leaves
inside `and_filter` / `or_filter` composition arms
(`0079/envoy.yaml:26-27`) — so all three sit on the very engine this sub-phase's two
fixtures exist to protect. Nothing 75.2 shipped
is wrong here, so this is **not** a re-entry obligation — but the defect I-1 named is
still live in three corpus fixtures and should be a RECORDED carry-forward rather
than an implicit "future fixtures only" waiver. Fix: open **CF-75-4** naming
`0078`/`0079`/`0080`, and adjust §G's closing sentence to say the rule applies to
existing fixtures too, with those three named as outstanding.

#### M-6 — `PROGRESS.md:1720` points at evidence that is not in the document

> *"The forward obligation stands and was honoured this session: the ADR-0035 delta
> check **below** was run against the FULL superseded set INCLUDING that bullet."*

There is no delta check below — lines 1725-1755 are `## What this session did NOT
do` and `## Next`. The numbers live only in the `da06059` commit message. **The
claim is TRUE** (independently re-derived: 13 removed `STATE.md` lines, each at
delta +1 in `STATE_HISTORY.md`, 0 history lines lost, including the
`### Doctrine reminders` §5.1 bullet M-8 warned about). But disk is the only memory
across sessions, and M-8's whole lesson was that an orphaned relocation is
self-concealing. Fix: append the delta-check block to `PROGRESS.md`, or reword
"below" to "recorded in the commit message".

#### M-7 — `STATE.md`'s own stated line-length range excludes the line stating it

`STATE.md:28` — *"`STATE.md` top-section blocks are single 3000-15000 char lines"*.
Measured: that line is **16577** characters, and it sits in the `## Next expected
skill` top section. The figure 16577 is itself correctly recorded at `:29`, so the
session measured it and then left the adjacent range stale. Fix: widen the range, or
better, drop the number and say "single multi-thousand-character lines — measure
before editing".

### §R4.4 Nit

- **N-1 — two elided COMMAND lines inside blocks presented as verbatim.**
  `PROGRESS.md:2231` (`$ for n in 0078 … 0085; do …`) and `:2456`
  (`$ awk -F' \\| ' '/^\| *[0-9]/ {n++; s=$4; …}'`) are not re-runnable as shown,
  and the `#![forbid(unsafe_code)]` census at `:2461` has no `$` command line at all
  inside a fenced block that reads as output. The section's own preamble
  (`:1774-1778`) declares only two mechanical elisions. Every OTHER recipe in the
  section is executable — this review ran them all and all reproduce. Note N-7 of
  the first review already declared an abbreviated-COMMAND elision class for the
  re-entry section; the state-4 section did not inherit that declaration.
- **N-2 — "this exact HEAD SHA" is the ADR's own parent.** `PROGRESS.md:2023` and
  `DECISIONS.md:2429` cite run `30264483379`, which is CI on `a0384b2` — correct at
  the moment of measurement (the gate necessarily runs before its own commit), but a
  reader at HEAD `1da0662` querying that SHA gets a different run id. Both claims
  survive: CI on `1da0662` is run `30294110868`, also `162 / 2105 / 0 / success`.
  One clause would fix it.
- **N-3 — ADR-0164 does not cite the on-disk prior art that reached the same
  decomposition.** `75.1/PROGRESS.md:1326` and `:1360` already name
  `### FAMILY 1 — deterministic in isolation` and
  `### FAMILY 2 — passes in isolation`, and `:1377` already adjudicated two binaries
  outside the flake list on a near-identical test;
  `74/PROGRESS.md:1943` did the same. This *understates* the finding's support — a
  rule three independent sessions converged on is far more credible than one
  session's call. Cite them in the ADR's Context. (ADR-0164 is landed; state this
  forward rather than editing it.)
- **N-4 — "0 Critical, 4 Important, 7 Minor" survives in two landed artifacts.**
  `DECISIONS.md:2451` (ADR-0163) and `STATE_HISTORY.md:42/4590/14250` say 7 Minor
  where `REVIEW.md:49` says **8** and §5.3 defines M-1..M-8. Both are append-only and
  must NOT be edited; the correction is already carried forward in `STATE.md`'s
  traps. Recorded so a future census does not re-inherit the 7.
- **N-5 — `0084/expectations.yaml`: "ALL THREE halves".** Read "all three parts".
- **N-6 — `SPEC.md` §2.2 citations have drifted.** `SPEC.md:63` cites
  `crates/envoy-accesslog/src/filter.rs:139` (actual **`:141`**) and `:65` cites
  `crates/envoy-config/src/matcher.rs:69` for the `HeaderMatch` impl (actual
  **`:82`**; `:69` is now `mode_result ^ self.invert_match`). Both drifted from this
  sub-phase's own comment additions. **`SPEC.md` is a landed state-1 artifact and
  must NOT be retro-edited** (D-3.5) — recorded so a future reader meets the drift
  rather than being misled by it. Note deliberately that **neither fixture README nor
  either entrypoint cites a single line number**, which is the correct pattern and is
  why the drift never reached the durable artifacts.
- **N-7 — ADR-0164's leg (i) rationale can be sharpened, and its Consequences
  paragraph over-generalizes.** Two points. (a) The stated ground — *"a RED that
  never reached an assertion carries no information about matching semantics"* — is
  weaker than the evidence actually available: the failure text names **upstream
  Envoy** failing to become accept-ready, and envoy-rust code cannot cause the
  *reference* proxy's container to fail to start. That is a much stronger
  discriminator and it is sitting unused in the quoted text. (b) The Consequences
  paragraph promotes the four-part test to a standing rule; legs (i)–(iii) are not
  sufficient without leg (iv), and leg (iv) should be bound to a MEASURED artifact
  (the phase's comment-stripped executable diff, §R2.4) rather than to
  per-binary inspection. See §R3.

## §R5. Reported by a review dimension but CORRECTED or DROPPED after re-verification on disk

Recorded because the correction is itself evidence that the fan-out was re-verified
rather than transcribed.

1. **"Five sibling fixtures are blind" — CORRECTED to THREE.** A dimension listed
   `0078`/`0079`/`0080`/`0081`/`0082`. Re-verified on disk: `0081` and `0082` render
   `M=%DYNAMIC_METADATA(com.example:k)%`, which resolves to `-` / `1` / `2` across
   their probes, so their kept lines ARE distinguishable and a keep-swap would be
   caught. Only `0078`/`0079`/`0080` — whose format is `STATUS=…  PATH=…` alone with
   a shared `/x` — carry the true blindness. M-5 records three.
2. **"`0083` would catch the arm-hoist" — UPGRADED from hand-derivation to
   MEASUREMENT.** A dimension derived this by hand and flagged it as unverified.
   This session MEASURED it (§R2.3): `0083` REDs at `p07-absent-keeps-GUARD` with a
   byte-exact body mismatch. It also found the stronger case the dimension missed —
   **`0085` REDs too** — which is what makes I-1 a refutation of §D's own named
   fixtures rather than an argument about a third one.
3. **"ADR-0164's leg (iv) is asserted, not measured" — DOWNGRADED to a Nit.** The
   assertion is correct (verified on disk here) and the decisive measurement — 75.2's
   EMPTY executable diff — was already in the document, just uncited. That makes it a
   citation gap in the ADR (N-7b), not an evidence gap in the gate.

## §R6. Strengths

- **The I-1 fix is minimal, correct, and generalized.** Distinct `path:` per probe +
  `prefix: "/"` on the route + a format that ALREADY contained `%REQ(:PATH)%` = the
  kept line names its own probe at **zero** runtime cost. Choosing `:path` over the
  gating header is right and rightly justified: `REQ_ALLOW_LIST` really is the seven
  names claimed, and `%REQ(X-A)%` really is boot-fatal. And the fix did not stop at
  the two fixtures — it became a standing §G rule with an in-file *"Do NOT collapse
  these paths back to a common value"* warning at the exact site a future author
  would break it.
- **The TDD RED is real and this review reproduced it character for character.**
  §R2.2. Both REDs show one line per side, so the count assertion still passed —
  exactly the class the shared `/x` masked. The record does not merely assert RED; it
  shows the RED has the *predicted shape*, and it re-greps the mutation as still
  present after the run (defeating the parallel-`git checkout` trap).
- **Both fixtures witness precisely what they claim.** Every probe was walked by hand
  through the post-fix and pre-fix engines; all five verdicts match their
  `expect_logged` values, and both fixtures would have been RED pre-75.1. The
  per-side config divergence is exactly the four sanctioned items and nothing else —
  `node:`, `codec_type`, the whole `header_filter` body, the format string, the route
  table and the ~18-line rationale comment are byte-identical across sides.
- **The state-4 gate was not a walk-over, and says so.** Three sweeps when two was the
  stated minimum, specifically because the failing SET moved between runs 1 and 2. A
  lazier session stops at two and never sees the floating tail at all.
- **The departure from `PLAN.md`'s flake list is stated plainly rather than glossed**
  (`PROGRESS.md:2178`), and handed UP for grading rather than declared settled —
  `STATE.md` explicitly tells this reviewer to decide whether the test is sound *"not
  merely whether the gate says GREEN"*. That is the right posture for a self-graded
  gate and it is why I-1 was found.
- **Gate (c) refused to offer its own local green as evidence**, demonstrating the
  h2spec self-skip under `--nocapture` and reporting the criterion `PLAN.md`
  actually states. A weaker session quotes `3 passed` and moves on.
- **The numbers are overwhelmingly right.** Every hard census re-derived on disk
  reproduced exactly: 85 fixtures, 85 entrypoints, 104 ROADMAP rows / 102 `done` /
  exactly `75` and `75.2` `in-progress`, 22-of-22 `#![forbid(unsafe_code)]` (D-3.8
  HOLDS), 21-line `known-failures.txt` at a 0-line diff, 5 fuzz targets / 75 tracked
  corpus files / 63 for `parse_bootstrap` / **0** for `cdn_loop_parse`, all five
  `deny.toml` warning line numbers, every document line count, the ADR ledger head,
  and all eight rows of the `0078`–`0085` fixture→binary map. The
  gate-(d) self-correction of the long-repeated "5 targets / 63 seeds" pairing is a
  session catching its own document's error by measurement.
- **Append-only and scope discipline are exact across both sessions.** The re-entry
  is 441 insertions / 0 deletions on `PROGRESS.md` with zero pre-existing lines
  modified; `ROADMAP.md`, `DECISIONS.md`, `75/SPEC.md` and `75.1/` are absent from
  every diff; the ADR-0035 relocation checks out at 13/13 at delta +1 with 0 history
  lines lost. Neither session flipped a ROADMAP row, trimmed `known-failures.txt`, or
  widened into CF-75-3 or CF-75-2.
- **M-5 and M-6 of the first review were closed the HARDER way.** M-6 offered
  "quote the failure text OR admit family-membership adjudication"; the re-entry ran
  a fresh isolation run and produced verbatim text. M-5 offered disclosure; the
  re-entry disclosed AND reconstructed the mechanism AND explicitly labelled the
  reconstruction *"an explanation, not a proof"*.

## §R7. What this review did NOT do

- **Did NOT re-run the §7.5 gate.** It is landed and GREEN on HEAD; re-running it is
  not state-5's job. The only runs made were this review's own control + mutation
  measurements, in an isolated worktree with its own `CARGO_TARGET_DIR`.
- **Did NOT edit any artifact it grades** — no fixture, no `crates/` file, no
  `PLAN.md`, no `SPEC.md`, no `PROGRESS.md`, no `BEHAVIOR_CONTRACT.md`. A review
  grades; the re-entry fixes.
- **Did NOT re-open or re-grade sub-phase 75.1**, and did not touch the frozen
  `75/SPEC.md` or any `75.1/` artifact.
- **Did NOT flip ROADMAP row `75.2` or parent row `75`.** Both stay `in-progress`;
  both flips belong to the state-6 close-out.
- **Did NOT widen into CF-75-3 or CF-75-2**, and did not re-raise the h2spec
  vacuity question — SETTLED at ADR-0163.
- **Did NOT edit a landed ADR.** I-1's correction to `DECISIONS.md:2462` and N-3/N-4
  are stated FORWARD, not retro-edited (D-3.5).
- **Did NOT chain into the state-6 close-out**, and created no `stop` file — 102 of
  104 ROADMAP rows are `done` and the `## Feature Families` are largely unbuilt.
- **Did NOT disturb the parallel workstream.** Only this session's own worktree was
  removed; the four `.claude/worktrees/agent-*` were left untouched and verified
  present afterwards. All censuses used `git ls-files` or the root `Cargo.toml`
  rather than a repo-wide `find`.

## §R8. Carry-forward disposition after this review

**OPENED by this review:**

- **CF-75-4** — fixtures `0078`, `0079` and `0080` carry the same probe-attribution
  blindness that finding I-1 removed from `0084`/`0085`: every probe shares
  `path: /x` under a `STATUS=… PATH=…` format, so a regression that MOVES the keep
  between probes preserves the line count and passes GREEN. `0078` is the direct
  stencil `0084` was copied from and is the only other access-log `header_filter`
  fixture. The fix is the same ~6 lines each. Owner: whoever next touches the
  access-log fixture family. See M-5.

**CLOSED by this sub-phase (confirmed):** **CF-72-1** (by 75.1's engine fix,
witnessed cross-proxy on the access-log path by `0084`); **M74-31** (the kept-LAST
causal non-sequitur, corrected at all four live sites and not propagated anywhere in
75.2's new material — verified).

**BANKED, NOT fixed (confirmed on disk):** **CF-75-1** and the extended **CF-72-2**
(name-only `{ name }`, `treat_missing_header_as_empty` accepted AND honored, the
top-level `contains_match` arm). Owner: a future `HeaderMatcher` wire-shape-parity
phase, which should decide them together.

**OPEN and OUT OF SCOPE, unchanged:** **CF-75-3** (ADR-0163 — `ci.yml` runs
`cargo test --workspace` with NO `--no-fail-fast`, so on a RED CI run conformance
status is UNKNOWN, not green; wants its own phase) and **CF-75-2** (upstream
COMMA-JOINS duplicate header values before value matching; envoy-rust matches only
the FIRST occurrence; the PRESENCE axis is PARITY so none of 75.1's rule is
affected). Re-confirmed: `grep -c "CF-75-2"` in `BEHAVIOR_CONTRACT.md` is **0** — it
is recorded in `STATE.md`, and both new fixture READMEs were corrected at the
re-entry to say so.

**Observations handed up by state-4, disposed of here:**

1. **The "exactly FIVE REDs" figure is incomplete** — ACCEPTED as correct and
   already recorded (ADR-0164). No further action.
2. **`cdn_loop_parse` has ZERO tracked fuzz-corpus seeds** while the other four have
   8 / 63 / 3 / 1 — **CONFIRMED on disk** (`git ls-files | grep 'fuzz/corpus/'`
   yields no row for it), and it DOES have its own `ci.yml` step, so the target runs
   in CI from an empty corpus. **Disposition: CARRY-FORWARD, not a 75.2 finding.**
   75.2 added no fuzz surface and §7.4's gate (d) asks only that a *new* fuzzer run
   clean. Seeding it is a real improvement but belongs to whoever next owns the fuzz
   surface. Banked as **CF-75-5**.
3. **The "5 targets / 63 seeds" pairing** — already corrected by measurement in
   gate (d); the earlier section is correctly left as-landed. No further action.

**Untouched, carried forward:** CF-74-1/2/3/4/6, CF-73-1, N73-R2, M73-R1/M73-R2,
M71-3, M71-6/7/8, M70-R4/R9, M69-A..I, CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7,
M74-3..M74-14, M74-16, M74-17/18/20/21/22/26/27/28/29, M74-30, M74-32..M74-39, the
older Minors in `67.3/SPEC.md` §10, and the HTTP-filters-family (1)-(4) in
`STATE_HISTORY.md`.

## §R9. Next state — the §5.2 state-3 RE-ENTRY (step 3, NOT step 4)

Per `BOOTSTRAP_PROMPT.md` §5.2, a CHANGES-REQUESTED verdict re-enters at **step 3**.
The next session must:

1. **Fix I-1** — correct `BEHAVIOR_CONTRACT.md` §D to the measured behavior (the
   arm-order hoist breaks BOTH the P1 cell and the `present_match: false` × absent
   cell; it REDs `0085` and `0083` and leaves only `0084` green), replace the false
   *"the differential fixtures cannot catch a regression in it"* conclusion with the
   true coverage statement, and correct the same text in `STATE.md:28`'s
   Standing-traps clause (a). Quote §R2.3's measurement. **Re-derive the
   `BEHAVIOR_CONTRACT.md` site BY TEXT ANCHOR, never by the line number quoted here
   — these numbers are valid at `1da0662` only and every earlier fix shifts the ones
   below it.**
2. **Fix I-2** — re-anchor the comment-only proof on `3f0ec89..HEAD`, widen the
   filter to catch `//` and `//!`, and state the base wherever the recipe is
   propagated.
3. **Fix M-1, M-2, M-3, M-6, M-7** — all small factual corrections, appended per
   D-3.5 rather than retro-edited where the text is landed.
4. **Record M-5 as CF-75-4** and adjust §G's closing sentence; **record
   observation 2 as CF-75-5**.
5. **Take NO action on N-4 and N-6** — both are landed artifacts that must not be
   retro-edited. N-1/N-2/N-3/N-5/N-7 are optional polish; N-7's ADR-0164 sharpening
   is stated forward, never by editing the ADR.
6. **Honour TDD.** I-1 and I-2 are documentation corrections to claims about
   measured behavior, so the RED is the measurement itself: re-run mutation H and
   confirm `0085` and `0083` go RED with an unmutated control from the same
   worktree, in the re-entry's OWN scratch worktree. Do not transcribe §R2.3 —
   re-measure it.
7. **Append to `PROGRESS.md`**; fire an ADR only if a genuinely new decision arises.
   The §D correction is arguably one (it supersedes a MEASURED claim in the canonical
   contract), and **ADR-0165** is the next available number.

It must **NOT** flip ROADMAP row `75.2` or parent row `75`, re-run the §7.5 gate,
re-open sub-phase 75.1, edit any `75.1/` artifact or the frozen `75/SPEC.md`, edit a
landed ADR, widen 75.2 into CF-75-3 or CF-75-2, chain into the close-out, or create
a `stop` file.

After the re-entry lands, the sequence is: **state-4 RE-VERIFICATION** → **state-5
RE-REVIEW** → **state-6 CLOSE-OUT** (at which ROADMAP row `75.2` AND parent row `75`
both flip `done`). Each is its own session.

Ledger head after this review: **ADR-0164**; next available **ADR-0165**.
