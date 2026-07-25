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
