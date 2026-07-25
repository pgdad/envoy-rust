# Phase 75.1 — `HeaderMatcher` absence semantics: PROGRESS

> **What this document is.** The §5 **state-3 implementation** running log for
> sub-phase **75.1** (`75.1-headermatcher-absence-engine-route`), created by the
> §6.1 SPLIT of phase 75 (ADR-0157). It is appended to on EVERY task completion,
> quoting the ACTUAL command output, and it is the evidence base the §5 state-4
> verification session (a SEPARATE session per §5.1 / ADR-0127) grades against.
>
> **Written for a stranger with zero prior context (D-3.4).** The plan being
> executed is
> `docs/envoy-rust/phases/75.1-headermatcher-absence-engine-route/PLAN.md`
> (13 TDD-ordered tasks); the measured basis is that directory's `SPEC.md`.
>
> **The rule this sub-phase implements** (MEASURED cross-proxy against
> `envoyproxy/envoy:v1.33.0`, the `ENVOY_TARGET.md` pin):
>
> ```
> present := the named header is present (name matched case-insensitively;
>            an EMPTY VALUE still counts as PRESENT)
>
> if mode is present_match(want):   result = (present == want) XOR invert_match
> else if not present:              result = false      # invert_match NOT applied
> else:                             result = mode_matches(value) XOR invert_match
> ```
>
> **Session start state (verified on disk):** `git status --porcelain` clean,
> branch `main`, `HEAD` = `2856976f95da1bf920d6b914220b9099c76acc11`, with
> `origin/main` at the SAME SHA after `git fetch origin --prune` — no sibling
> workstream had advanced.

---

## Task 1 — the in-process engine matrix (RED)

**What it does.** Appends `absence_semantics_matrix_matches_measured_upstream`
to the existing `#[cfg(test)] mod tests` of
`crates/envoy-config/src/matcher.rs`: seven modes × {absent, present-matching,
present-non-matching} × {invert, no-invert}, plus the empty-header-VALUE
control. This is the coverage whose absence let divergence **D2** (upstream
`present_match: false` means the header must be ABSENT; the in-tree engine
modelled it as unconditionally true) survive in-tree from phase 04.2 to here.

**This is a DELIBERATE RED commit** — the TDD RED step required by doctrine
D-3.1. It is the only commit in this sub-phase permitted to be red, and Task 2
(the engine fix) follows it immediately.

### Command

```bash
cargo test -p envoy-config --lib \
  matcher::tests::absence_semantics_matrix_matches_measured_upstream
```

### Output (verbatim)

```
   Compiling envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.32s
     Running unittests src/lib.rs (target/debug/deps/envoy_config-53c650ab60359ac8)

running 1 test
test matcher::tests::absence_semantics_matrix_matches_measured_upstream ... FAILED

failures:

---- matcher::tests::absence_semantics_matrix_matches_measured_upstream stdout ----

thread 'matcher::tests::absence_semantics_matrix_matches_measured_upstream' (1883701) panicked at crates/envoy-config/src/matcher.rs:607:13:
exact_match+invert: ABSENT must be false — invert_match is NOT applied to a missing header (D1 / CF-72-1)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    matcher::tests::absence_semantics_matrix_matches_measured_upstream

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 648 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p envoy-config --lib`
```

### Adjudication

- **RED is REAL and SEMANTIC**, not a build or startup artifact: the failure is
  an `assert!` message from the test body, reached at
  `crates/envoy-config/src/matcher.rs:607`.
- **The failing cell is the one `PLAN.md` Task 1 step 2 predicted**: the FIRST
  assertion to fire is the **D1** cell (`exact_match+invert: ABSENT`), because
  the unfixed engine at `matcher.rs:52` computes `false ^ true` = `true`
  (KEEP) where upstream DROPS.
- **The run really rebuilt** — `grep -c 'Compiling envoy-config'` = **1**
  (memory `mutation-check-needs-forced-rebuild`: cargo can reuse a stale test
  binary and report a FALSE result; for `cargo test`/`build` the token is
  `Compiling`, for `clippy` it is `Checking`).
- `648 filtered out` is the rest of the `envoy-config` lib suite, deselected by
  the name filter — not a skip.

**Commit:** `phase 75.1 task 1: RED — in-process absence-semantics matrix for the shared HeaderMatcher engine`

---

## Task 2 — the mode-scoped engine fix, 3 amended tests, 3 strengthened guards (GREEN)

**What it does.** Replaces the UNIFORM `mode_result ^ self.invert_match` engine
in `HeaderMatcher::matches` (`crates/envoy-config/src/matcher.rs`) with a single
exhaustive tuple `match (&self.mode, value)` whose `(_, None) => return false`
arm sits **AFTER** the `PresentMatch` arm and **BEFORE** every value arm, so ARM
ORDER carries the whole rule and the compiler checks exhaustiveness (the shape
decided by ADR-0159; the two rejected alternatives are recorded there).

In the SAME commit, because they would otherwise be red:

- **THREE divergence-encoding tests AMENDED** (they asserted the OLD behavior),
  each renamed to describe PARITY rather than divergence:
  - `present_match_false_returns_true_when_present`
    → `present_match_false_requires_the_header_to_be_absent` (assertion FLIPPED;
    this was the in-tree test that PINNED divergence **D2**)
  - `pv4_value_matcher_absent_plus_invert_kept_diverges_from_upstream`
    → `pv4_value_matcher_absent_plus_invert_dropped_is_parity_with_upstream`
    (BOTH assertions flipped — the inherent engine and the ADR-0150 trait object)
  - `header_match_trait_delegates_to_inherent_engine` (its final assertion
    flipped; the THIRD copy of the divergent assertion)
- **THREE guards KEPT GREEN and strengthened** — assertions UNCHANGED, rationale
  restated: `invert_match_inverts_present_match_result`,
  `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream`, and
  `present_match_false_returns_true_when_absent`
  → `present_match_false_matches_when_absent` (right answer, previously for the
  WRONG stated reason).

The `RangeMatch` arm changed from
`value.and_then(|v| v.parse::<i64>().ok()).is_some_and(..)` to
`v.parse::<i64>().is_ok_and(..)` — a mechanical consequence of `v` already being
a `&str` after the tuple destructure. `Result::is_ok_and` is stable since Rust
1.70; the toolchain pin is 1.95.0.

### Command 1 — the matcher suite

```bash
cargo test -p envoy-config --lib matcher
```

```
test result: ok. 60 passed; 0 failed; 0 ignored; 0 measured; 589 filtered out; finished in 0.00s
```

The six load-bearing tests, all `ok`:

```
test matcher::tests::invert_match_inverts_present_match_result ... ok
test matcher::tests::present_match_false_matches_when_absent ... ok
test matcher::tests::present_match_false_requires_the_header_to_be_absent ... ok
test matcher::tests::pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream ... ok
test matcher::tests::pv4_value_matcher_absent_plus_invert_dropped_is_parity_with_upstream ... ok
test matcher::tests::absence_semantics_matrix_matches_measured_upstream ... ok
```

> **CORRECTION to `PLAN.md` Task 2 step 4 — the expected count is 60, not 59.**
> The plan predicted `59 passed` and glossed it as "58 pre-existing in the two
> `matcher` test modules, plus Task 1's matrix". Both figures are off by one, in
> a way that matters only to whoever reads the number:
>
> - The `matcher` substring filter does NOT select "the two matcher test
>   modules". It selects any test whose full path contains `matcher`, which is
>   **35** in `matcher::tests` + **3** in `matcher::metadata_match_tests` +
>   **22** in `bootstrap::tests::*matcher*` (e.g.
>   `bootstrap::tests::parses_header_matcher_exact`,
>   `bootstrap::tests::rbac_tests::path_matcher_unknown_subkey_is_denied`).
> - Task 1 added exactly ONE test, so the pre-existing count under this filter
>   was **59**, not 58. The PLAN-write pre-flight's own `59 passed` was
>   evidently the count WITHOUT Task 1's matrix, then mislabelled as being with
>   it.
>
> **Nothing behavioral is implied** — `0 failed` either way, and every named
> guard passes. Recorded so the state-4 session does not read `60` as an
> unexplained drift from a plan that says `59`.

### Command 2 — the whole crate (no other module regressed)

```bash
cargo test -p envoy-config
```

```
test result: ok. 649 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Command 3 — lint

```bash
touch crates/envoy-config/src/matcher.rs
cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.39s
```

Clippy exit `0`, ZERO warnings under `-D warnings`; `fmt --check` exit `0`,
silent. The `touch` + `grep -c 'Checking envoy-config'` = **1** proves the run
really re-analysed the crate rather than replaying a cache — clippy prints
`Checking`, NOT `Compiling`, so grepping for `Compiling` here would give a FALSE
NEGATIVE (memory `clippy-prints-checking-not-compiling`).

**Commit:** `phase 75.1 task 2: GREEN — mode-scoped HeaderMatcher absence rule; amend 3 divergence-encoding tests, strengthen 3 guards`

