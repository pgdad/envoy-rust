# Sub-phase 76.2 — PROGRESS (§5 state 3, the implementation)

> **What this file is.** The running log of the §5 **state-3** implementation of sub-phase
> `76.2-redirect-runtime-fixture`, appended **on each task completion** with **real, quoted
> command output** — never reconstructed at the end. Its input is `PLAN.md` (2371 lines, 12
> TDD-ordered tasks); its siblings are `SPEC.md` (556 lines) and, once state 5 runs, `REVIEW.md`.
>
> Written for a reader with **zero prior context** (doctrine D-3.4).
>
> **This session does NOT verify.** State 4 is a separate session (§5.1; ADR-0127 — a verifier
> must not grade what it ran). The commands quoted below are the per-task TDD evidence that each
> task is RED-then-GREEN, **not** the §7.5 gate adjudication.

## Session preamble

**Cold start (disk-authoritative).** `git status --porcelain` clean; branch `main`; `HEAD` at
`0ea2de1cd6a992af7916ba1b736e3c0180069f00`, equal to `origin/main` after
`git fetch origin --prune`. `docs/envoy-rust/phases/76.2-redirect-runtime-fixture/` holds
`SPEC.md` + `PLAN.md` and **no** `PROGRESS.md` and **no** `REVIEW.md` — §5 state 3's unambiguous
detection rule. ROADMAP row `76.2` is `planned`, `76.1` is `done`, parent `76` is `in-progress`
(re-measured by splitting each row on `' | '` and reading **field 4**, not field 5).

**Execution stance: SOLO-SERIAL, no subagents.** The handoff permits a `{1, 8, 9, 10, 12}` fan-out
alongside the `2→3→4→5→6/7` chain. Rejected on measured grounds: `PLAN.md` mandates **one commit
per task**, which makes the git index a shared mutable resource across agents, and the project's
own standing record documents a parallel agent's `git checkout` silently reverting an in-place
mutation. Tasks 1-6 also all touch `crates/envoy-http1/src/hcm.rs` or its crate, so the cargo lock
would serialize the builds regardless. Coordination cost exceeds the win.

## Deviations from `PLAN.md` — recorded here because `PLAN.md` is this state's INPUT, not its output

`PLAN.md` must not be edited (it is the state-2 artifact). Where the plan's literal text could not
be transcribed verbatim, the deviation is recorded at the task that hit it, with the measurement
that forced it. Running list:

| # | task | deviation | why |
|---|---|---|---|
| D-1 | 2 | `PLAN.md` Task 2 Step 3's expectation *"success, no `unused_imports` warning"* is **REFUTED**. Standalone, Task 2 leaves `RedirectAction` imported-but-unused in the non-test lib build, so `clippy -D warnings` exits **101** at this one boundary. | The import is consumed only by Task 3's `plan_redirect`. The plan's §2 pre-flight applied Tasks 1-5 **together** in one worktree, so it never observed the standalone state. Accepted rather than "fixed": Task 2 exists precisely so Task 3's RED is unambiguous, and inventing an `#[allow(unused_imports)]` would be cruft removed one commit later. **Self-closes at Task 3.** |

---

## Task 1 — `canonical_reason` 303/307/308 + the `location` header constant

**Status: COMPLETE.** Commit message: `phase 76.2 task 1: canonical_reason 303/307/308 + the location header constant`.

### Anchors re-verified on disk before transcribing (never trust an inherited number)

`PLAN.md` §1's citations for this task all reproduced **exactly** at `0ea2de1`:

- `crates/envoy-http1/src/response.rs` — `canonical_reason` fn at `:188`, `301 => "Moved
  Permanently"` at `:195`, `302 => "Found"` at `:196`, `_ => "OK"` at `:213`, `mod tests` at `:218`.
- `crates/envoy-http1/src/headers.rs` — **7** name constants (`HOST`, `CONTENT_LENGTH`,
  `CONNECTION`, `SERVER`, `DATE`, `TRANSFER_ENCODING`, `CONTENT_TYPE`) and **no** `LOCATION`.
  Also measured, and relevant to Task 5: `find_header` lives in **`headers.rs:13`**, not in `hcm.rs`.

### Step 2 — the test RUN RED, before any implementation line (D-3.1)

```
$ cargo test -p envoy-http1 --lib -- canonical_reason_covers_the_three_redirect_codes
exit=101
   Compiling envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
     Running unittests src/lib.rs (target/debug/deps/envoy_http1-1586217c9fbd1c1a)

running 1 test
test response::tests::canonical_reason_covers_the_three_redirect_codes ... FAILED

---- response::tests::canonical_reason_covers_the_three_redirect_codes stdout ----
thread '...' panicked at crates/envoy-http1/src/response.rs:513:9:
assertion `left == right` failed
  left: "OK"
 right: "See Other"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 187 filtered out
```

