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