---

## Task 3 — the MUTATION CHECK (guard-level RED evidence)

**Why this task exists.** Task 1 gave TDD's RED for the **fix**. This task gives
the RED for the **GUARD**: it proves the three P1 guard tests actually catch the
specific wrong fix `SPEC.md` §2.2 warns about, rather than passing vacuously.
P1 (`present_match: true` + `invert_match` + ABSENT header) is MEASURED PARITY
on both proxies, so a naive uniform "absent ⇒ DROP" fix would BREAK it and mint
a NEW divergence. The mutation IS that wrong fix.

**Run in a scratch `git worktree`, NEVER in the main tree** (memory
`mutation-checks-collide-with-parallel-subagents`: a parallel reviewer's
`git checkout -- <file>` can silently revert an in-place mutation mid-run,
producing a FALSE GREEN that a `Compiling` grep does NOT catch).

### Step 1 — worktree created

```bash
git worktree add /tmp/wt-75-1-mutation HEAD --detach
```

```
Preparing worktree (detached HEAD 723bc3b)
HEAD is now at 723bc3b phase 75.1 task 2: GREEN — mode-scoped HeaderMatcher absence rule; amend 3 divergence-encoding tests, strengthen 3 guards
```

### Step 2 — the UNMUTATED CONTROL, run FIRST

A mutation RED is not automatically a SEMANTIC red — a run can "fail" on a build
or startup error that never reached an assertion (memory
`mutation-red-needs-unmutated-control`). So the control establishes the baseline
from the SAME tree:

```bash
cd /tmp/wt-75-1-mutation && cargo test -p envoy-config --lib matcher
```

```
test result: ok. 60 passed; 0 failed; 0 ignored; 0 measured; 589 filtered out; finished in 0.00s
```

### Step 3 — the mutation (a one-line reordering)

The mutation is exactly the mistake the SPEC warns about — a **uniform
"absent ⇒ DROP"**. In this engine shape that is achieved by hoisting the
`(_, None) => return false` arm ABOVE the `PresentMatch` arm, so an absent
header short-circuits for EVERY mode including `present_match`:

```diff
         let mode_result = match (&self.mode, value) {
+            (_, None) => return false,
             (HeaderMatcherMode::PresentMatch(want_present), v) => v.is_some() == *want_present,
-            (_, None) => return false,
```

**Mutation verified PRESENT on disk before the run** (not merely assumed):

```
32-            // present_match: false → must be ABSENT.
33:            (_, None) => return false,
34-            (HeaderMatcherMode::PresentMatch(want_present), v) => v.is_some() == *want_present,
```

### Step 4 — the mutated run

```bash
cargo test -p envoy-config --lib matcher
```

```
test result: FAILED. 56 passed; 4 failed; 0 ignored; 0 measured; 589 filtered out; finished in 0.00s

failures:
    matcher::tests::absence_semantics_matrix_matches_measured_upstream
    matcher::tests::invert_match_inverts_present_match_result
    matcher::tests::present_match_false_matches_when_absent
    matcher::tests::pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream
```

`grep -c 'Compiling envoy-config'` = **1** — the run really rebuilt, so this is
not a stale-binary FALSE result (memory `mutation-check-needs-forced-rebuild`).

### The verbatim assertion messages — all four are SEMANTIC, none is a build/startup artifact

```
---- matcher::tests::invert_match_inverts_present_match_result stdout ----
thread '...' panicked at crates/envoy-config/src/matcher.rs:439:9:
assertion failed: m.matches(&[])

---- matcher::tests::present_match_false_matches_when_absent stdout ----
thread '...' panicked at crates/envoy-config/src/matcher.rs:358:9:
assertion failed: m.matches(&[])

---- matcher::tests::absence_semantics_matrix_matches_measured_upstream stdout ----
thread '...' panicked at crates/envoy-config/src/matcher.rs:654:9:
present(true)+invert: ABSENT must stay KEEP (P1 — MEASURED PARITY)

---- matcher::tests::pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream stdout ----
thread '...' panicked at crates/envoy-config/src/matcher.rs:489:9:
present_match absent+invert = KEEP on BOTH proxies (PARITY, not a divergence)
```

### Adjudication

- **The guards are NOT vacuous.** All THREE guards `PLAN.md` names — `:463`
  (`pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream`), `:425`
  (`invert_match_inverts_present_match_result`) and `:348`
  (`present_match_false_matches_when_absent`) — go RED under the wrong fix, each
  with a genuine left-vs-right assertion failure, against a `60 passed; 0 failed`
  unmutated control from the SAME worktree.
- **Every failure is on the P1 cell**, which is the point: the mutation is
  invisible to every value-mode assertion and detectable ONLY by the
  `present_match` guards.
- **`4 failed`, not the plan's `3`** — `PLAN.md` Task 3 step 4 predicted three
  and then explicitly noted the fourth: "Task 1's matrix also asserts the P1
  cell, so if it is present in this worktree it fails too — that is expected and
  additive, not a discrepancy." It is present (Task 1 is committed), it fails on
  exactly that cell (`matcher.rs:654`), and the count is `56 passed; 4 failed`
  rather than the pre-flight's `56 passed; 3 failed` measured before Task 1
  existed. **This CONFIRMS the plan rather than contradicting it.**

### Step 5 — teardown; the mutation is NEVER committed

```bash
git worktree remove --force /tmp/wt-75-1-mutation
git worktree list
git status --porcelain
```

```
/home/esa/git/envoy-rust                                            723bc3b [main]
/home/esa/git/envoy-rust/.claude/worktrees/agent-a0cda5e6afdd64be2  2d6ecda [worktree-agent-a0cda5e6afdd64be2]
/home/esa/git/envoy-rust/.claude/worktrees/agent-a22debad535db1d78  7140aba [worktree-agent-a22debad535db1d78]
/home/esa/git/envoy-rust/.claude/worktrees/agent-a54a85accb5dc112f  2b535b5 [worktree-agent-a54a85accb5dc112f]
/home/esa/git/envoy-rust/.claude/worktrees/agent-ac17c8d4a0ab78914  9e8cfe7 [worktree-agent-ac17c8d4a0ab78914]
```

`/tmp/wt-75-1-mutation` is GONE and `git status --porcelain` is EMPTY. The four
`.claude/worktrees/agent-*` entries belong to a PARALLEL WORKSTREAM and were
LEFT ALONE — only this session's own scratch worktree was removed.

Main-tree arm order re-verified INTACT after teardown (`(_, None)` at
`matcher.rs:37`, i.e. AFTER the `PresentMatch` arm, not before):

```
28:        let mode_result = match (&self.mode, value) {
37:            (_, None) => return false,
```

**Commit:** `phase 75.1 task 3: mutation check — uniform absent-DROP turns all three P1 guards RED; reverted`

---

## Task 4 — the eight doc comments and the in-source citation fix

