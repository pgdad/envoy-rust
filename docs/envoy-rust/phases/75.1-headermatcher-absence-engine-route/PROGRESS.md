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