This is the exact RED text `PLAN.md` Task 1 Step 2 predicted. The `Compiling envoy-http1` line is
quoted deliberately: a stale test binary gives a FALSE result, so the rebuild is part of the
evidence, not noise.

### Steps 3-4 — implementation

- `response.rs`: `303 => "See Other"`, `307 => "Temporary Redirect"`, `308 => "Permanent
  Redirect"` added around the existing `304 => "Not Modified"`, with the measured-provenance
  comment. Transcribed verbatim from `PLAN.md`.
- `headers.rs`: `pub const LOCATION: &str = "location";` after `CONTENT_TYPE`, with the doc comment
  warning never to add it to the harness's `HEADER_ALLOW_LIST`. Transcribed verbatim.

### Step 5 — GREEN, and the two gates

```
$ cargo test -p envoy-http1 --lib -- canonical_reason
test-exit=0
test response::tests::canonical_reason_covers_the_three_redirect_codes ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 187 filtered out; finished in 0.00s
```

Asserted on the **count** (`1 passed`), never on the exit code — `0 passed; N filtered out` is a
false green that also exits 0.

```
$ cargo fmt --all -- --check
fmt-exit=0 bytes=0

$ cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings
clippy-exit=0 Checking-lines=6
```

`fmt` exit 0 with **zero bytes** of output — the plan's literals are already canonical and
transcribed verbatim. Clippy exit 0 with **6** `Checking` lines: the line count is asserted
separately because a clippy exit 0 with **zero** `Checking` lines is a fully-cached no-op, not
evidence. (Clippy prints `Checking`; build and test print `Compiling`.)

---

## Task 2 — widen the `envoy_config` import so `RedirectAction` is nameable outside tests

**Status: COMPLETE.** Commit message: `phase 76.2 task 2: import RedirectAction into hcm.rs's non-test body`.

**Why this is its own task.** `76.1` imported `RedirectAction` **only** inside
`#[cfg(test)] mod tests`. Adding `plan_redirect` to the non-test body therefore fails with
`error[E0425]: cannot find type RedirectAction in this scope` — and **a missing-import compile
error is textually indistinguishable from a missing-function one**, which would make Task 3's TDD
RED ambiguous. Splitting it out is what keeps Task 3's RED able to mean only one thing.

### Anchors re-verified on disk (all exact at `6c0fcd2`)

- `crates/envoy-http1/src/hcm.rs:11-14` — the top-level `use envoy_config::{…}`, containing
  `AttemptOutcome, DirectResponse, HashPolicy, HttpConnectionManagerConfig, RetryConfig, Route,
  RouteAction, RouteConfiguration, VirtualHost` and **not** `RedirectAction`.
- `:2360` — the test module's `use super::*;` (the previously-banked `:2353` had itself drifted).
- `:2361-2363` — the test module's own `use envoy_config::{…, RedirectAction, …}`, now redundant
  because `use super::*;` re-exports the widened top-level import.
- Also measured, and consumed by Task 5: `find_header` is already imported into `hcm.rs` at
  **`:7`** (`use crate::headers::{self, find_header};`), so Task 5's arm needs no new import.

### Steps 1-2 — the two edits

Both transcribed verbatim from `PLAN.md`. The top-level list gains `RedirectAction` (rustfmt's
canonical re-wrap puts it at the end of line 2 of the braced list); the test-module import drops
it and collapses from three lines to one.

### Step 3 — verification, and a REFUTED plan expectation (deviation **D-1**)

```
$ cargo build -p envoy-http1 --all-targets
build-exit=0
   Compiling envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
warning: unused import: `RedirectAction`
warning: `envoy-http1` (lib) generated 1 warning

$ cargo fmt --all -- --check
fmt-exit=0 bytes=0

$ cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings
clippy-exit=101
    Checking envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
error: unused import: `RedirectAction`
error: could not compile `envoy-http1` (lib) due to 1 previous error
```

`PLAN.md` Task 2 Step 3 predicted *"success, no `unused_imports` warning."* **Measured: there IS
one**, and under `-D warnings` it is a hard error. The build itself is exit 0 — this is a lint
gate, not a build break.

**Root cause, and why the plan got it wrong.** The `#[cfg(test)]` build consumes `RedirectAction`
through `use super::*;`, but the plain lib build has no consumer until Task 3 lands
`plan_redirect`. The plan's §2 pre-flight applied Tasks 1-5 **as one patch**, so the standalone
Task-2 state never existed there. This is the same class of error the project has caught fifteen
times: **an expectation inherited from a bundled measurement does not survive being unbundled.**

**Disposition:** accepted and recorded as deviation **D-1**, not papered over. Adding
`#[allow(unused_imports)]` would be cruft deleted one commit later, and folding Task 2 into Task 3
would destroy the disambiguation the task exists for. The boundary closes at Task 3, where
`plan_redirect` consumes the import — verified there.