**What it does.** Pure documentation: eight comment blocks across three crates
stated the PRE-75.1 uniform-XOR rule (or D2's rule verbatim) as if it were
correct. No behavior changes, so the gate is a BY-HAND wrap check plus the
existing suite staying green.

**The doc-comment hazard is real here** (memory
`mechanical-fanout-scripts-corrupt-doc-comments`): `cargo fmt` does NOT reflow
`///` / `//!` / `//` lines, so nothing mechanical catches a mis-wrapped or
semantically-backwards comment. Every block was edited individually — no
mechanical fan-out script was used — and hand-checked.

### The eight sites

| # | site | what was wrong |
|---|---|---|
| 1 | `matcher.rs` `HeaderMatcher::matches` doc | described name/value casing only; said nothing about absence. Now states the full MODE-SCOPED rule, names both P1 guards, and warns against "simplifying" the arm order |
| 2 | `matcher.rs` the `PresentMatch` arm's inline comment | stated **D2's rule verbatim** — `present_match: false → no presence requirement (always true)`. REMOVED by Task 2's body replacement; re-verified gone |
| 3 | `matcher.rs` the ADR-0150 seam doc | asserted `mode_result ^ invert_match, incl. absent+invert = keep` as a design GUARANTEE of the seam |
| 4 | `matcher.rs` the `// PresentMatch: 4 cells` comment | still 4 cells, but two expectations flipped |
| 5 | `matcher.rs` the in-SOURCE `matcher.rs:51` citation | the XOR is at `:52`. Removed by Task 2's comment replacement; re-verified gone |
| 6 | `bootstrap.rs` the `invert_match` field doc | "the entire mode-specific match result is inverted (XOR after the mode match runs)" — no longer unconditional |
| 7 | `bootstrap.rs` the `PresentMatch` variant doc | repeated the wrong rule (`"no presence requirement" (false)`) |
| 8 | `bootstrap.rs` the `ValueMatcher` cross-reference (**Trap A**) | said the RBAC rule is "NOT the HeaderMatcher `present_match` precedent" — now restated as: the two AGREE when PRESENT and still DIFFER when ABSENT |
| 9 | `crates/envoy-accesslog/src/filter.rs` the `LogFilter::Header` arm | "PV-4's `mode_result ^ invert_match` is preserved because the injected impl calls `HeaderMatcher::matches` verbatim" — the delegation stays true, the asserted semantics did not |

> **Trap A was NOT collapsed.** `ValueMatcher.present_match` (RBAC /
> access-log metadata) keeps its own rule — **`present_match: false` NEVER
> matches** — which is DIFFERENT and CORRECT. Site 8 restates the comparison; it
> does not touch the `ValueMatcher` rule itself.

### Verification

```bash
grep -n 'no presence requirement' crates/envoy-config/src/matcher.rs   # step 2
grep -n 'matcher.rs:51'           crates/envoy-config/src/matcher.rs   # step 5
git diff -U0 | grep -E '^\+.*(///|//!|//)' | awk '{ if (length($0) > 83) print "TOO LONG: " $0 }'
grep -rn 'no presence requirement\|always true' crates/envoy-config/src/ crates/envoy-accesslog/src/
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p envoy-config -p envoy-accesslog
```

- **The in-source `matcher.rs:51` citation is GONE** — zero hits. This is ONE of
  the exactly TWO sites this sub-phase corrects; the other is
  `BEHAVIOR_CONTRACT.md:2369` (Task 13).
- **The wrap check printed NOTHING** — no added comment line exceeds ~82
  columns. 37 added `///` lines were reviewed.
- **`cargo fmt --all -- --check` exit `0`** (silent);
  **`cargo clippy --workspace --all-targets --all-features -- -D warnings` exit
  `0`**, zero warnings across the WHOLE workspace, not just the touched crates.
- **Tests green:**

```
test result: ok. 112 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 649 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

#### The stale-phrase grep returns FOUR hits — adjudicated BY LINE, all four CORRECT

`PLAN.md` step 10 expects this grep to print nothing. It prints four lines, and
**all four are the OLD rule being QUOTED inside the correction that retires it**
— exactly the "a record legitimately quotes the defect it fixes" case the plan
warns about two steps later ("Adjudicate greps by LINE, not by count"):

```
matcher.rs:350:    // measured rule is `(present == want)`, not "false ⇒ always true".
matcher.rs:365:        // 75.1 this test asserted the opposite ("no presence requirement,
matcher.rs:366:        // always true") and was the in-tree test that PINNED divergence D2.
bootstrap.rs:3152:    /// documented `false` as "no presence requirement (always true)", which was
```

Read in context, each says the phrase WAS the rule and is no longer:
`matcher.rs:350` is the `// PresentMatch: 4 cells` comment saying the measured
rule is `(present == want)` and NOT "always true"; `:365-366` is the amended D2
pin explaining what it used to assert; `bootstrap.rs:3152` is the `PresentMatch`
variant doc naming its own former wording as divergence D2. **ZERO sites still
assert the old rule as current.** No edit was made to "clean these up" —
deleting them would destroy the record of what was fixed.

**Commit:** `phase 75.1 task 4: correct eight doc comments stating the pre-75.1 uniform-XOR rule; fix the in-source matcher.rs:51 citation`

---

## Tasks 5-9 — consumer propagation: the RED methodology used for all five

`HeaderMatcher::matches` is evaluated at **exactly five production call sites**
across five subsystems in three crates. Tasks 5-9 add ONE in-process test per
call site proving the mode-scoped rule actually reaches it. **No call-site
production code is edited in any of these tasks** — the behavior comes entirely
from Task 2's engine fix; these tasks are coverage.

**How RED was honored.** `PLAN.md` Task 5 step 3 says: run the new tests against
the PRE-Task-2 engine, or — if Tasks 1-2 are already committed, which is the
normal ordering and was the case here — verify RED in a scratch worktree. A
scratch worktree `/tmp/wt-75-1-consumers` was created at the Task-4 commit and
the Task-2 engine fix was **REVERSED in it** (the pre-75.1 uniform-XOR engine
restored verbatim), leaving every consumer test at its post-fix form. Each new
test's file was then copied in and run. This is the honest "would this test have
failed before the fix?" check, per crate, and it is run in a worktree so nothing
can mutate the main tree mid-run (memory
`mutation-checks-collide-with-parallel-subagents`).

> **A `0 passed` FALSE GREEN was caught and corrected mid-task.** The first RED
> attempt returned `test result: ok. 0 passed; 0 failed; 184 filtered out` with
> **exit code 0** — which looks like a pass and is not one: the worktree was at
> the Task-4 commit, so the not-yet-committed Task-5 test did not exist in it and
> **the test never ran** (memory `cargo-test-p-name-false-green-filtered-out`:
> assert on the passed COUNT, never on the exit code). The test files were copied
> into the worktree and the presence of the test + the pre-fix engine were both
> `grep`-confirmed before re-running. Every RED below is from a run that
> demonstrably executed the test.

---

## Task 5 — consumer propagation: the ROUTE walker (H1 AND H2)

Call site **1 of 5**: `crates/envoy-http1/src/hcm.rs` `route_matches`, which
serves BOTH protocols — H2 has no independent walker and delegates via
`envoy_http1::hcm::resolve_route`. This is the call site fixture `0083`
witnesses cross-proxy.

**Interface correction, anticipated by the plan.** `PLAN.md` Task 5 step 2 wrote
the H2 test using `envoy_http1::codec::Request::test("GET", "/x", &[])` and its
implementer note said to mirror the sibling
`h2_resolve_route_reachable_and_returns_cors_route` if that constructor differs
in the live tree. **It does** — no such constructor exists; the sibling builds an
`envoy_http1::Request` STRUCT LITERAL (`method` / `path` / `version` /
`headers` / `bytes_consumed` / `body`). The literal form was used, exactly as the
note directs. **No new public constructor was added to `envoy-http1`.** The
resolved-route accessor is likewise the method form `r.route().name`, as the
sibling uses.

### RED — on the pre-75.1 engine (scratch worktree)

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 184 filtered out
thread 'hcm::tests::route_header_matcher_absence_rule_is_mode_scoped' panicked at crates/envoy-http1/src/hcm.rs:10024:9:
value+invert, ABSENT → route must NOT match (D1 / CF-72-1 closed)
```

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 110 filtered out
thread 'hcm::tests::h2_resolve_route_inherits_mode_scoped_absence_rule' panicked at crates/envoy-http2/src/hcm.rs:6838:9:
assertion `left == right` failed: D1 on the H2 path
  left: "gated"
 right: "catch-all"
```

The H2 failure is the most legible statement of the bug in the whole phase: with
the header ABSENT and a `exact_match` + `invert_match` route, the PRE-fix engine
let the **gated** route win — i.e. envoy-rust routed a request to a route whose
header condition upstream Envoy considers unmet.

### GREEN — on the fixed engine (main tree)

```bash
cargo test -p envoy-http1 --lib route_header_matcher_absence_rule_is_mode_scoped
cargo test -p envoy-http2 --lib h2_resolve_route_inherits_mode_scoped_absence_rule
```

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 184 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 110 filtered out; finished in 0.00s
```

`1 passed` on each — asserted on the COUNT, not the exit code.

Each test covers all three cells: **D1** (value matcher + invert + absent must
not match), **D2** (plain `present_match: false` requires absence), and **P1 THE
GUARD** (`present_match: true` + invert + absent STILL matches).

**Commit:** `phase 75.1 task 5: pin mode-scoped absence propagation through the route walker (H1 + H2 via resolve_route)`

---

## Task 6 — consumer propagation: HTTP RBAC

Call site **2 of 5**: `crates/envoy-filter/src/rbac.rs`, inside
`pub(crate) fn eval`. The test lives IN-CRATE because `RuntimeMatcher` and
`eval` are both `pub(crate)` — they are not reachable from an integration test.

> The shared `crate::types::header_matcher_exact` helper was deliberately NOT
> used: it builds the `StringMatch` mode, not `ExactMatch`, and this test needs
> to name the mode explicitly.

### RED — pre-75.1 engine

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 212 filtered out
thread 'rbac::tests::rbac_header_condition_absence_rule_is_mode_scoped' panicked at crates/envoy-filter/src/rbac.rs:1412:9:
value+invert, ABSENT → must NOT match (D1 / CF-72-1 closed)
```

### GREEN — fixed engine

```bash
cargo test -p envoy-filter --lib rbac_header_condition_absence_rule_is_mode_scoped
```

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 212 filtered out; finished in 0.00s
```

`rbac.rs` production code is NOT edited — the behavior comes from Task 2.

**Commit:** `phase 75.1 task 6: pin mode-scoped absence propagation through the HTTP RBAC matcher tree`

---

## Task 7 — consumer propagation: the fault filter header gate

Call site **3 of 5**: `crates/envoy-filter/src/fault.rs`, inside
`fn header_gate_matches` — which is PRIVATE, so the test lives in-crate and
observes the gate INDIRECTLY but unambiguously: the gate AND-combines its
matchers and a 100%-percentage abort fires iff the gate matches, so the verdict
is observable as `Decision::StopAndSend(_)` vs `Decision::Continue`.

### RED — pre-75.1 engine

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 212 filtered out
thread 'fault::tests::fault_header_gate_absence_rule_is_mode_scoped' panicked at crates/envoy-filter/src/fault.rs:210:9:
value+invert, ABSENT → gate must NOT fire (D1 / CF-72-1 closed)
```

This is the most operationally pointed of the five REDs: before the fix, a fault
gate of `exact_match` + `invert_match` **injected a 503 abort into requests that
did not carry the header at all**, where upstream Envoy leaves them alone.

### GREEN — fixed engine

```bash
cargo test -p envoy-filter --lib fault_header_gate_absence_rule_is_mode_scoped
```

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 212 filtered out; finished in 0.00s
```

`fault.rs` production code is NOT edited.

**Commit:** `phase 75.1 task 7: pin mode-scoped absence propagation through the fault filter header gate`

---

## Task 8 — consumer propagation: JWT-authn rule matching

Call site **4 of 5**: `crates/envoy-filter/src/jwt_authn.rs`, inside
`fn route_match_matches`. The observable is the one the neighbouring
`header_matcher_gates_rule_match` already uses, so no new observable was
invented: when a rule's header matcher does NOT match, the request takes the
"no rule matched ⇒ allow without JWT check" path and a TOKENLESS request is
`Continue`d; when the rule DOES match, a tokenless request is DENIED. So
`denied == 1` ⟺ the rule matched — **no token needs to be minted to read the
matcher's verdict.**

**Helper check (the plan's implementer note).** All helpers the plan assumed
exist: `keypair()` (imported from `envoy_jwt::test_support`, NOT defined
locally), `registry()`, `req(headers, path)`, `host()`, `denied_value(&reg)` and
`ISS`. The one deliberate deviation from the precedent is a **fresh `registry()`
per invocation**, so the verdict is a clean `denied_value == 1` rather than the
precedent's cumulative running count — which is what lets a single closure
answer six independent questions.

### RED — pre-75.1 engine

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 213 filtered out
thread 'jwt_authn::tests::jwt_rule_header_matcher_absence_rule_is_mode_scoped' panicked at crates/envoy-filter/src/jwt_authn.rs:715:9:
value+invert, ABSENT → rule must NOT match (D1 / CF-72-1 closed)
```

### GREEN — fixed engine

```bash
cargo test -p envoy-filter --lib jwt_rule_header_matcher_absence_rule_is_mode_scoped
```

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 213 filtered out; finished in 0.09s
```

`jwt_authn.rs` production code is NOT edited.

**Commit:** `phase 75.1 task 8: pin mode-scoped absence propagation through JWT-authn rule matching`

---

## Task 9 — consumer propagation: the access-log `header_filter` through the ADR-0150 trait object

Call site **5 of 5**: `crates/envoy-accesslog/src/filter.rs`
`LogFilter::should_log`, reached as `Arc<dyn HeaderMatch>`. This is the only
call site that goes through the **trait object** rather than the inherent
method, so it is the one that proves the ADR-0150 seam carries the new rule.

**Why the test lives in `crates/envoy-http1/src/hcm.rs` and not in
`envoy-accesslog`.** `envoy-accesslog` MUST NOT depend on `envoy-config`
(ADR-0150 — the reverse edge already exists, so it would be a dependency cycle),
so `envoy-accesslog`'s own tests can only use LOCAL STUB trait objects and could
never exercise the real engine. `envoy-http1` depends on both crates and owns
the private `compile_access_log_filter`, which is where a real `HeaderMatcher`
is actually boxed into the seam at runtime. Compiling through that function is
what makes this test exercise the real boxing rather than a stub.

**The ADR-0150 seam was NOT moved.** `envoy-accesslog` keeps ZERO workspace
dependencies; matchers still cross as injected trait objects; `LogFilter` still
has NO `Eq`/`PartialEq`. Neither `envoy-accesslog` nor `compile_access_log_filter`
was edited.

### RED — pre-75.1 engine

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 185 filtered out
thread 'hcm::tests::access_log_header_filter_absence_rule_is_mode_scoped_through_the_seam' panicked at crates/envoy-http1/src/hcm.rs:10089:9:
value+invert, ABSENT → DROP (D1 / CF-72-1 closed) — this is the divergence fixture 0078's README recorded as deferred
```

### GREEN — fixed engine

```bash
cargo test -p envoy-http1 --lib access_log_header_filter_absence_rule_is_mode_scoped_through_the_seam
```

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 185 filtered out; finished in 0.00s
```

Beyond D1/D2/P1 this test also carries the **empty-VALUE control through the
seam**: an empty header value counts as PRESENT, so `present_match: false` DROPs
it. That is the in-process pin which makes fixture `0083`'s two empty-value
probes a confirmation rather than the sole evidence (see Task 11).

### The RED worktree was torn down

```bash
git worktree remove --force /tmp/wt-75-1-consumers
git worktree list
```

Only the four `.claude/worktrees/agent-*` of the PARALLEL WORKSTREAM remain —
they were LEFT ALONE throughout, per memory
`concurrent-loop-sessions-race-on-phase-pick` (remove only your OWN artifacts).

### All five call sites are now pinned

| # | call site | subsystem | task |
|---|---|---|---|
| 1 | `envoy-http1/src/hcm.rs` `route_matches` | route header matching (H1 **and** H2) | 5 |
| 2 | `envoy-filter/src/rbac.rs` `eval` | HTTP RBAC | 6 |
| 3 | `envoy-filter/src/fault.rs` `header_gate_matches` | fault header gate | 7 |
| 4 | `envoy-filter/src/jwt_authn.rs` `route_match_matches` | JWT authn | 8 |
| 5 | `envoy-accesslog/src/filter.rs` `should_log` (via `Arc<dyn HeaderMatch>`) | access-log `header_filter` | 9 |

**Commit:** `phase 75.1 task 9: pin mode-scoped absence propagation through the ADR-0150 HeaderMatch trait object`

---

## Task 10 — fixture `0083`: the two configs

**Files created:**
`tests/fixtures/0083-headermatcher-absence-parity/{envoy.yaml,envoy-rust.yaml}`
(137 lines each).

One HTTP/1.1 HCM listener, `clusters: []`, `direct_response` only — so **no
backend container spawns**. Backend-free-ness is decided by a literal substring
scan for the `{{BACKEND_PORT}}` marker; `grep -c BACKEND_PORT` = **0** on both
files. EIGHT matchers over SIXTEEN routes as ordered PAIRS on prefix `/pNN`: the
first carries the matcher under test and answers `pNN=MATCH`, the second is an
unguarded catch-all answering `pNN=NOMATCH`. **The response body IS the
matcher's verdict, byte-exact.**

The probe ids are NON-CONTIGUOUS on purpose — they are the ids of the
`SPEC.md` §2.3 measured matrix, so every expectation reads straight off that
table's UPSTREAM column with nothing re-derived.

**Both files were generated from ONE shared body**, so the route table is
byte-identical between the sides by construction rather than by inspection. The
complete diff is exactly the three house per-side deltas and nothing else:

```
0a1,3
> node:
>   id: x
>   cluster: y
5c8
<         socket_address: { address: 0.0.0.0, port_value: {{PORT}} }
---
>         socket_address: { address: 127.0.0.1, port_value: {{PORT}} }
135,137d137
< admin:
<   address:
<     socket_address: { address: 0.0.0.0, port_value: 0 }
```

`codec_type: HTTP1` is on BOTH sides and is **NOT** a per-side divergence
(ADR-0158 correction C3). The unquoted `node: { cluster: y }` YAML-1.1 boolean
trap does NOT apply — the `node:` block exists only on the envoy-rust side,
exactly as in the proven `0007`.

### Step 3 — the SUBJECT side parses and routes

```bash
cargo build -p envoy-bin        # the harness runs target/debug/envoy-bin
./target/debug/envoy-bin -c /tmp/0083-rust-r1.yaml
```

Booted clean (`listener bound … 127.0.0.1:18083 … codec_type=HTTP1`). All eight
absent-header probes, plus the `x-a: v` variants:

```
p01 absent -> p01=NOMATCH    p06 absent -> p06=NOMATCH
p07 absent -> p07=MATCH      p08 absent -> p08=NOMATCH
p09 absent -> p09=NOMATCH    p10 absent -> p10=NOMATCH
p11 absent -> p11=NOMATCH    p12 absent -> p12=MATCH
```

`p07`=MATCH is the **P1 GUARD** holding and `p12`=MATCH is **D2** fixed — the
two cells that carry the whole phase.

### Step 4 — the UPSTREAM side parses

```bash
docker run --rm -v $D:/cfg envoyproxy/envoy:v1.33.0 --mode validate -c /cfg/0083-envoy-r1.yaml
```

```
configuration '/cfg/0083-envoy-r1.yaml' OK
```

Written to a FRESH FILENAME in a FRESH DIRECTORY (this host's Docker bind mounts
are STALE-CACHED — an in-place edit silently validates the PREVIOUS contents).
The run also emits `Deprecated field: … 'HeaderMatcher.exact_match'` warnings;
these are informational, upstream still honors the field, and the pre-existing
`0007` fixture uses the same spelling.

### The 22-cell CROSS-PROXY sweep — run BEFORE writing the expectations

Both proxies were run simultaneously (upstream in Docker with **`-p` PORT
MAPPING**, never `--network host`; envoy-rust as the freshly-rebuilt DEBUG
binary) and every one of the 22 probe cells was driven at BOTH:

```
PROBE                                  UPSTREAM       ENVOY-RUST     VERDICT
p01-absent-drops                       p01=NOMATCH    p01=NOMATCH    OK
p01-value-matches-so-invert-drops      p01=NOMATCH    p01=NOMATCH    OK
p01-value-differs-so-invert-keeps      p01=MATCH      p01=MATCH      OK
p06-absent-drops                       p06=NOMATCH    p06=NOMATCH    OK
p06-non-numeric-so-invert-keeps        p06=MATCH      p06=MATCH      OK
p06-in-range-so-invert-drops           p06=NOMATCH    p06=NOMATCH    OK
p07-absent-keeps-GUARD                 p07=MATCH      p07=MATCH      OK
p07-present-drops                      p07=NOMATCH    p07=NOMATCH    OK
p08-absent-drops                       p08=NOMATCH    p08=NOMATCH    OK
p08-present-keeps                      p08=MATCH      p08=MATCH      OK
p09-absent-drops                       p09=NOMATCH    p09=NOMATCH    OK
p09-value-matches-so-invert-drops      p09=NOMATCH    p09=NOMATCH    OK
p09-value-differs-so-invert-keeps      p09=MATCH      p09=MATCH      OK
p10-absent-drops                       p10=NOMATCH    p10=NOMATCH    OK
p10-value-matches                      p10=MATCH      p10=MATCH      OK
p10-value-differs                      p10=NOMATCH    p10=NOMATCH    OK
p11-absent-drops                       p11=NOMATCH    p11=NOMATCH    OK
p11-present-keeps                      p11=MATCH      p11=MATCH      OK
p11-empty-value-counts-as-present      p11=MATCH      p11=MATCH      OK
p12-absent-keeps                       p12=MATCH      p12=MATCH      OK
p12-present-drops                      p12=NOMATCH    p12=NOMATCH    OK
p12-empty-value-counts-as-present      p12=NOMATCH    p12=NOMATCH    OK

OVERALL: ALL 22 CELLS AGREE CROSS-PROXY
```

Every value also equals the body `PLAN.md` Task 11 predicts for that probe, so
the expectations file is transcribed from a MEASUREMENT, not from a hope.

Only this session's own `er-0083-up` container was removed. The parallel
workstream's `quizzical_goldstine` (also `envoyproxy/envoy:v1.33.0`) was
observed still running and **LEFT ALONE**.

**Commit:** `phase 75.1 task 10: fixture 0083 configs — 8 HeaderMatchers over 16 direct_response routes, backend-free`

---

## Task 11 — fixture `0083`: `expectations.yaml`

**File created:**
`tests/fixtures/0083-headermatcher-absence-parity/expectations.yaml`
(`kind: http1_probe_list`, `Driver::Http1ProbeList`, reused with ZERO harness
change).

### Step 2 — the probe census

```bash
grep -c '^    - name:' tests/fixtures/0083-headermatcher-absence-parity/expectations.yaml
```

**22**, and the per-group split is exactly the plan's: p01=3, p06=3, p07=2,
p08=2, p09=3, p10=3, p11=3, p12=3.

Every `expected_body` was cross-checked line-for-line against the Task-10 live
cross-proxy sweep — the file is a transcription of a measurement taken from both
proxies, not a derivation.

### Step 3 — THE ONE WIRE-SHAPE UNKNOWN: **RESOLVED, nothing weakened**

`SPEC.md` §2.3's empty-value column was measured with `curl -H "x-a;"`, which
puts `x-a:` on the wire. The harness instead emits `x-a: ` (a SPACE before
CRLF) because `drive_http1` formats `"{n}: {v}\r\n"`. Both are an empty value,
but this was the only cell in the fixture whose exact wire bytes differ from
what was originally measured — so `PLAN.md` required probing it directly and
PRE-AUTHORISED dropping ONLY the two empty-value probes if the proxies
disagreed.

**Both byte shapes were driven at BOTH proxies:**

| probe | shape | upstream | envoy-rust |
|---|---|---|---|
| `/p11` | `x-a: ` (harness shape, raw socket) | `p11=MATCH` | `p11=MATCH` |
| `/p12` | `x-a: ` (harness shape, raw socket) | `p12=NOMATCH` | `p12=NOMATCH` |
| `/p11` | `x-a:` (`curl -H "x-a;"`, the SPEC shape) | `p11=MATCH` | `p11=MATCH` |
| `/p12` | `x-a:` (`curl -H "x-a;"`, the SPEC shape) | `p12=NOMATCH` | `p12=NOMATCH` |

**The two proxies AGREE on BOTH byte shapes, and both shapes agree with each
other.** HTTP header values are whitespace-trimmed, so `x-a: ` and `x-a:` are
the same empty value to both implementations. **The pre-authorised fallback was
therefore NOT taken: both `p11-empty-value-counts-as-present` and
`p12-empty-value-counts-as-present` are KEPT at full strength, and no probe in
this fixture was weakened.**

This also independently corroborates the SPEC §2.3 empty-value column on a
second wire encoding, and pairs with the in-process empty-value control that
Task 1 (engine) and Task 9 (through the ADR-0150 seam) already pin.

**Commit:** `phase 75.1 task 11: fixture 0083 expectations — 22 probes across 8 matchers, all read off the measured upstream column`

---

## Task 12 — fixture `0083`: README and the test entrypoint

**Files created:**
- `tests/fixtures/0083-headermatcher-absence-parity/README.md` (the 8-section
  house shape: what it witnesses + the rule quoted in full, the config shape, the
  8-matcher table, the full 22-probe table, the "why p07 is load-bearing"
  subsection, the per-side divergence table, cross-references, and the
  "Deferred — NOT in this differential" section).
- `tests/differential/tests/headermatcher_absence_parity.rs` — the entrypoint.

**Registration cost was ONE file, as projected.** `tests/differential/Cargo.toml`
has no `[[test]]` stanza (cargo autodiscovers `tests/*.rs`), the workspace root
already lists `tests/differential`, CI is `cargo test --workspace`, and there is
no fixture registry — `run_fixture` takes a directory path. **No `ci.yml` edit,
no workspace edit, no `[[test]]` stanza.**

### Step 3 — the fixture runs GREEN cross-proxy

```bash
cargo build -p envoy-bin       # NOT optional: the harness runs target/debug/envoy-bin
cargo test -p differential --test headermatcher_absence_parity
```

```
running 1 test
INFO node registered node.id=x node.cluster=y
INFO listener bound with SO_REUSEPORT (one accept queue per worker) addr=127.0.0.1:37197 sockets=32
INFO envoy-rust listening (http_connection_manager) addr=127.0.0.1:37197 stat_prefix=ingress_http codec_type=HTTP1
test headermatcher_absence_parity_fixture ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.09s
```

### TWO ANTI-VACUITY CHECKS — because a 1.09 s "green" deserves suspicion

A differential that boots an Envoy container and finishes in ~1 s is fast enough
to warrant proving it did the work. Both checks were run.

**(a) The upstream container really is spawned.** `docker ps -a` after the run
shows nothing (testcontainers removes on drop), so the run was repeated while
polling `docker ps`:

```
SPOTTED upstream container during run:
interesting_cohen	envoyproxy/envoy:v1.33.0
```

A real upstream container on the `ENVOY_TARGET.md`-pinned image is started and
torn down per run. Envoy simply boots fast from a warm image. (The parallel
workstream's long-running `quizzical_goldstine` was filtered out of the poll and
LEFT ALONE.)

**(b) The 22 probes are genuinely ASSERTED against upstream, not merely driven.**
A negative control flipped the load-bearing `p07-absent-keeps-GUARD` expectation
from `p07=MATCH` to `p07=NOMATCH` and re-ran:

```
test result: FAILED. 0 passed; 1 failed
thread 'headermatcher_absence_parity_fixture' panicked at ...:37:10:
fixture passes: probe p07-absent-keeps-GUARD: upstream body != expected
  upstream: [112, 48, 55, 61, 77, 65, 84, 67, 72]
  expected: [112, 48, 55, 61, 78, 79, 77, 65, 84, 67, 72]
```

The failure names the probe and compares against **UPSTREAM's actual response
bytes** (`[112,48,55,61,77,65,84,67,72]` = `p07=MATCH`), which is direct proof
that the upstream side is driven and asserted. The expectation was then reverted
with `git checkout --` and the file confirmed clean.

> **A FIRST attempt at this control was DISCARDED as inconclusive.** Run in a
> scratch worktree, it went RED with
> `envoy-bin not found at /tmp/wt-0083-neg/target/debug/envoy-bin` — a STARTUP
> failure that never reached an assertion, which is exactly the trap memory
> `mutation-red-needs-unmutated-control` describes. Reading the failure TEXT
> rather than the exit code caught it; the control was redone in the main tree
> (which has the built binary) and only then produced the semantic RED above.

### Step 4 — the census moved by exactly one

```
fixture dirs: 83   (was 82)
tracked files in tests/fixtures/0083-headermatcher-absence-parity/: 4
```

**Commit:** `phase 75.1 task 12: fixture 0083 README + differential entrypoint — the route-path absence-parity witness is green`

---

## Task 13 — `BEHAVIOR_CONTRACT.md` §C rewrite, correction C2, the citation fix, and the `0078` README

**Files modified:** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (§C and the C2 site)
and `tests/fixtures/0078-accesslog-header-filter/README.md`.
**`docs/envoy-rust/DECISIONS.md` is NOT modified** — see step 4.

### Step 1 — §C rewritten

§C previously recorded D1 as an ACCEPTED, CARRIED divergence ("Phase 72 reuses
the engine verbatim … and does NOT fix it; the shared-engine fix is
carry-forward **CF-72-1**"), and **omitted D2 entirely** — there was no mention
anywhere in the contract of the non-inverted `present_match: false` divergence.

The rewrite: retitles §C to the parity rule, states the §2.1 rule in a fenced
block including the EMPTY-VALUE-counts-as-PRESENT clause, adds D2 inline,
records **CF-72-1 CLOSED**, names fixture `0083` as the cross-proxy pin and the
renamed in-process pins, lists all five sharing subsystems, fixes the stale
`matcher.rs:51` → **`:52`** citation, and points forward to 75.2 for the
access-log witness.

**KEPT, as `PLAN.md` requires** — re-tensed, not removed: the mode-dependence
warning and the "a fixer MUST preserve the `present_match` KEEP" instruction.
Both remain true and remain the guard, now phrased as *the phase-75.1 fixer
preserved it; any future refactor MUST continue to*, with the mutation evidence
cited.

### Step 2 — correction C2 at the `ValueMatcher` block

The contract described the RBAC `ValueMatcher` rule as "a MATERIAL DIVERGENCE
from the existing `HeaderMatcherMode::PresentMatch` (`want ? present : true`)".
After this phase that parenthetical formula is simply wrong.

**The `ValueMatcher` rule itself — "`present_match: false` NEVER matches" — is
CORRECT and was NOT touched** (Trap A). Only the comparison was restated: the
`HeaderMatcher` rule is `(present == want)` since 75.1; the two now AGREE when
the key/header is PRESENT and still DIFFER when it is ABSENT (`ValueMatcher` →
`false`, `HeaderMatcher` → `true`); they are different fields on different
messages and must not be unified.

### Step 3 — the `0078` README

Its "Deferred" bullet documented the invert+absent divergence as live. Rewritten
to record **CF-72-1 CLOSED**, describe the new short-circuit, note that
`present_match` still carries an absent header into the inversion and MUST, and
point at `0083` (route) and 75.2 (access log).

**`0078`'s configs, `expectations.yaml` and probes were NOT touched** — only its
README:

```
 M tests/fixtures/0078-accesslog-header-filter/README.md
```

and the fixture is still GREEN:

```
cargo test -p differential --test access_log_header_filter
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.13s
```

### Step 4 — ADR-0159 verified intact; `DECISIONS.md` UNTOUCHED

ADR-0159 landed at the state-2 PLAN-write (house precedent ADR-0153/0155/0158:
the §6.2 reconciliation ADR ships WITH the plan). State-3 must not re-create it
and must not append an ADR-0160 for work the plan already covers.

```
grep -c '^## ADR-0159' docs/envoy-rust/DECISIONS.md   → 1
grep -c '^## ADR-0160' docs/envoy-rust/DECISIONS.md   → 0
git status --porcelain docs/envoy-rust/DECISIONS.md   → (empty)
git log --oneline -1 -- docs/envoy-rust/DECISIONS.md
  78c37a3 phase 75.1 state-2 PLAN-write: … [ADR-0159]
```

The last commit touching `DECISIONS.md` is the state-2 PLAN-write, not any
commit from this session. Ledger head **ADR-0159**; next available **ADR-0160**.
**No genuinely new decision arose during this implementation**, so no ADR was
appended: every choice was already recorded (the engine shape and its two
rejected alternatives, the pre-validated mutation, the amend-list of three, the
size, the `is_ok_and` change, the empty-value wire-shape caveat, the §7.4
disposition, and the two stale figures).

### Step 5 — citation scope verified BY FILE, never by count

Exactly TWO sites are corrected by this sub-phase, and both are now clean:

```
matcher.rs:51 hits in crates/                          → 0
matcher.rs:51 hits in BEHAVIOR_CONTRACT.md             → 0
```

Every remaining hit is in an APPEND-ONLY artifact and was NOT touched:

```
      8 docs/envoy-rust/DECISIONS.md
      6 docs/envoy-rust/STATE_HISTORY.md
      6 docs/envoy-rust/phases/75.1-.../PLAN.md
      4 docs/envoy-rust/phases/75.1-.../PROGRESS.md
      3 docs/envoy-rust/STATE.md
      3 docs/envoy-rust/phases/75-headermatcher-absence-parity/SPEC.md
      3 docs/envoy-rust/phases/72-accesslog-header-filter/PLAN.md
      2 docs/envoy-rust/phases/75.1-.../SPEC.md
      2 docs/envoy-rust/phases/74-accesslog-metadata-filter/SPEC.md
      2 docs/envoy-rust/phases/72-accesslog-header-filter/SPEC.md
      2 docs/envoy-rust/phases/72-accesslog-header-filter/REVIEW.md
      1 docs/envoy-rust/ROADMAP.md
      1 docs/envoy-rust/phases/74-accesslog-metadata-filter/PLAN.md
      1 docs/envoy-rust/phases/72-accesslog-header-filter/PROGRESS.md
```

**Adjudicated by FILE and by LINE, never by total count.** The count was 26 when
the SPEC was written, 32 at the PLAN-write, and is higher again now because this
session's own `PROGRESS.md` legitimately quotes the defect it fixes. **A rising
count is expected and is evidence of nothing.** The `DECISIONS.md` LINE NUMBERS
the SPEC lists were already stale when written and have drifted twice since, so
this check was done by file, exactly as ADR-0159 correction 2 instructs.

### Step 6 — the §C rewrite stayed inside its boundary

```
2364:**§C Invert + ABSENT — the MODE-SCOPED absence rule (MEASURED; PARITY since phase 75.1).**
2423:**§D Name-only + treat_missing_header_as_empty (PV-5, MEASURED — inherited
```

§D still exists, still immediately follows §C, and its body is byte-unchanged.
The old framing is gone (`grep -c 'the shared-engine fix is carry-forward
**CF-72-1**'` → **0**). The diff touches exactly the two intended regions and
nothing else:

```
@@ -1878,3 +1878,10 @@   (the C2 correction)
@@ -2357,5 +2364,18 @@   (§C)
@@ -2363,15 +2383,39 @@  (§C)
1 file changed, 67 insertions(+), 23 deletions(-)
```

Fixture `0083` re-confirmed green after the doc edits: `1 passed; 0 failed`.

**Commit:** `phase 75.1 task 13: BEHAVIOR_CONTRACT §C rewritten to the measured parity rule (CF-72-1 CLOSED); C2 correction; 0078 README`

---

## Post-task-13 — `cargo fmt` normalisation of the PLAN's literal Rust

**A real finding, not bookkeeping.** After all 13 tasks were committed,
`cargo fmt --all -- --check` **FAILED** in FIVE places — every one of them in
code copied VERBATIM from `PLAN.md`:

| file | what rustfmt changed |
|---|---|
| `crates/envoy-filter/src/fault.rs` | a one-line `assert!(aborts(g, vec![]), "…")` split across lines |
| `crates/envoy-filter/src/jwt_authn.rs` | the `rule_matched` closure re-indented (its param list exceeded the width) |
| `crates/envoy-filter/src/jwt_authn.rs` ×2 | two `rule_matched(...)` call sites split one-arg-per-line |
| `crates/envoy-http1/src/hcm.rs` | the final `compile(..).should_log(..)` chain split one-arg-per-line |

This is memory `plan-md-example-code-trips-clippy` recurring in its `fmt` form:
**the plan's own literal Rust does not satisfy the plan's own gates.** The
PLAN-write pre-flight lints the ENGINE restructure (Task 2) but not the test
bodies of Tasks 5-9, so this was not caught there. It is also exactly the
"CI is often red-at-fmt mid-phase" pattern of memory
`envoy-rust-state4-ci-first-execution`.

Resolved by running `cargo fmt --all` — a pure whitespace/wrapping
normalisation, no semantic change:

```
cargo fmt --all && cargo fmt --all -- --check   → exit 0
 M crates/envoy-filter/src/fault.rs
 M crates/envoy-filter/src/jwt_authn.rs
 M crates/envoy-http1/src/hcm.rs
```

**Doc-comment safety check.** `cargo fmt` does NOT reflow `///` / `//!` / `//`
lines, so a reformatting pass could in principle leave a comment stranded
mid-sentence. `git diff -U0 | grep -cE '^[+-].*///'` = **0** — the normalisation
touched ZERO doc-comment lines, so the hazard of memory
`mechanical-fanout-scripts-corrupt-doc-comments` did not bite.

All four affected tests re-run GREEN after the reformat:

```
fault_header_gate_absence_rule_is_mode_scoped:      1 passed; 0 failed
jwt_rule_header_matcher_absence_rule_is_mode_scoped: 1 passed; 0 failed
access_log_..._through_the_seam:                     1 passed; 0 failed
route_header_matcher_absence_rule_is_mode_scoped:    1 passed; 0 failed
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings` exit `0`,
and `#![forbid(unsafe_code)]` still holds at every crate root (the
"crate roots missing it" grep returned nothing).

> **NOTE FOR THE STATE-4 SESSION.** This is a state-3 convenience run of two
> cheap gates to avoid pushing trivially-red CI. It is **NOT** the §7.5
> verification gate, which is state-4's job and a SEPARATE session (§5.1,
> ADR-0127). `cargo test --workspace`, `cargo deny check`, the full differential
> suite and the conformance suites have **NOT** been run by this session.

---

# §5 STATE-4 VERIFICATION — the §7.5 phase-done gate

> **What this section is.** The §5 **state-4 verification** for sub-phase 75.1,
> run in a SEPARATE session from the state-3 implementation per §5.1 / ADR-0127
> (the context that wrote the code must not grade it). It runs the §7.5
> phase-done gate and quotes every command output verbatim. It did **NOT**
> re-implement, re-plan, edit `DECISIONS.md`, write `REVIEW.md`, or start 75.2.
>
> **Session start state (verified on disk, not inherited).** `git fetch origin
> --prune` → `HEAD == origin/main == ee865747f1e3fd891e53ec2aa8f98a16bb74b7eb`;
> `git status --porcelain` EMPTY; branch `main`; the phase directory holds
> exactly `SPEC.md` + `PLAN.md` + `PROGRESS.md` and **no `REVIEW.md`** — state 4
> exactly. No sibling session had advanced past state-3.

## STEP 0.5 — CI re-confirmed on the FULL 40-char SHA

```
$ gh run list --limit 5 --json databaseId,headSha,status,conclusion
{"conclusion":"success","databaseId":30173334248,
 "headSha":"ee865747f1e3fd891e53ec2aa8f98a16bb74b7eb","status":"completed"}

$ gh run view 30173334248 --json jobs
{"conclusion":"success","name":"fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + ...","steps":13}
{"conclusion":"success","name":"build + test + lint","steps":15}
```

Both jobs `success` at FULL step counts (**15** and **13**) — no runner-starvation
signature (`cancelled` + `runner_name:""` + `steps:0`). Matched on the full
40-char SHA, never a short one.

**This is load-bearing for the adjudication below:** CI on this exact tree is
GREEN, so every local RED recorded here passed on CI hardware.

---

## Gate (e) — build, clippy, fmt, deny

### `cargo build --workspace --all-targets`

```
   Compiling envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.65s
```

Exit **0**; `grep -cE '^(warning|error)'` = **0**.

### `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Exit **0**, ZERO warnings. The first run finished in 1.04 s off 8 `Checking`
lines, which is **partially cached** — memory `mutation-check-needs-forced-rebuild`
says a fast green must not be believed on its face, and memory
`clippy-prints-checking-not-compiling` says the token to grep is `Checking`, NOT
`Compiling` (grepping for `Compiling` here yields a FALSE NEGATIVE). So the eight
files this sub-phase changed were `touch`ed and clippy re-run:

```
$ touch crates/envoy-config/src/{matcher,bootstrap}.rs \
        crates/envoy-accesslog/src/filter.rs \
        crates/envoy-http{1,2}/src/hcm.rs \
        crates/envoy-filter/src/{rbac,fault,jwt_authn}.rs
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Checking envoy-accesslog v0.0.0 ...
    Checking envoy-config v0.0.0 ...
    Checking envoy-filter v0.1.0 ...
    Checking envoy-http1 v0.0.0 ...
    Checking envoy-http2 v0.0.0 ...
    Finished `dev` profile in 1.98s
CLIPPY_FORCED_EXIT=0   Checking lines: 14   warnings/errors: 0
```

**14** crates genuinely re-analysed, including EVERY crate this sub-phase touched.
The green is real, not a cache replay.

### `cargo fmt --all -- --check`

Exit **0**, silent. (State-3 had to run `cargo fmt --all` to fix five sites copied
verbatim from `PLAN.md`; that normalisation is committed and holds.)

### `cargo build -p envoy-bin` — BEFORE any differential

Exit **0**. `target/debug/envoy-bin` rebuilt this session. **Not optional** — the
harness runs the DEBUG binary and a stale one silently mis-reports a fixture
carrying a new config key (memory `differential-harness-uses-debug-envoy-bin`).

### `cargo deny check`

```
advisories ok, bans ok, licenses ok, sources ok
```

Exit **0**. Four `warning[license-not-encountered]` lines (`MPL-2.0`,
`Unicode-DFS-2016`, `Zlib`, and one more) are pre-existing unmatched allow-list
entries in `deny.toml`, not findings. No freshly-published RustSec advisory bit
this run (the hazard of memory `cargo-deny-reds-on-unrelated-advisory`).

---

## Gate (e) continued — `cargo test --workspace --no-fail-fast`, run TWICE

Run with `--no-fail-fast` and the FULL output redirected to a file, never piped
through `tail` (a bare `cargo test --workspace` aborts at the first failing
BINARY and never exercises the rest of the gate; `tail` truncates the `failures:`
block and destroys the failing test names — memories
`local-red-set-varies-run-to-run` and `never-pipe-verification-runs-through-tail`).

**Both runs produced IDENTICAL totals across 160 test binaries:**

```
run 1:  passed=2096  failed=7  ignored=9      (TESTS_EXIT=101)
run 2:  passed=2096  failed=7  ignored=9
```

The 9 `ignored` are pre-existing `#[ignore]`d benchmarks (`bench_date_contended`,
`bench_parse_request_get`, …), not skips introduced here.

### The failing SET is NOT stable between runs — which is the whole diagnosis

| test | run 1 | run 2 | isolation |
|---|---|---|---|
| `access_log_rf_upstream_reset` | FAILED | FAILED | **FAILED** |
| `access_log_rcd_upstream_reset` | FAILED | FAILED | **FAILED** |
| `access_log_h2_uc_upstream_reset` | FAILED | FAILED | **FAILED** |
| `access_log_h2_rcd_upstream_reset` | FAILED | FAILED | **FAILED** |
| `admin_config_dump_server_info` | FAILED | FAILED | **FAILED** |
| `upstream_active_health_check_fixture` | FAILED | ok | **1 passed** |
| `upstream_circuit_breaker_budgets_fixture` | FAILED | ok | **1 passed** |
| `network_filter_direct_response_fixture` | ok | FAILED | **1 passed** |
| `network_filter_rbac_allow_fixture` | ok | FAILED | **1 passed** |

This is exactly the two-family split memory `local-red-set-varies-run-to-run`
describes: **a deterministic environmental CORE plus a varying parallel-load
TAIL.** Every isolation re-run named its target binary and was adjudicated on the
`N passed` COUNT, never on the exit code (memory
`cargo-test-p-name-false-green-filtered-out`).

> **One isolation invocation was itself wrong and was caught by reading the
> text.** `cargo test -p differential --test network_filter_rbac` exited **101**
> — not a test failure but `error: no test target named 'network_filter_rbac' in
> 'differential' package` (that target lives in `envoy-bin`; the fixture's real
> binary is `network_filter_rbac_allow`). Re-run against the correct target it is
> `1 passed`. An exit code alone would have recorded a phantom RED.

### FAMILY 1 — deterministic in isolation = the ENVIRONMENTAL signature (5 tests)

**(1a) The four upstream-reset fixtures** — `TcpCloseBackend` IPv6-unreachable
family (memory `tcpclosebackend-ipv6-unreachable-host-flake`, fixtures
`0061`/`0062`/`0069`). The failure text is decisive:

```
envoy=      {"rc":503,"rcd":"upstream_reset_before_response_started{
              remote_connection_failure|immediate_connect_error:_Network_is_unreachable|
              remote_address:[fdc4:f303:9324::254]:34937}","rf":"UF"}
envoy-rust= {"rc":503,"rcd":"upstream_reset_before_response_started{
              connection_termination}","rf":"UC"}
```

**Real Envoy is the side that is wrong here**, and for a host reason: it resolved
the close backend to an **IPv6** address (`[fdc4:f303:9324::254]`) and got
`Network is unreachable`, so it logged a connect-failure `UF` instead of ever
reaching the backend and observing the reset. envoy-rust logged the CORRECT
`UC` / `connection_termination`. Nothing about this touches `HeaderMatcher`.

**(1b) `admin_config_dump_server_info`** — the `192.168.65.2` bridge-IP family
(memory `differential-host-bridge-ip-192-168-65-2`):

```
envoy-only:      ["backend::192.168.65.2:32965::canary::false", …
                  "backend::192.168.65.2:32965::hostname::host.docker.internal", …]
envoy-rust-only: []
```

This host routes the backend via `192.168.65.2`, which is not in the allow-list.

Both sub-families fail DETERMINISTICALLY in isolation — and per the standing
traps that determinism **is** the environmental signature, not a regression.

### FAMILY 2 — passes in isolation = parallel-load startup flake (4 tests)

All four share ONE failure text, and it is a **startup** failure that never
reached a semantic assertion:

```
Error: upstream Envoy never became accept-ready
Caused by: 127.0.0.1:55336 not accept-ready within 10s: Connection refused (os error 111)
```

This is the documented port-reuse startup-race family (memory
`differential-fixtures-flake-under-parallel-load`;
`network_filter_direct_response_fixture` is named explicitly in the STATE.md
standing-traps flake set). Reading the failure TEXT rather than the exit code is
what distinguishes it from a semantic RED (memory
`mutation-red-needs-unmutated-control`).

**`upstream_active_health_check` and `upstream_circuit_breaker_budgets` were NOT
in the documented flake set**, so they were treated as candidate regressions and
checked structurally rather than assumed:

```
$ grep -nE 'invert_match|present_match|^\s*headers:' \
      tests/fixtures/0019-upstream-active-health-check/envoy{,-rust}.yaml \
      tests/fixtures/0025-upstream-circuit-breaker-retry-budget/envoy{,-rust}.yaml
    NONE (0 hits across 2 config files)   # per fixture
```

Both fixtures carry **ZERO** header-matcher constructs in either config, so they
cannot exercise `HeaderMatcher::matches` at all — structurally outside this
sub-phase's blast radius. The fixture directories were resolved from the test
entrypoints rather than from a glob, **because the first attempt used a glob that
did not resolve and returned a vacuously clean `0 hits`** (the exact trap the
standing traps warn about: assert the number of files actually scanned).

### Adjudication — no failure is attributable to sub-phase 75.1

1. All 9 distinct failures across both runs are differential/admin fixtures whose
   failure text is either a host-networking mismatch (IPv6 / bridge IP) or a
   container startup timeout. **None is an assertion about matching behavior.**
2. **CI on this exact SHA (`ee86574…`) is GREEN** with both jobs at full step
   counts — every one of these passed on CI hardware.
3. The two failures NOT already documented were shown structurally incapable of
   reaching the changed engine, and both pass in isolation at `1 passed`.

---

## Gate (a) — the new fixture is green

```
$ cargo test -p differential --test headermatcher_absence_parity
running 1 test
INFO envoy-rust listening (http_connection_manager) addr=127.0.0.1:37839
     stat_prefix=ingress_http codec_type=HTTP1
test headermatcher_absence_parity_fixture ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`0083-headermatcher-absence-parity` — **`1 passed` in BOTH full runs.** All 22
probes across 8 matchers green cross-proxy, including the load-bearing
`p07-absent-keeps-GUARD` (P1) and the `p12` D2 cell.

State-3 already audited this fixture's ~1 s green as genuine (a `docker ps` poll
caught the real `envoyproxy/envoy:v1.33.0` container, and a negative control
failed quoting upstream's actual response bytes), so it is not re-litigated here.

## Gate (b) — the pre-existing corpus is green (THE DOMINANT RISK)

This was the phase's dominant risk: a shared-engine change under five subsystems,
where a pre-existing fixture or test could silently have depended on the old
semantics. **It did not happen.** Every fixture on the `SPEC.md` §10 watch-list is
green in BOTH runs:

```
test http1_direct_response_fixture ... ok     # 0007 — the other route-header witness
test http_filter_rbac_fixture ... ok          # 0017
test http_filter_fault_fixture ... ok         # 0018
test access_log_header_filter ... ok          # 0078
test access_log_metadata_filter ... ok        # 0081
test access_log_metadata_filter_key_not_found ... ok   # 0082
```

and the in-process backstops (`http_filter_rbac_in_process_backstop`,
`http_filter_fault_in_process_backstop`, `http1_direct_response_round_trip`) are
green too. **PV-9's prediction — that the complete break set is FOUR in-process
assertions across THREE tests in ONE file, all amended by Task 2, with ZERO
fixture risk — is CONFIRMED by measurement.**

## Gate (c) — conformance at the declared threshold

```
     Running tests/h2spec_runner.rs
test tests::parse_summary_line_extracts_pass_fail_counts ... ok
test tests::parse_h2spec_output_extracts_section_failure_ids ... ok
test h2spec_pass_rate_gate ... ok
```

`h2spec_pass_rate_gate` **ok in BOTH runs**. `known-failures.txt` is still **21**
lines and was **NEVER trimmed** — this host scores h2spec 3.5/2 as PASS, so
trimming on local evidence would break CI (memory
`h2spec-3-5-2-preface-host-sensitive`).

## Gate (d) — VACUOUS BY DESIGN, and CONFIRMED rather than assumed

```
new fuzz targets added since 2856976 : 0     (5 targets total, unchanged)
.github/workflows/ci.yml commits      : 0     (untouched)
tracked parse_bootstrap corpus seeds  : 63    (unchanged)
```

No new parser/codec/filter and no new config surface, so §7.4 requires no new
target, seed or `ci.yml` step — and each was verified on disk rather than
inherited from the plan.

## Gate (f) — REVIEW.md

**Not this session.** State-5 is a SEPARATE session (§5.1 / ADR-0127).

---

## Ledger and census invariants — all re-verified on disk

```
fixture directories                    83     (82 + 0083)
tracked parse_bootstrap corpus seeds   63
known-failures.txt                     21 lines
fuzz targets                            5     (0 new)
ADR-0159 present / ADR-0160 present    1 / 0
last commit touching DECISIONS.md      78c37a3  (the state-2 PLAN-write)
#![forbid(unsafe_code)] missing from   (none)
ROADMAP rows 75 / 75.1 / 75.2          in-progress / in-progress / planned
```

**`DECISIONS.md` is UNTOUCHED by this session.** No genuinely new decision arose
at the gate: every RED resolved to an already-documented host-environment family
or to a parallel-load startup flake, neither of which is a decision. Ledger head
stays **ADR-0159**; next available **ADR-0160**. **No `ROADMAP.md` edit** — row
`75.1` flips `done` at state-6, not here.

The engine's arm order was re-confirmed on disk (the P1 guard shape ADR-0159
decided — `(_, None)` sits AFTER the `PresentMatch` arm, never before it):

```
26:        let mode_result = match (&self.mode, value) {
31:            (HeaderMatcherMode::PresentMatch(want_present), v) => v.is_some() == *want_present,
35:            (_, None) => return false,
```

The parallel workstream's four `.claude/worktrees/agent-*` and its long-running
`quizzical_goldstine` container (`envoyproxy/envoy:v1.33.0`) were observed and
**LEFT ALONE**; this session created no worktrees and no containers of its own
beyond those testcontainers manages per fixture.

## §7.5 verdict

| gate | verdict |
|---|---|
| (a) new/changed differential fixtures green | **PASS** — `0083` = `1 passed` in both runs |
| (b) pre-existing differential fixtures green | **PASS** — whole watch-list green; all REDs adjudicated to documented host families, none attributable to 75.1 |
| (c) conformance at declared threshold | **PASS** — `h2spec_pass_rate_gate` ok, `known-failures.txt` untrimmed at 21 |
| (d) new fuzzer short-budget run | **VACUOUS BY DESIGN, CONFIRMED** — 0 new targets/seeds, `ci.yml` untouched |
| (e) build / clippy / fmt / test / deny | **PASS** — 0/0/0/`2096 passed`/`ok`; clippy green re-proved under a forced re-check |
| (f) `REVIEW.md` approved | **state-5, a SEPARATE session** |

**Next: 75.1's §5 state-5 CODE REVIEW** (`superpowers:requesting-code-review`), a
SEPARATE session per §5.1 / ADR-0127 — the context that ran this gate must not be
the one that reviews it.
