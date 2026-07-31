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
| D-1 | 2, 3, 4 | `PLAN.md`'s Global Constraint *"`clippy -D warnings` must be clean at every task boundary"* is **NOT ACHIEVABLE under the plan's own task split**, and Task 2 Step 3's *"no `unused_imports` warning"* is **REFUTED**. Measured: Task 2 leaves `RedirectAction` unused (`clippy` exit **101**); Task 3 leaves `RedirectPlan`/`plan_redirect` dead (`never constructed` / `never used`, exit **101**); Task 4 will leave `synth_redirect` dead. | Structural, one root cause: Tasks 2-4 each add a **non-test** item whose only **non-test** consumer is Task 5's dispatch arm. The plan's §2 pre-flight applied Tasks 1-5 as **one patch**, so the unbundled intermediate states never existed there. Accepted, not papered over — `#[allow(dead_code)]` would be cruft deleted at Task 5, and folding 2-4 into 5 would destroy the TDD granularity and the unambiguous REDs. **Self-closes at Task 5**, verified there. `cargo build` and `cargo fmt --check` stay green throughout; only the lint gate is transiently red. |
| D-2 | 3 | `PLAN.md` T3-3's literal `Some("/é"[..2].into())` **panics in the test itself** and never reaches `plan_redirect`. Replaced with a 2-byte ASCII prefix, `Some("ab")`. | `"/é"` is **3 bytes** (`/`=1, `é`=2), so byte index 2 is **not a char boundary** and `str` slicing there aborts. The replacement witnesses the identical cell honestly: `matched_len` is 2, `"/é".get(2..)` lands mid-`é` and returns `None`, so the `unwrap_or("")` inside `plan_redirect` — the very thing being tested — is what keeps the function total. The plan's §2 pre-flight **ran only two representative tests**, so a runtime panic in a third was invisible to it (`fmt`/`clippy` do not execute tests). |
| D-4 | 5 | `PLAN.md` Task 5 Step 2's prediction *"the compile fails first on `&mut req` (the signature is still `&Request`)"* is **REFUTED** — it compiles, and the RED is the assertion `left: 501, right: 301`. No code change; the plan's expected-output text is simply wrong. | Rust **coerces `&mut T` to `&T`** at a call site, so passing `&mut req` to a fn still declared `&Request` is legal. The observed RED is strictly *better* evidence than the plan expected: a pure behavioural flip of the placeholder, mirroring the pre-flight's `left: 301, right: 501` from the other side. Recorded so a reviewer comparing the plan's expected text against `PROGRESS.md` does not read the difference as a skipped step. |
| D-5 | 7 | `PLAN.md` T7-1's literal `version: envoy_http1::codec::HttpVersion::Http2` **does not exist**. Uses `envoy_http1::HttpVersion::Http11`. | MEASURED: `HttpVersion` (`crates/envoy-http1/src/codec.rs:14-17`) has **exactly two** variants, `Http10` and `Http11` — there is no `Http2`. The sibling H2 test `h2_resolve_route_reachable_and_returns_cors_route` likewise builds its `envoy_http1::Request` with `Http11`; the field does not participate in route dispatch. Also corrected: the re-export path is `envoy_http1::HttpVersion` (`lib.rs:28`), not `envoy_http1::codec::…`. This is the ONE task the plan flagged as **not pre-flighted end-to-end**, and this is exactly the class of error that predicts. The plan's trailing `let _ = (…)` scaffolding line was deleted as it instructs, the imports being genuinely consumed. |
| D-6 | 10 | `PLAN.md` Task 10 Step 5's validation command `./target/debug/envoy-bin --mode validate -c <path>` **does not exist**. Replaced with a real boot-validate: substitute `{{PORT}}`, run `envoy-bin -c <file>` under `timeout`, and require zero errors. | MEASURED: `envoy-bin` exits **2** with `unknown argument: --mode`. Confirmed by reading the parser — `crates/envoy-bin/src/argv.rs:27-29` says *"Phase 01 accepts exactly one flag: `-c <path>` or `--config-path <path>`"* (clap is deliberately avoided as not on the D-3.2 permitted-foundations list). The project's own standing-traps ledger already records "envoy-bin takes only `-c <path>`", so `PLAN.md` contradicts a banked fact. The replacement is strictly stronger: it proves the config parses **and** the listener binds. |
| D-3 | 3 | `PLAN.md` T3-2's assert message `"a bare redirect{} rewrites nothing"` **does not compile**. Escaped to `redirect{{}}`. | `assert_eq!`'s third argument is a **format string**, so the bare `{}` is parsed as a positional placeholder: `error: 1 positional argument in format string, but no arguments were given`. Rendered output is unchanged. Note this contradicts `PLAN.md` §2's claim that the Task-3 block passed `clippy -D warnings` — a block that does not compile cannot have. Recorded as a measured fact about the plan, not repaired in `PLAN.md` (D-3.5: it is this state's input). |

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

---

## Task 3 — the pure `location`-builder: `RedirectPlan` + `plan_redirect`

**Status: COMPLETE.** Commit message: `phase 76.2 task 3: the pure location-builder — all 22 MEASURED cells pinned`.

This is the phase's centre of gravity: one pure, total function encoding the whole MEASURED
upstream rule set, so all 22 cells are unit-testable **without a socket**.

### Anchors re-verified on disk (at `c7d3735`)

- Insertion point: `synth_status` ends at `hcm.rs:2225`; the next item is
  `synth_no_healthy_upstream`, whose **doc block starts at `:2227`** (`/// 12.2 (parent-12 D6.2 per
  ADR-0037): …`). `PLAN.md` is explicit that the insert goes **above that doc block, never between
  it and its function** — this is the exact hazard that produced `76.1`'s M-1. Honoured: verified
  after the edit that `/// 12.2 …` still sits immediately above `pub(crate) fn
  synth_no_healthy_upstream`.
- `strip_port` at `hcm.rs:2146` ✓ (consumed by rule (b)).
- `76.1`'s types, re-read rather than assumed: `RedirectAction` at `bootstrap.rs:2226` derives
  **`Default`** (`:2224`) — required by the table's `RedirectAction::default()` rows;
  `strip_query` is a bare `bool` (`:2240`); `response_code` is `RedirectResponseCode` (`:2242`);
  the enum's five variants are `MovedPermanently`/`Found`/`SeeOther`/`TemporaryRedirect`/
  `PermanentRedirect` (`:2185-2192`) with `status()` at `:2198`.

### Step 2 — RUN RED, twice, and the second run is the honest one

First run surfaced **two** distinct errors — the intended one plus a defect in the plan's own
literal (deviation **D-3**):

```
exit=101
   Compiling envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
error: 1 positional argument in format string, but no arguments were given
error[E0425]: cannot find function `plan_redirect` in this scope   (x6)
error: could not compile `envoy-http1` (lib test) due to 7 previous errors
```

After escaping `{}` → `{{}}`, the RED is **unambiguous**, which is the whole point of Task 2
existing:

```
$ cargo test -p envoy-http1 --lib -- plan_redirect
exit=101
      6 error[E0425]: cannot find function `plan_redirect` in this scope
      1 error: could not compile `envoy-http1` (lib test) due to 6 previous errors
```

Six errors, all the same, all "the function does not exist". Because Task 2 already landed the
import, this error **can only mean** the function is missing — a missing-import error would have
been textually identical, and that is precisely why the plan split Task 2 out.

### Step 3 — implementation, transcribed verbatim

`RedirectPlan` + `plan_redirect` inserted verbatim from `PLAN.md` (which `cargo fmt --check`
confirms was already canonical — see below). Rules (a) scheme, (b) the authority asymmetry,
(c) path, (d) query, (e) status, each carrying its MEASURED provenance comment.

### Step 4 — GREEN

```
$ cargo test -p envoy-http1 --lib -- plan_redirect
exit=0
   Compiling envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
test hcm::tests::plan_redirect_is_total_on_degenerate_spans ... ok
test hcm::tests::plan_redirect_reports_a_rewritten_path_only_for_prefix_rewrite ... ok
test hcm::tests::plan_redirect_matches_every_measured_location_cell ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 188 filtered out; finished in 0.00s
```

**`3 passed`** — asserted on the count, never the exit code. All **22** measured cells pass, and
the test itself asserts `cells.len() == 22` so a silently dropped row fails loudly.

The 22 cells were kept **table-driven**, per `PLAN.md` §4's binding mitigation: written as 22
house-style `#[test]` fns this group would grow from ~255 to ~400 LoC and consume the entire §6.1
headroom. Each row carries its own `label`, so attribution is not lost — proved by the mutation
check below, which names its cell exactly.

```
$ cargo fmt --all -- --check
fmt-exit=0 bytes=0
```

### Step 5 — MUTATION CHECK: the authority asymmetry really is pinned

The single most likely from-scratch mistake is treating `host_redirect` symmetrically with the
scheme change. Run under full hygiene: **the clean work was committed FIRST** (`d53a38b`) and the
mutation applied in a **scratch `git worktree --detach` at that commit** — never in the main tree,
because a parallel agent's `git checkout` can silently revert an in-place mutation, and four
sibling `.claude/worktrees/agent-*` worktrees were live during this session.

**Control first, from the same worktree** (a RED that never reached an assertion is not evidence):

```
$ git worktree add --detach <scratch> d53a38b
worktree at: d53a38b372c88b19ecdacce520fc0f6e9ba70506
$ cargo test -p envoy-http1 --lib -- plan_redirect_matches_every_measured_location_cell
CONTROL exit=0
   Compiling envoy-http1 v0.0.0 (<scratch>/crates/envoy-http1)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 190 filtered out
```

Mutation applied — `let host_part = rd.host_redirect.as_deref().unwrap_or(authority);` →
`let host_part = authority;`:

```
MUTATED exit=101
   Compiling envoy-http1 v0.0.0 (<scratch>/crates/envoy-http1)
test hcm::tests::plan_redirect_matches_every_measured_location_cell ... FAILED
thread '...' panicked at crates/envoy-http1/src/hcm.rs:10573:13:
assertion `left == right` failed: cell R1 host_redirect replaces the authority: location
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 190 filtered out
```

**RED, naming `cell R1 host_redirect replaces the authority: location`** — the exact cell
`PLAN.md` Step 5 predicted, which also demonstrates the table's per-row `label` preserves
attribution. `Compiling envoy-http1` on both runs proves neither used a stale binary.

Post-run integrity re-grep (a sibling agent can revert a mutation mid-run):

```
mutation still present? -> 1 (expect 1)
original still absent?  -> 0 (expect 0)
```

Worktree removed; `git worktree list` re-checked from the repo root shows only the main tree and
the **four pre-existing sibling `agent-*` worktrees, left untouched**. Main tree re-verified
unmutated (`grep -c` of the original line → **1**) and `git status --porcelain` clean.

### Lint gate at this boundary — deviation **D-1**, widened

```
$ cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings
clippy-exit=101 Checking=1
error: struct `RedirectPlan` is never constructed
error: function `plan_redirect` is never used
```

Same root cause as Task 2's: the only **non-test** consumer of `plan_redirect` is Task 5's
dispatch arm. `cargo build` and `cargo fmt --check` are green; the lint gate is transiently red
across Tasks 2-4 and **closes at Task 5**. See deviation **D-1** for why this is accepted rather
than suppressed.

---

## Task 4 — `synth_redirect`, the dedicated response builder

**Status: COMPLETE.** Commit message: `phase 76.2 task 4: synth_redirect — five headers, no content-type`.

**Why a dedicated builder exists at all.** MEASURED under the harness's exact request shape (a raw
`GET <target> HTTP/1.1` with `Host:` and `Connection: close`), a **redirect** carries
`location`, `date`, `server`, `connection`, `content-length` — and **no `content-type`** — whereas
a `direct_response` **does** carry one. The shared `synth_with` always emits `content-type`. Had
the redirect arm reused it, `diff_headers` would bail on its **first** check, the lowercased
name-set equality, with `only-in-envoy-rust=["content-type"]`, and fixture `0086` would be red for
a reason having nothing to do with `location`. `synth_overflow` is the established in-repo
precedent for a synth path owning its own header list.

### Anchors re-verified on disk

The three helpers the literal consumes all exist in `hcm.rs`: `DEFAULT_SERVER_NAME` (`:21`),
`now_imf_fixdate` (`:2176`), `connection_value` (`:2180`). `headers::LOCATION` came from Task 1.

### Step 2 — RUN RED

```
$ cargo test -p envoy-http1 --lib -- synth_redirect_emits_five_names
exit=101
      1 error[E0425]: cannot find function `synth_redirect` in this scope
      1 error: could not compile `envoy-http1` (lib test) due to 1 previous error
```

Single, unambiguous error — the function does not exist.

### Step 4 — GREEN

```
$ cargo test -p envoy-http1 --lib -- synth_redirect
exit=0
   Compiling envoy-http1 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http1)
test hcm::tests::synth_redirect_emits_five_names_and_no_content_type ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 191 filtered out; finished in 0.00s

$ cargo fmt --all -- --check
fmt-exit=0 bytes=0
```

The test asserts the header names as an **ordered `Vec`**, not a set, so it pins the measured
upstream wire order as well as the absence of `content-type`; it also pins `content-length: 0`
value-exact, because `diff_headers` compares that value byte-exact too.

### Doc-comment orphan check — the M-1 hazard, checked MECHANICALLY

Tasks 3 and 4 both inserted immediately above an item that **has** a doc block
(`synth_no_healthy_upstream`'s `/// 12.2 (parent-12 D6.2 per ADR-0037): …`). `76.1`'s M-1 was
caused by exactly this move done blind. Verified after both insertions that the block's last line
still sits directly above its own function:

```
$ grep -n -B1 '^pub(crate) fn synth_no_healthy_upstream' crates/envoy-http1/src/hcm.rs
2362-/// paths keep `synth_status`'s empty body.
2363:pub(crate) fn synth_no_healthy_upstream(close: bool) -> Response {
```

Nothing orphaned. Note **no gate catches this** — `envoy-config`/`envoy-http1` enable no
`missing_docs` lint and `cargo fmt` does not reflow doc comments — so the check has to be run
deliberately, by grep, every time.

**Lint gate:** still transiently red per deviation **D-1** (`synth_redirect` now also has no
non-test consumer until Task 5). Closes at Task 5.

---

## Task 5 — the real dispatch arm + `&mut Request` + the deliberate flip of T-C9

**Status: COMPLETE.** Commit message: `phase 76.2 task 5: the real redirect dispatch arm + &mut Request; T-C9 deliberately flipped`.

The integration task: `plan_redirect` (Task 3) and `synth_redirect` (Task 4) are wired into the
single `match &route.action` seam, which serves **both codecs** because HTTP/2 has no route-action
dispatch of its own.

### The call-site census — the plan's REFUTATION independently reproduced

`76.2/SPEC.md` claimed "**8** `build_response` call sites"; `PLAN.md` refuted that to
"**7 call sites + 2 definitions**". Re-measured here by grep, and the plan is right:

| site | file |
|---|---|
| definition | `hcm.rs:2039` `pub fn build_response` |
| definition | `hcm.rs:2051` `pub(crate) fn build_response_in` |
| 1 | `hcm.rs:919` `build_response_in(&route_snapshot, …)` |
| 2 | `hcm.rs:2045` `build_response_in(&config.current_route_config(), …)` — text unchanged; `req` is now `&mut Request` and reborrows |
| 3-5 | `hcm.rs:9860` / `:9887` / `:9904` — the **three** in-file test call sites (the SPEC said two; `76.1`'s own T-C9 added the third) |
| 6 | `uring.rs:287` |
| 7 | `envoy-http2/src/hcm.rs:518` |

Line numbers here are **this session's**, re-derived by grep — Tasks 3 and 4 shifted every
`hcm.rs` anchor below `synth_status`, so the plan's `:9734`/`:9761`/`:9778` no longer apply. This
is the same lesson the plan itself banked: **anchor on text, never on a number.**

### Step 1 — the flip, and N-3 fixed for free

`76.1` attached the T-C9 doc block — including the words **"76.2 MUST flip this test"** — to the
`redirect_placeholder_config` **helper** rather than to the test. That is banked finding **N-3**,
and it is confirmed on disk: the block sat at `hcm.rs:9815-9821`, immediately above
`async fn redirect_placeholder_config`, while the test began at `:9856`.

Fixed as the plan directs: the helper keeps a plain two-line doc describing what it builds, and
the rewritten doc block — explaining the flip and pinning the `%RESPONSE_CODE_DETAILS%`
observable — now sits on the **test**. The test is renamed
`build_response_redirect_is_not_implemented_placeholder` →
`build_response_redirect_emits_301_and_location`. **The rename is the point:** the placeholder's
replacement is a visible, named change rather than a silent behaviour shift.

### Step 2 — RUN RED, and a REFUTED plan prediction (deviation **D-4**)

```
$ cargo test -p envoy-http1 --lib -- build_response_redirect_emits_301_and_location
exit=101
test hcm::tests::build_response_redirect_emits_301_and_location ... FAILED
thread '...' panicked at crates/envoy-http1/src/hcm.rs:9872:17:
  left: 501
 right: 301
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 191 filtered out
```

`PLAN.md` Step 2 predicted *"The compile fails first on `&mut req` (the signature is still
`&Request`)"*. **Measured: it does not.** Rust coerces `&mut T` to `&T` at a call site, so the test
compiled against the OLD signature and failed straight on the assertion. The RED is therefore
*cleaner* than the plan expected — a pure behavioural `left: 501, right: 301`, the exact designed
flip, and the mirror image of the pre-flight's `left: 301, right: 501`. Recorded as **D-4**.

### Steps 3-4 — the signature widening and the arm

Two signatures widened to `&mut Request`; the 7 call sites updated (sites 3-5 also needed
`let req =` → `let mut req =`). The H2 site took `&mut envoy_req` at `:518` only, after the
`mem::take` write-back and the `matched_route` borrow of `config.inner` have ended — the
borrow-checker caveat the plan flagged **did not materialise**, exactly as its pre-flight said.

The arm itself was transcribed verbatim. Note it re-reads `Host` into an **owned `String`**
deliberately: that ends the immutable borrow of `req.headers` before the `req.path` write-back.

`synth_501` is **not** dead code after this replacement — re-verified by grep, it remains in use by
the chunked-`Transfer-Encoding` path at `hcm.rs:915`:

```
$ grep -n 'synth_501' crates/envoy-http1/src/hcm.rs
915:                    BuildOutcome::Synth(synth_501(close), None)
2501:pub(crate) fn synth_501(close: bool) -> Response {
```

### Step 5 — GREEN across the whole H1 + H2 regression surface

```
$ cargo test -p envoy-http1 --lib --no-fail-fast     # --no-fail-fast BEFORE the --
h1-exit=0
test result: ok. 192 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.47s

$ cargo test -p envoy-http2 --lib --no-fail-fast
h2-exit=0
test result: ok. 110 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.53s
```

Zero `^error` lines in either log. Output was redirected to files and read, never piped through
`tail` — `tail` truncates the `failures:` block and hides `Compiling`.

### Deviation **D-1 CLOSED** — the lint gate is green again

Tasks 2-4 each left a non-test item with no non-test consumer. Task 5 consumes all three, so the
full workspace gate now passes:

```
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
clippy-exit=0 Checking=7

$ cargo fmt --all -- --check
fmt-exit=0 bytes=0
```

Exit 0 with a **non-zero** `Checking` count — a clippy green with ZERO `Checking` lines would be a
fully-cached no-op rather than evidence. **D-1 is closed exactly where it was predicted to close.**

---

## Task 6 — the `prefix_rewrite` in-place `:path` mutation pins

**Status: COMPLETE.** Commit message: `phase 76.2 task 6: pin the prefix_rewrite :path mutation and the path_redirect non-mutation`.

**The observable.** MEASURED upstream with
`text_format: "PROBE path=%REQ(:PATH)% …"`: request `/e-pfx/sub` on a
`prefix_rewrite: "/replaced"` route is logged as `path=/replaced/sub`, while `/c-pathr/sub` on a
`path_redirect: "/newpath"` route is logged **unchanged**. That asymmetry is a real discriminating
observable and a parity trap — and it is **invisible to fixture `0086`**, which compares responses,
not logs. These two in-process pins are its only guard.

**This task adds no implementation.** Task 5 already landed the write-back, so both tests pass on
arrival:

```
$ cargo test -p envoy-http1 --lib -- build_response_prefix_rewrite build_response_path_redirect
exit=0
test hcm::tests::build_response_path_redirect_leaves_the_request_path_alone ... ok
test hcm::tests::build_response_prefix_rewrite_mutates_the_request_path ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 192 filtered out
```

A test that passes on arrival is **not** TDD evidence. Per `PLAN.md` Step 2 and the standing
project discipline for this situation, **the RED is produced by mutation instead**, and THAT is
recorded as the evidence.

### Step 2 — the RED, by mutation, under full hygiene

Clean work committed FIRST (`b641c27`), then a scratch `git worktree --detach` **at that commit** —
never the main tree, because four sibling `.claude/worktrees/agent-*` worktrees were live and a
parallel `git checkout` can silently revert an in-place mutation. The two write-back lines in
Task 5's arm were disabled:

```
MUTATED exit=101
   Compiling envoy-http1 v0.0.0 (<scratch>/crates/envoy-http1)
test hcm::tests::build_response_path_redirect_leaves_the_request_path_alone ... ok
test hcm::tests::build_response_prefix_rewrite_mutates_the_request_path ... FAILED
thread '...' panicked at crates/envoy-http1/src/hcm.rs:9885:9:
  left: "/e-pfx/sub"
 right: "/replaced/sub"
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 192 filtered out
```

**`left: "/e-pfx/sub"  right: "/replaced/sub"`** — the exact RED text `PLAN.md` predicted.

Two things make this stronger than a bare RED:

1. **The mutation is CELL-ACCURATE.** T6-2 (`path_redirect` non-mutation) stayed **GREEN** under
   the same mutation, which is what it must do — disabling the write-back cannot affect the arm
   that never writes back. A mutation that reddened both tests would have indicated the pins were
   measuring the same thing twice.
2. **`Compiling envoy-http1` appears on the mutated run**, proving no stale binary. A mutation
   check against a cached test binary is a FALSE PASS.

Post-run integrity re-grep, then the **unmutated control from the SAME worktree** (a RED that never
reached an assertion is not evidence, and a control from a different tree proves nothing):

```
mutation still present after the run? -> 1 (expect 1)
reverted; mutation now -> 0 (expect 0)

CONTROL exit=0
   Compiling envoy-http1 v0.0.0 (<scratch>/crates/envoy-http1)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 192 filtered out
```

Worktree removed; `git worktree list` re-checked from the repo root shows the main tree plus the
**four pre-existing sibling `agent-*` worktrees, untouched**. Main tree `git status --porcelain`
clean.

### Gates

```
$ cargo fmt --all -- --check
fmt-exit=0 bytes=0

$ cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings
clippy-exit=0 Checking=1
```

**Transcription note.** The plan's literals for both tests use
`let mut rd = RedirectAction::default(); rd.<field> = …`. Transcribed with struct-update syntax
(`RedirectAction { <field>: …, ..Default::default() }`) instead, which is the same construction
without the `clippy::field_reassign_with_default` exposure — a cosmetic transcription choice, not
a behavioural deviation, and `cargo fmt --check` is byte-clean either way.

---

## Task 7 — the HTTP/2 shared-seam in-process test

**Status: COMPLETE.** Commit message: `phase 76.2 task 7: pin that the shared dispatch seam serves HTTP/2`.

**The claim being pinned.** HTTP/2 has **no route-action dispatch of its own** — it calls H1's
resolver and H1's `build_response` (`crates/envoy-http2/src/hcm.rs:18` imports them, `:518` calls
`build_response`). So the ONE arm Task 5 added serves **both codecs**, and a bug there hits both.

**This is the one task `PLAN.md` flagged as NOT pre-flighted end-to-end** (it depended on a helper
that did not yet exist), with an explicit instruction to read the existing
`h2_resolve_route_reachable_and_returns_cors_route` first and copy its shape. Done — and that
reading is what caught deviation **D-5**.

### D-5 — the plan's literal names a variant that does not exist

`PLAN.md` T7-1 builds its request with `version: envoy_http1::codec::HttpVersion::Http2`.
MEASURED: `HttpVersion` (`crates/envoy-http1/src/codec.rs:14-17`) has **exactly two** variants:

```rust
pub enum HttpVersion {
    Http10,
    Http11,
}
```

There is **no `Http2`**. The sibling H2 test builds its `envoy_http1::Request` with `Http11` too —
the version field plays no part in route dispatch — so `Http11` is used here, with an in-code
comment saying why. The re-export path is also `envoy_http1::HttpVersion` (`lib.rs:28`), not
`envoy_http1::codec::…`. The plan's trailing `let _ = (…)` scaffolding line was deleted as
instructed, its imports being genuinely consumed by the helper.

The helper `h2_redirect_h1_config` was modelled on the CORS test's `Http1HCMConfig::from_config(
&cfg, cluster_mgr, registry, None)` shape rather than invented.

### GREEN

```
$ cargo test -p envoy-http2 --lib -- h2_shared_seam
exit=0
test hcm::tests::h2_shared_seam_serves_the_redirect_arm ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 111 filtered out

$ cargo fmt --all -- --check
fmt-exit=0 bytes=0
$ cargo clippy -p envoy-http2 --all-targets --all-features -- -D warnings
clippy-exit=0 Checking=9
```

### The RED, by mutation — and it is the STRONGEST evidence in this phase

Task 5 was already committed, so the test passes on arrival. `PLAN.md` Task 7 Step 2 anticipates
exactly this and directs that TDD's RED be honoured by **Task 3's `host_part = authority`
mutation** in a scratch worktree. Run at `cbfcaf1`:

```
MUTATED exit=101
   Compiling envoy-http1 v0.0.0 (<scratch>/crates/envoy-http1)
   Compiling envoy-http2 v0.0.0 (<scratch>/crates/envoy-http2)
test hcm::tests::h2_shared_seam_serves_the_redirect_arm ... FAILED
thread '...' panicked at crates/envoy-http2/src/hcm.rs:6932:17:
  left: Some("http://envoy-rust.test/a-host")
 right: Some("http://example.com/a-host")
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 111 filtered out
```

**Read what this actually proves.** The mutation was applied to `crates/envoy-http1/src/hcm.rs`,
and the test that went RED lives in `crates/envoy-http2/src/hcm.rs`. A one-line change to H1's
`plan_redirect` reddens an H2 test. That is a **direct demonstration** of the shared seam — not an
assertion that H2 reaches the arm, but proof that H2's answer is *computed by* the H1 arm. Both
crates appear in the `Compiling` list, so neither used a stale binary.

Control and integrity, from the same worktree:

```
mutation still present after run? -> 1 (expect 1)
CONTROL exit=0
   Compiling envoy-http1 v0.0.0 (<scratch>/crates/envoy-http1)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 111 filtered out
```

**Housekeeping note.** `git worktree remove` was run while the shell's cwd was still *inside* the
worktree, so the next command died with `fatal: Unable to read current working directory`. That is
the known benign symptom, not a failed removal — re-verified from the repo root that
`git worktree list` shows exactly **5** entries: the main tree plus the **four pre-existing sibling
`agent-*` worktrees**, all untouched. Main tree clean.

### What this test does NOT prove

It pins envoy-rust's **own** seam, not upstream parity. Upstream's H2 `:scheme`/`:authority`
handling was never probed (`SPEC.md` §8 item 2), and an H2 differential fixture is an explicit
non-goal (`SPEC.md` §7 item 4) — the disposition phases 68 and 69 took.

---

## Task 8 — CF-76-2: the RDS warm path re-validates the redirect oneofs

**Status: COMPLETE. CF-76-2 is CLOSED by this task.** Commit message:
`phase 76.2 task 8: close CF-76-2 — RDS warm path re-validates the redirect oneofs [CF-76-2]`.

### The carry-forward, RE-CONFIRMED verbatim on disk

`crates/envoy-config/src/rds.rs:135` re-validated a hot-reloaded route table with an **`if let`,
not an exhaustive `match`**:

```rust
            if let crate::RouteAction::Route(ar) = &route.action
                && !known_cluster(&ar.cluster)
            {
```

so `76.1` adding the `Redirect` variant tripped **no compile error here**. `76.1` joined an
ADR-0028-sanctioned hole rather than creating one, and its blast radius was **NIL** because the
runtime arm was the inert 501 either way. **Task 5 removed that inertness** — those routes now
serve a real 3xx built from fields never checked for mutual exclusivity — so the condition that
made it tolerable is gone.

### Step 2 — RUN RED: this IS CF-76-2, reproduced

```
$ cargo test -p envoy-config --lib -- rds_reload_rejects_a_conflicting_redirect_oneof
exit=101
test rds::tests::rds_reload_rejects_a_conflicting_redirect_oneof ... FAILED
thread '...' panicked at crates/envoy-config/src/rds.rs:397:14:
a conflicting redirect oneof must be warm-rejected: RouteConfiguration { name: "local_route",
  virtual_hosts: [VirtualHost { ... routes: [Route { ... action: Redirect(RedirectAction {
  host_redirect: None, port_redirect: None, path_redirect: Some("/p"), prefix_rewrite: Some("/q"),
  https_redirect: None, scheme_redirect: None, strip_query: false,
  response_code: MovedPermanently }), ... }] }] }
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 681 filtered out
```

**Read the dump, not just the FAILED.** `reparse_and_select_route_config` returned **`Ok(...)`**
carrying `path_redirect: Some("/p")` **and** `prefix_rewrite: Some("/q")` simultaneously — a
mutually-exclusive pair, accepted warm and ready to install LIVE, while the byte-identical config
at boot is boot-fatal. That is CF-76-2 in one line of output, not a proxy for it.

### Steps 3-4 — the fix, minimal and precise

1. **Lifted** `76.1`'s two inline checks (`bootstrap.rs:4076`/`:4082`) into
   `pub(crate) fn validate_redirect_oneofs(rd, context, route)`, so boot and warm paths are the
   same code **by construction rather than by discipline**. The boot arm collapses to a single
   `validate_redirect_oneofs(rd, listener_name, &r.name)?;`.
2. **Converted** `rds.rs`'s `if let` to an **exhaustive `match`**, restoring the compile-time
   forcing function: a future fourth `RouteAction` variant must now fail to build until handled
   here.
3. **Left `DirectResponse` an explicit `=> {}` arm naming ADR-0028**, so the pre-existing
   sanctioned deferral is *documented* rather than silently re-joined.

The function's doc block (step 4 of the documented walk) was updated to describe all three arms.

**Doc-comment hazard checked, not assumed.** The new function is inserted immediately above a
`#[derive]` — the exact move that caused `76.1`'s M-1. MEASURED first: `bootstrap.rs:2657` is a
**bare `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]` for `RouteMatch` with no doc
comment above it** (`:2656` blank, `:2655` a closing brace), so inserting there orphans nothing.

**Scope boundary held. ADR-0028 is NOT lifted.** No `validate_hcm`, no `InvalidStatusCode`, no
`validate_data_source` was added to the RDS path. Exactly the hole `76.1` opened is closed, in the
sub-phase where that hole goes live.

**On the `ConfigError` field name.** Both variants carry `{ listener, route }`; on the RDS path
there is no listener, so `context` is passed as `rds:<path>`, giving a mildly loose message
(``redirect action on listener `rds:/tmp/…/rds.yaml` route ``). Accepted deliberately rather than
minting a 126th `ConfigError` variant for a context string — **error TEXT is not part of the
equivalence contract**, only the verdict is, so this costs nothing differentially.

### Step 5 — GREEN

```
$ cargo test -p envoy-config --lib --no-fail-fast -- rds::
exit=0
test rds::tests::rds_reload_accepts_a_valid_redirect_route ... ok
test rds::tests::rds_reload_rejects_a_conflicting_redirect_oneof ... ok
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 668 filtered out
```

**`14 passed`** — exactly the plan's prediction: the **12** pre-existing RDS tests (so the rewrite
breaks none) plus the **2** new ones. Both directions are pinned: T8-1 rejects the conflicting
pair, and T8-2 proves a *valid* redirect route still reloads warm — without T8-2, T8-1 would pass
just as well if the path had started rejecting every redirect.

```
$ cargo test -p envoy-config --lib --no-fail-fast
test result: ok. 682 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo fmt --all -- --check
fmt-exit=0 bytes=0
$ cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
clippy-exit=0 Checking=1
```

**Test-module helper note.** `PLAN.md` transcribes `tempfile::tempdir()` inline but instructs the
executor to prefer an existing helper. The module has one — `write_temp(contents) -> (TempDir,
PathBuf)` at `rds.rs:280` — so both tests use it. No new dev-dependency (`tempfile = "3"` was
already there).

### The generalised lesson, carried forward

`76.1/SPEC.md` §2.3's claim that *"the compiler enforces the seam"* is **weaker than it sounds**.
It holds only at genuine exhaustive `match` sites. It did **not** hold at `rds.rs:135`'s `if let`
(which is exactly how CF-76-2 happened) and it still does **not** hold at the `Route` visitor's own
`_` catch-all (`bootstrap.rs:2591`), where a future fourth `RouteAction` variant would silently
fall into *"more than one is present"* rather than failing to build. **Add any future
`RouteAction` variant by AUDITING EVERY SITE BY GREP, never by trusting the build.** Task 9 records
this in the code itself.

---

## Task 9 — M-1 + M-2: restore and correct `RouteAction`'s doc comment

**Status: COMPLETE. Banked findings M-1 and M-2 are both CLOSED.** Commit message:
`phase 76.2 task 9: restore and correct RouteAction's doc comment [M-1, M-2]`.

**Documentation only** — no behaviour change, no test change, so there is no TDD RED step here and
`PLAN.md` specifies none. The verification is mechanical (below), because **nothing in §7.5 gate
(e) reads prose**: `envoy-config` enables no `missing_docs` lint and `cargo fmt` does not reflow
doc comments.

### The defect, RE-CONFIRMED on disk before touching anything

`76.1` inserted `RedirectResponseCode` **between** `RouteAction`'s 04.3 doc block and
`RouteAction`'s `#[derive]`. Measured at `0d08c48`, `bootstrap.rs:2170-2182` was a **single
contiguous doc block** — the 04.3 text (`:2170-2176`) glued directly onto `76.1`'s
`RedirectResponseCode` text (`:2177-2182`) with no blank line — all of it attaching to
`pub enum RedirectResponseCode` (`:2185`). And `:2245` was a bare
`#[derive(Debug, Clone, PartialEq)]` with `pub enum RouteAction` (`:2246`) **undocumented**.

**M-2** confirmed in the same read: the orphaned text was also **stale**. It said the route's peer
keys are ``direct_response: { ... }` OR `route: { ... }`` — a **two**-way oneof — and that
"both-present and neither-present are errors", which `76.1` had widened to three. **M-1 and M-2
had to be fixed together**: a verbatim restore would have re-attached stale text.

### The fix

1. Deleted the seven orphaned lines, leaving `RedirectResponseCode` with its own correct
   `76.1 (§4.1): …` doc block.
2. Inserted the corrected block above `#[derive] / pub enum RouteAction`, naming all **three**
   peer keys and correcting the cardinality wording to "neither-present and more-than-one-present
   are both errors".
3. Added the paragraph `PLAN.md` specifies, recording **in the code itself** the generalised lesson
   CF-76-2 came from: adding a fourth variant does **not** fail the build everywhere it must,
   because the `Route` visitor's cardinality check ends in a `_ =>` catch-all and the RDS
   re-validation historically used an `if let`. **Audit every site by grep, never by trusting the
   compiler.**

### Step 3 — verified MECHANICALLY, not by eye

```
$ grep -n -B3 '^pub enum RouteAction {' crates/envoy-config/src/bootstrap.rs
2250-/// variant can slip through silently. Audit every site BY GREP, never by
2251-/// trusting the compiler.
2252-#[derive(Debug, Clone, PartialEq)]
2253:pub enum RouteAction {

new text  (expect 1): 1
old text  (expect 0): 0
```

The block now sits directly above `pub enum RouteAction`, appears exactly once, and the stale
wording is gone from the file entirely. Cross-checked from the other side that
`RedirectResponseCode` retains its own, correct doc block starting at the `76.1 (§4.1):` line.

```
$ cargo build -p envoy-config --all-targets
build-exit=0
$ cargo fmt --all -- --check
fmt-exit=0 bytes=0
$ cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
clippy-exit=0 Checking=1
```

### Why this was in scope at all

`76.1`'s review graded the orphaning **Minor** and **put on the record that two of three reviewers
argued Issue**. It is scheduled here — and only here — because `76.2` edits this exact region
(Task 8 inserts `validate_redirect_oneofs` a few hundred lines below), which makes it the cheapest
possible place to fix. The other banked findings (M-3, M-4, M-6, N-1, N-2, N-4…N-9) stay **banked
and unfixed**: they are polish on `76.1`'s config surface with no `76.2` witness, and fixing them
would widen scope against §6.3. **N-10/N-11 are defects in the landed `76.1/PROGRESS.md` and are
NOT EDITABLE by any session** (D-3.5).

---

## Task 10 — differential fixture `0086-route-redirect-action`

**Status: COMPLETE.** Commit message:
`phase 76.2 task 10: differential fixture 0086 — 18 probes over the redirect location rules`.

### Census RE-DERIVED, not inherited

```
fixture dirs:      85
differential .rs:  85
0086 free? -> 0    (expect 0)
highest:           0085-headermatcher-absence-accesslog-present-polarity
[[test]] sections: 0
```

`0086` is the next free id. Derived with `git ls-files`, deliberately — a bare `find` would also
walk the four live sibling `.claude/worktrees/agent-*` worktrees and inflate every count.

### The four files

`envoy.yaml` (18 `prefix:`-matched redirect routes, `clusters: []`, trailing `admin:` on
`port_value: 0`), `envoy-rust.yaml`, `expectations.yaml` (18 probes), `README.md`.

**`envoy-rust.yaml` was DERIVED MECHANICALLY from `envoy.yaml`, not hand-copied** — a script
applies exactly the three permitted hunks and asserts each one matched, so the two configs cannot
silently drift apart. The resulting diff is exactly:

```
0a1,3   > node: / id: x / cluster: y
5c8     < address: 0.0.0.0 …  →  > address: 127.0.0.1 …
67,69d69  < admin: / address: / socket_address: { address: 0.0.0.0, port_value: 0 }
```

The YAML 1.1 trap is left alone as instructed: an unquoted `cluster: y` parses as boolean `true`,
every existing fixture writes it exactly that way, and it is fine there — **not "improved".**

### The four binding authoring constraints — verified MECHANICALLY, not by eye

Each of these can silently vacate a probe, so none was eyeballed:

```
routes=18  probes=18  names=18
PREFIX-SHADOWING pairs: NONE  <-- required
distinct probe paths:   True (18/18)
distinct probe names:   True
distinct routes selected: True (18/18)
routes with no probe:   NONE
{{ADMIN_PORT}} present: False <-- must be False
{{PORT}} count in envoy.yaml: 1
expected_headers is a BARE SCALAR everywhere: True
```

- **No prefix is a prefix of another.** Prefix overlap *silently shadows* a probe — a parent-recon
  cell was lost exactly this way when `/scheme` preceded `/schemehost`. Zero shadowing pairs, and
  the check simulates the match to confirm **each probe selects a DIFFERENT route** (18 distinct
  routes for 18 probes, none left unprobed).
- **This is why `q01`/`q03` get their OWN routes** (`/q1-hostport`, `/q3-hostport`) instead of
  re-probing `/f-https` and `/j-bare` with a different `Host:` — that would violate the
  distinct-`path:` rule.
- **Every route is `prefix:`-matched**, keeping the fixture clean of the open **CF-76-1** (upstream
  strips the query before route matching; envoy-rust matches the raw target). A live design
  constraint, not a footnote — `r02`, `r04`, `r08` and `r13` all carry queries.
- **`{{PORT}}` is the only token substituted** — `{{ADMIN_PORT}}` absent, confirmed.
- **`expected_headers` is a BARE SCALAR** in all 18 probes, not a map.

### Step 5 — validation, and a REFUTED plan command (deviation **D-6**)

`PLAN.md` says to run `./target/debug/envoy-bin --mode validate -c …`. MEASURED:

```
envoy-bin: unknown argument: --mode
validate-exit=2
```

`crates/envoy-bin/src/argv.rs:27-29` is explicit: *"Phase 01 accepts exactly one flag: `-c <path>`
or `--config-path <path>`"*. There is no `--mode`. Validated the way that actually works —
substitute `{{PORT}}` into a scratch copy and boot it under `timeout`:

```
$ cargo build -p envoy-bin          # MANDATORY; 11 Compiling lines, exit 0
$ timeout 6 ./target/debug/envoy-bin -c <scratch>/0086-validate-r1.yaml
exit=124  (124 = still running at timeout => config ACCEPTED and listener bound)
INFO node registered node.id=x node.cluster=y
INFO listener bound with SO_REUSEPORT … addr=127.0.0.1:18086 sockets=32
INFO envoy-rust listening (http_connection_manager) … stat_prefix=ingress_http codec_type=HTTP1
INFO envoy-rust exited cleanly
--- ConfigError present? (expect 0) --- 0
```

All 18 redirect routes parsed and the listener bound. This is **strictly stronger** than the
plan's intended check.

### Bonus: a local smoke probe of six representative cells, on the real wire

Backend-free means fully local, so the fixture's cells were driven directly before spending a
Docker run — this isolates an envoy-rust-side bug from harness noise:

```
r01 /a-host              HTTP/1.1 301 Moved Permanently  | location: http://example.com/a-host                 | content-type: absent(ok)
r05 /e-pfx/sub           HTTP/1.1 301 Moved Permanently  | location: http://envoy-rust.test/replaced/sub       | content-type: absent(ok)
r13 /m-see/y?q=1         HTTP/1.1 303 See Other          | location: http://e.com/m-see/y                      | content-type: absent(ok)
r16 /p-perm              HTTP/1.1 308 Permanent Redirect | location: http://example.com/p-perm                 | content-type: absent(ok)
q01 /q1-hostport/x :1234 HTTP/1.1 301 Moved Permanently  | location: https://envoy-rust.test:1234/q1-hostport/x| content-type: absent(ok)
q03 /q3-hostport/d :1234 HTTP/1.1 301 Moved Permanently  | location: http://envoy-rust.test:1234/q3-hostport/d | content-type: absent(ok)
```

Three things worth naming:

1. **`303 See Other` and `308 Permanent Redirect` appear on the WIRE** — before Task 1 these read
   `303 OK` / `308 OK`. This is the silent-wrong-answer hazard closed, observed end-to-end. Note
   the differential fixture still cannot see it (the harness parses the status **code** only) —
   which is exactly why Task 1's in-process pin exists.
2. **`q01`/`q03` keep `:1234`** while `r01` drops the port — the authority asymmetry, live.
3. **No `content-type` on any redirect**, confirming Task 4's dedicated builder is what the wire
   actually gets.

---

## Task 12 — `BEHAVIOR_CONTRACT.md` Phase 76 section

**Status: COMPLETE.** Commit message:
`phase 76.2 task 12: BEHAVIOR_CONTRACT Phase 76 — the redirect location + header-set rules`.

Documentation, but a **§7.5 obligation**: the contract is the canonical definition of equivalence
(doctrine D-3.3), and MEASURED behaviour that lives only in a phase SPEC is not durable — a phase
SPEC is a landed historical artifact nobody re-reads, whereas `BEHAVIOR_CONTRACT.md` is a
cold-start read.

**MEASURED absent before this task:** `grep -c 'Phase 76' docs/envoy-rust/BEHAVIOR_CONTRACT.md`
→ **0**.

### Placement

Inserted after the Phase 75 section (which ends at the ADR-0158 one-sink paragraph) and **before**
`## xDS wire state machine`, exactly as `PLAN.md` specifies. Six subsections, modelled on Phase 75's
structure:

- **§A** — the `location` construction rules (a)-(e) with the full R1-R16 / Q1-Q4 / E1-E2 tables.
  The **authority asymmetry** is called out as the headline rule.
- **§B** — the redirect response header set, and the explicit contrast with `direct_response`:
  **a redirect carries NO `content-type`.**
- **§C** — all five status lines, plus the statement that the reason phrase is **not** part of the
  equivalence matrix, which is *why* 303/307/308 are pinned in-process rather than differentially.
- **§D** — the access-log observables: `%RESPONSE_CODE_DETAILS%` = `direct_response` (so no new
  detail string/`Op`/field exists), `%RESPONSE_FLAGS%` = `-`, and the `prefix_rewrite`-mutates /
  `path_redirect`-does-not asymmetry.
- **§E** — the harness rule as a **standing prohibition**: `location` is not allow-listed, is
  compared value-exact, and **must never be added to the allow-list** — with the reason named
  (doing so vacates the witness *while leaving the fixture green*, which looks like success).
- **§F** — the NOT-MEASURED list, **eight items**.

### §F items 7 and 8 — the two cells this phase CREATED rather than measured

`PLAN.md` §6 finding (6) is explicit that the implementation introduced two behaviours that are
**unwitnessed by construction**, and requires both to be banked so a later session does not mistake
them for settled:

7. Whether `prefix_rewrite` on a **`path:`-matched** route replaces the whole path. envoy-rust
   implements *"the matched span is the whole path when `match.prefix` is `None`"* — a **choice**,
   not a measurement. Every route in `0086` is `prefix:`-matched, so nothing exercises it.
8. Whether the query rides along on the **rewritten `:path`** when `prefix_rewrite` and a query
   combine. envoy-rust preserves it. `0086`'s `r05` probe is **deliberately query-free**, so
   nothing exercises it.

Both are recorded with an explicit closing note that they are envoy-rust's current choice, pinned
only by in-process tests, and **never compared against upstream**.

### Verification — structural, and delta-based

```
$ grep -c '^### Phase 76' docs/envoy-rust/BEHAVIOR_CONTRACT.md
1
$ grep -n '^### Phase 76\|^## xDS wire state machine' docs/envoy-rust/BEHAVIOR_CONTRACT.md
2957:### Phase 76 (ADR-0168/0169): …
3128:## xDS wire state machine

$ git diff --numstat docs/envoy-rust/BEHAVIOR_CONTRACT.md
169     0       docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

The heading appears **exactly once**, sits **before** the xDS heading, and the numstat shows
**169 added / 0 deleted** — no pre-existing line was touched, so no content was lost and Phase 75
is intact (`grep -c '^### Phase 75'` → 1).

**Duplication check run on the DELTA, not the whole file.** `PLAN.md` suggests
`sort <file> | uniq -d`, but over a 3700-line document that returns dozens of legitimate repeats
(markdown table separators, `>` blockquote markers) and would drown a real signal. Checking only
the added lines:

```
$ git diff -U0 … | grep '^+' | sed 's/^+//' | grep -v '^$' | sort | uniq -d
|---|---|
|---|---|---|---|---|
```

Both are markdown table separator rows, legitimately repeated across the section's five tables.
No duplicated prose.

**One defect caught and fixed by that structural check:** the first insertion left a **doubled
`---` separator** (the section's own separator plus the pre-existing one). Detected with an
explicit scan for a `---` / blank / `---` run, and removed; the file now has a single separator
between the Phase 76 section and `## xDS wire state machine`. This is exactly why the structural
check is run rather than eyeballing the diff.

---

## Task 11 (continued) — Step 2 the fixture run, its AUDIT, and Step 3 the full suite

### Step 2 — `0086` green, and WHY that green is trusted

```
$ cargo build -p envoy-bin          # MANDATORY before any local differential
$ cargo test -p differential --test route_redirect_action -- --nocapture
exit=0
   Compiling differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
```

Docker daemon confirmed **up** and the pinned `envoyproxy/envoy:v1.33.0` image confirmed present
before the run.

**A 1.00 s green on a backend-free fixture is NORMAL — but it is also exactly what a silent skip
looks like, so it was AUDITED with a deliberate NEGATIVE CONTROL** rather than trusted. In a
scratch worktree at `81aee77`, `r13`'s `expected_status` was falsified from `303` to `302`:

```
NEGATIVE-CONTROL exit=101
thread 'route_redirect_action_fixture' panicked at tests/differential/tests/route_redirect_action.rs:37:10:
fixture passes: probe r13-response-code-303-with-strip-query: upstream status 303 != expected 302
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.98s
```

**Read the words `upstream status 303`.** They can only have come from the upstream Envoy container
actually running and answering. The fixture is genuinely driving both proxies and comparing — the
green is real. The control was re-grepped as still present afterwards, the worktree removed, and
the main fixture re-verified unmutated (`r13` still `expected_status: 303`).

### Step 3 — the full `-p differential` suite: PARTIAL, and honestly labelled

```
$ cargo test -p differential --no-fail-fast          # redirected to a file, never through `tail`
binaries=88 passed=241 failed=6
```

Census taken with `grep -oE 'test result: (ok|FAILED)\. …'` and **awk fields `$4`/`$6`** — the
`ok`-only form would discard `FAILED` lines and make `failed=0` true *by construction*, and
`$5`/`$7` would return a vacuous `passed=0`.

**`route_redirect_action_fixture ... ok` IN THE FULL PARALLEL SWEEP** (log line 995), not merely in
isolation — which also clears the known "passes in isolation, fails under parallel load"
differential flake family for this fixture.

The **6** failures, censused from the `---- <name> stdout ----` markers (never by indentation,
which invents phantom test names):

| failing binary | family |
|---|---|
| `access_log_h2_rcd_upstream_reset` | the 4-member `TcpCloseBackend` IPv6-unreachable **deterministic core** |
| `access_log_h2_uc_upstream_reset` | ″ |
| `access_log_rcd_upstream_reset` | ″ |
| `access_log_rf_upstream_reset` | ″ |
| `admin_config_dump_server_info` | the `192.168.65.2` bridge-IP core member |
| `access_log_upstream_host` | backend-routing family — RED locally on this host, CI-authoritative |

That is **exactly** the documented 5-member stable core plus one backend-routing fixture. **None is
on `76.2`'s surface, and no redirect-related test failed.**

> **THIS RUN WAS CUT SHORT — the numbers above are PARTIAL, not a gate adjudication.** The sweep
> **stalled on `xds_rds_hot_reload`** with no output for ~11 minutes (this host has virtiofs and no
> inotify, so bind-mount watch/reload tests are native-CI-authoritative) and was killed by PID, so
> a handful of trailing `xds_*` binaries never reported. **§7.5 gate (b) is state 4's to
> adjudicate, on its own 2-3× sweep with a diffed failing SET** — this Task 11 evidence is
> supplementary.

**A trap worth carrying forward, hit concretely here.** The background waiter
`until ! pgrep -f 'cargo test -p differential'` reported `STILL UP` indefinitely *after* cargo had
exited, because **its own command line contains the pattern it greps for** — the documented
`pkill -f` self-match hazard, in its `pgrep` form. Adjudicate "is it still running?" with
`pgrep -x cargo`, never with `pgrep -f <a pattern your own shell contains>`.

---

## Session close — state 3 COMPLETE, handing off to state 4

**All twelve tasks landed, one commit each**, in `PLAN.md` order:

| task | commit | what |
|---|---|---|
| 1 | `6c0fcd2` | `canonical_reason` 303/307/308 + `headers::LOCATION` |
| 2 | `c7d3735` | widen `hcm.rs`'s non-test `envoy_config` import |
| 3 | `721e6da` | the pure `plan_redirect` + `RedirectPlan`; all 22 measured cells |
| 4 | `9e8a225` | `synth_redirect` — five headers, no `content-type` |
| 5 | `78aba4c` | the real dispatch arm + `&mut Request`; T-C9 flipped; N-3 closed |
| 6 | `015d9e1` | the `prefix_rewrite` `:path` mutation pins |
| 7 | `a42581d` | the HTTP/2 shared-seam test |
| 8 | `0d08c48` | **CF-76-2 CLOSED** |
| 9 | `5930158` | **M-1 + M-2 CLOSED** |
| 10 | `b9afd81` | fixture `0086` (18 probes) |
| 11 | `81aee77` | the fixture entrypoint |
| 12 | `f351c3e` | `BEHAVIOR_CONTRACT.md` Phase 76 |

**Size.** Net non-`docs/` change **+1334 / −69 = 1265** LoC against `PLAN.md`'s ≈1202 code-only
projection — **+5%**, versus `76.1`'s **+50%** overshoot. The table-driven 22-cell design was
preserved (not expanded into 22 `#[test]` fns, which `PLAN.md` §4 warned would eat the entire
headroom), **§6.1's mid-execution split trigger never fired, and no split was needed.**

**What was NOT done, deliberately.** No verification (state 4 is a separate session; ADR-0127 — a
verifier must not grade what it ran). No ROADMAP status cell flipped (a state-3 commit flips none).
No ADR (head **ADR-0170**, next free **ADR-0171**, re-derived on disk). No edit to `SPEC.md`,
`PLAN.md`, any `76.1` artifact, `known-failures.txt`, `ci.yml`, or `HEADER_ALLOW_LIST`. No banked
carry-forward fixed beyond the three `PLAN.md` names. **No `stop` file** — the stop condition was
re-measured and all three legs came back FALSE.

**The single most important thing for the state-4 session to read first:** the **Deviations
D-1..D-6** table at the top of this file. Three of the six are defects in `PLAN.md`'s own
*pre-flighted* literal Rust, so a reviewer diffing the plan's text against the landed code will
find intentional differences, each with its measurement.

---

# §5 state-4 — the §7.5 phase-done gate, RUN AND ADJUDICATED

> **What this section is.** The §5 **state-4** VERIFICATION of sub-phase `76.2-redirect-runtime-fixture`,
> run by a session separate from the state-3 implementation (§5.1; ADR-0127 — the context that wrote
> an artifact must not grade it). Every command below was **actually run in this session** and its
> **real output is quoted**. Written for a reader with zero prior context (D-3.4).
>
> **This session does NOT review.** State 5 is a separate session, and `REVIEW.md` does not exist yet,
> so gate (f) is legitimately **OPEN** — see the adjudication table.
>
> **Cold start (disk-authoritative).** `git status --porcelain` clean; branch `main`; `HEAD` at
> `a2ebc8a2a791012504ef13140d9c90a826388b6c`, equal to `origin/main` after `git fetch origin --prune`.
> Directory holds `SPEC.md` + `PLAN.md` + `PROGRESS.md` and **no** `REVIEW.md` — §5 state 4's
> unambiguous detection rule. ROADMAP row `76.2` `planned`, `76.1` `done`, parent `76` `in-progress`
> (re-measured by splitting each row on `' | '` and reading **field 4**, not field 5).

## S4.0 — censuses RE-DERIVED on disk, never inherited

Every figure this session used was re-measured. `git ls-files` was used throughout so the four live
sibling `.claude/worktrees/agent-*` worktrees cannot inflate a count.

| census | inherited | **MEASURED here** | verdict |
|---|---|---|---|
| ROADMAP rows / `done` / `in-progress` / `planned` | 107 / 105 / 1 / 1 | **107 / 105 / 1 / 1** | reproduces |
| fixture dirs (`tests/fixtures/`) | 86 | **86** | reproduces |
| differential test files | 86 | **86** | reproduces |
| `HEADER_ALLOW_LIST` entries | 3 | **3**, at `tests/differential/src/lib.rs:1177-1181` | reproduces (line range refined from the inherited `:1173-1181`) |
| `location` in `HEADER_ALLOW_LIST` | must be 0 | **0** | the `0086` witness is intact |
| `known-failures.txt` | 21 lines | **21**, byte-unchanged by the phase | reproduces |
| fuzz targets | 5 across 5 crates | **5** across 5 crates | reproduces |
| ADR ledger head | ADR-0170 | **ADR-0170**, next free **ADR-0171** | reproduces |
| `synth_501` still live | not dead code | **defined `hcm.rs:2501`, consumed `hcm.rs:915`** | correctly NOT deleted |

## S4.1 — gate (e): build, clippy, fmt, deny

Run in CI's order. **Each was gated on its `Compiling`/`Checking` LINE COUNT, not on its exit code
or duration** — a clippy exit 0 with zero `Checking` lines is a fully-cached no-op, not evidence.

```
$ cargo build --workspace --all-targets
exit=0     # 14 `Compiling` lines — real work, not a cached no-op
   Compiling envoy-config v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-config)
   Compiling differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
   Compiling envoy-listener v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-listener)
   ... (14 total)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.17s
```

```
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
exit=0     # 14 `Checking` lines
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.69s
```

**Zero warnings, zero errors.** This also CLOSES deviation **D-1** from the top of this file: the
plan's "clippy clean at every task boundary" was unachievable under its own task split and
self-closed at Task 5 — confirmed here at the phase level.

```
$ cargo fmt --all -- --check
exit=0     # ZERO bytes of output
```

```
$ cargo deny check
exit=0
advisories ok, bans ok, licenses ok, sources ok
```

`cargo deny` emitted **5** `warning[license-not-encountered]` lines (`0BSD`, `BSD-2-Clause`,
`MPL-2.0`, `Unicode-DFS-2016`, `Zlib`). These are **unmatched ALLOWANCES in `deny.toml`, not
violations** — the four verdict words above are all `ok`.

**ADR-0150 seam re-verified as a side-effect.** `envoy-accesslog`'s `[dependencies]` are exactly
`tokio`, `bytes`, `tracing`, `thiserror` — `grep -c envoy-config crates/envoy-accesslog/Cargo.toml`
returns **0** — and `envoy-accesslog` is **ABSENT** from the clippy `Checking` list even though this
phase changed `envoy-config`. Touching `envoy-config` did not force an `envoy-accesslog` re-check:
**the seam holds.**

**D-3.8 re-verified** on all three crates this phase touched: `crates/envoy-http1/src/lib.rs`,
`crates/envoy-http2/src/lib.rs` and `crates/envoy-config/src/lib.rs` each begin
`#![forbid(unsafe_code)]`.

## S4.2 — gate (b): THREE full workspace sweeps, with the failing SET diffed

`--no-fail-fast` placed **before** the `--` (it is a `cargo test` flag, not a harness flag), full
output redirected to a file, **never piped through `tail`**. Three sweeps, because ADR-0164 leg (iii)
cannot be satisfied by a single run.

Censused with `grep -oE 'test result: (ok|FAILED)\. …'` and **awk fields `$4`/`$6`**. The `ok`-only
form would discard `FAILED` lines and make `failed=0` true *by construction*; `$5`/`$7` returns a
vacuous `passed=0` — **verified here**: the same log under `$5`/`$7` yields
`binaries=163 passed=0 failed=0`.

| sweep | wall clock | binaries | passed | failed | **sum** |
|---|---|---|---|---|---|
| 1 | 5m18s | **163** | 2143 | 5 | **2148** |
| 2 | 5m21s | **163** | 2142 | 6 | **2148** |
| 3 | 5m34s | **163** | 2141 | 7 | **2148** |

**The sum is invariant at 2148 across all three sweeps.** The binary count is invariant at 163.

**A NOTE ON THE `xds_rds_hot_reload` STALL.** The state-3 session recorded that a full local
`-p differential` sweep **stalls on `xds_rds_hot_reload`** on this host and killed it after ~11 min.
**That did not reproduce here.** All three whole-workspace sweeps ran to completion in ~5½ minutes
each and reported all 163 binaries. The stall was an environmental artifact of that session, not a
property of the tree — recorded so a later session does not budget for a stall that is not there.

### The failing SET, diffed across the three sweeps

```
INTERSECTION (present in ALL THREE)      UNION (present in ANY)
access_log_h2_rcd_upstream_reset         access_log_command_operators
access_log_h2_uc_upstream_reset          access_log_h2_rcd_upstream_reset
access_log_rcd_upstream_reset            access_log_h2_uc_upstream_reset
access_log_rf_upstream_reset             access_log_h2_urx_retry_exhausted
admin_config_dump_server_info            access_log_rcd_upstream_reset
                                         access_log_rf_upstream_reset
TAIL = union − intersection              admin_config_dump_server_info
access_log_command_operators             xds_rds_hot_reload_fixture
access_log_h2_urx_retry_exhausted
xds_rds_hot_reload_fixture
```

Failing test names were extracted from the `---- <name> stdout ----` markers, **never by
indentation** — an indentation census also matches lines inside the failure BODY and invents phantom
test names. Failing binaries were derived from the preceding `Running` line.

The intersection is **exactly** the documented 5-member stable core. The tail has membership
**0 / 1 / 2** across the three sweeps — an open-ended tail whose SIZE carries no signal (ADR-0164).

### ADR-0164's four-part test, applied to every one of the 8 REDs

**CORE (5) — fail DETERMINISTICALLY in isolation; that determinism IS the environmental signature.**

```
$ cargo test -p differential --test <each>
access_log_h2_rcd_upstream_reset  exit=101
access_log_h2_uc_upstream_reset   exit=101
access_log_rcd_upstream_reset     exit=101
access_log_rf_upstream_reset      exit=101
admin_config_dump_server_info     exit=101
```

Each isolation run **reached its assertion** (it is not a bookkeeping failure) and reproduced the
documented signature verbatim:

```
---- access_log_rcd_upstream_reset stdout ----
fixture green: access log byte-exact mismatch: line 0 not byte-identical:
 envoy="{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{remote_connection_failure|
   immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:43387}\",\"rf\":\"UF\"}"
 envoy-rust="{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}"
```

```
---- admin_config_dump_server_info stdout ----
text_lines diverged after allow-lists:
  envoy-only:      ["backend::192.168.65.2:42525::canary::false", … 18 entries …]
  envoy-rust-only: []
```

That is the `TcpCloseBackend` IPv6-unreachable family (`[fdc4:f303:9324::254]`, upstream `rf: UF`
where envoy-rust correctly reports `rf: UC`) and the `192.168.65.2` bridge-IP family — both
host-environmental, both pre-existing, both CI-authoritative.

**TAIL (3) — each PASSES in isolation; the OPPOSITE signature.**

```
$ cargo test -p differential --test access_log_command_operators
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.60s
$ cargo test -p differential --test access_log_h2_urx_retry_exhausted
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.76s
$ cargo test -p differential --test xds_rds_hot_reload
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.98s
```

**These greens were asserted on the `1 passed` COUNT, never on the exit code** — `0 passed; N
filtered out` also exits 0, and `error: no test target named …` exits 101 exactly like a real RED.

All three failed in-sweep for the **identical startup-race reason, with the assertion NEVER
REACHED**:

```
---- xds_rds_hot_reload_fixture stdout ----
fixture passes: upstream Envoy never became accept-ready
Caused by:
    127.0.0.1:57226 not accept-ready within 10s: Connection refused (os error 111)
```
```
---- access_log_command_operators stdout ----
fixture green: upstream Envoy never became accept-ready
Caused by:
    127.0.0.1:57228 not accept-ready within 10s: Connection refused (os error 111)
```
```
---- access_log_h2_urx_retry_exhausted stdout ----
fixture green: upstream Envoy never became accept-ready
Caused by:
    127.0.0.1:57236 not accept-ready within 10s: Connection refused (os error 111)
```

The harness gave up waiting for the upstream container before the fixture's comparison logic ran a
single byte. **A RED that never reached an assertion carries no information about matching
semantics.**

**`xds_rds_hot_reload_fixture` deserved — and got — extra scrutiny, because it is the ONE RED that
sits on a surface `76.2` actually changed** (Task 8 rewrote `crates/envoy-config/src/rds.rs`'s
`if let` into an exhaustive `match`). Its RED is nonetheless **not** a `76.2` signal: the failure is
`upstream Envoy never became accept-ready`, i.e. the test aborted before any RDS reload was
performed or compared, and it **passes in isolation on this exact tree** (`1 passed`, 1.98s, with the
envoy-rust listener visibly bound in the log). Had Task 8's change been wrong, the RED would have
been a validation verdict, not a container-startup timeout.

**Leg (iv) — untouched by the phase's surface.** Mechanically checked: for each of the 8 RED
binaries, the fixture directories were derived FROM THE TREE (`grep -oE 'tests/fixtures/[0-9]{4}-…'`
on the test file, never guessed from the binary name) and each fixture's configs grepped for a
`redirect:` key:

```
access_log_h2_rcd_upstream_reset   [0070-accesslog-h2-rcd-upstream-reset]   redirect-using-configs=0
access_log_h2_uc_upstream_reset    [0069-accesslog-h2-uc-upstream-reset]    redirect-using-configs=0
access_log_rcd_upstream_reset      [0062-accesslog-rcd-upstream-reset]      redirect-using-configs=0
access_log_rf_upstream_reset       [0061-accesslog-rf-upstream-reset]       redirect-using-configs=0
admin_config_dump_server_info      [0014-admin-config-dump-server-info]     redirect-using-configs=0
access_log_command_operators       [0040-accesslog-command-operators]       redirect-using-configs=0
access_log_h2_urx_retry_exhausted  [0067-accesslog-h2-urx-retry-exhausted]  redirect-using-configs=0
xds_rds_hot_reload                 [0034-xds-rds-hot-reload]                redirect-using-configs=0
```

**Not one of the 8 configures a `redirect:` route.** A regression in the `location` builder, in
`synth_redirect`, in the dispatch arm or in the `prefix_rewrite` `:path` mutation could not express
itself through any of them.

**All four legs hold for all 8 REDs. Gate (b) is GREEN.**

### The arithmetic identities — both close EXACTLY

**Identity 1: `local passed + local failed == CI passed`.** CI totals for the base commit
`a2ebc8a2a791012504ef13140d9c90a826388b6c` were re-derived from the run log rather than inherited
(run `30639873487`, `gh` invoked **from the repo root** so it could resolve the base repo; the log
measured **402 314 bytes**, i.e. hundreds of KB, so it is a real log and not the ~120-byte
`failed to determine base repo` stub):

```
$ gh api repos/pgdad/envoy-rust/actions/jobs/91186772168/logs | grep -oE 'test result: (ok|FAILED)\. …' | awk '{b++;p+=$4;f+=$6}'
binaries=163 passed=2148 failed=0
```

```
sweep 1:  2143 + 5 = 2148 == 2148   ✓
sweep 2:  2142 + 6 = 2148 == 2148   ✓
sweep 3:  2141 + 7 = 2148 == 2148   ✓
```

**Identity 2: CI totals GREW, and the growth is fully accounted for.** The net
`#[test]`/`#[tokio::test]` attribute delta was measured directly from the diff:

```
$ git diff 0ea2de1 HEAD -- . ':(exclude)docs/' | grep -cE '^\+\s*#\[(tokio::)?test\]'   → 11
$ git diff 0ea2de1 HEAD -- . ':(exclude)docs/' | grep -cE '^-\s*#\[(tokio::)?test\]'    → 0
```

**+11, −0** = 10 new in-process tests + 1 new differential binary. Both figures close exactly:

```
binaries:  162 + 1  = 163   ✓
passed:   2137 + 11 = 2148  ✓
```

> **`PLAN.md` §6's prediction of `passed≈2168` is WRONG and is SUPERSEDED BY MEASUREMENT.** It
> assumed ~30 new tests. The real figure is **11**, and the gap is **by deliberate design, not lost
> coverage**: the 22 MEASURED `location` cells are **ONE table-driven test**
> (`plan_redirect_matches_every_measured_location_cell`), which `PLAN.md` §4 itself argued for
> because expanding them into 22 `#[test]` fns would have eaten the entire §6.1 size headroom.
> **Do not read `2148` against `2168` as "tests were lost."**

All 11 new tests were confirmed individually present and `ok` in sweep 1:

```
rds_reload_rejects_a_conflicting_redirect_oneof            ok
rds_reload_accepts_a_valid_redirect_route                  ok
build_response_prefix_rewrite_mutates_the_request_path     ok
build_response_path_redirect_leaves_the_request_path_alone ok
plan_redirect_matches_every_measured_location_cell         ok
plan_redirect_reports_a_rewritten_path_only_for_prefix_rewrite ok
plan_redirect_is_total_on_degenerate_spans                 ok
synth_redirect_emits_five_names_and_no_content_type        ok
canonical_reason_covers_the_three_redirect_codes           ok
h2_shared_seam_serves_the_redirect_arm                     ok
route_redirect_action_fixture                              ok
```

## S4.3 — gate (a): fixture `0086`, and WHY its 1-second green is trusted

```
$ cargo test -p differential --test route_redirect_action -- --nocapture
exit=0
test route_redirect_action_fixture ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.02s
```

`route_redirect_action_fixture ... ok` **in all three full parallel sweeps as well as in isolation**,
which also clears the "passes alone, fails under parallel load" differential flake family for it.

**A ~1 s green on a backend-free fixture is normal — but it is also exactly what a silent skip looks
like.** It was therefore audited two independent ways, neither of which reuses the state-3 session's
audit.

**Audit 1 — poll `docker ps` during the run.** Three containers were observed:

```
12073e255eda  envoyproxy/envoy:v1.33.0     ← THIS fixture's upstream
eebd9281dc70  testcontainers/ryuk:0.6.0    ← the testcontainers reaper
057ffbd5bc03  envoyproxy/envoy:contrib-v1.37.2  ← a SIBLING workstream's container, not ours
```

The pin was checked rather than assumed: the harness declares
`IMAGE_TAG: &str = "v1.33.0"` (`tests/differential/src/upstream.rs:56`), and
`envoyproxy/envoy:v1.33.0` resolves to image ID **`56da5afd7df3`**, matching `ENVOY_TARGET.md`'s
digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`. **D-3.7 pin
honoured.**

> A trap worth recording: the first `docker ps` line returned was the SIBLING's
> `contrib-v1.37.2` container, which momentarily looked like the harness running against the wrong
> image. It is not — it belongs to the parallel workstream. **With a concurrent workstream live, a
> `docker ps` census must be resolved to a container ID and image ID, not read off the first line.**

**Audit 2 — an independent NEGATIVE CONTROL, on a different probe than state 3 used.** In a scratch
`git worktree` created `--detach` at `a2ebc8a` (the main tree was clean and was never mutated), `r16`'s
`expected_status` was falsified from `308` to `307`:

```
NEGATIVE-CONTROL exit=101
thread 'route_redirect_action_fixture' panicked at tests/differential/tests/route_redirect_action.rs:37:10:
fixture passes: probe r16-response-code-308: upstream status 308 != expected 307
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.02s
```

**Read the words `upstream status 308`.** They can only have come from the pinned upstream Envoy
container actually running and answering — and they independently re-confirm the phase's own
MEASURED row R16 (`response_code: PERMANENT_REDIRECT` → 308). The fixture is genuinely driving both
proxies and comparing them. **The green is real.**

Full mutation hygiene was observed:
- the worktree was built with **`cargo build --workspace --all-targets`** (190 `Compiling` lines), not
  `-p envoy-bin` — the latter omits the `tcp-echo-server`/`http1-echo-server`/`http2-echo-server`
  helper backends and produces FALSE REDs that never reach an assertion;
- the mutation was **re-grepped as still present after the run** (a parallel agent's `git checkout`
  can silently revert an in-place mutation) — still there;
- an **UNMUTATED CONTROL was run from the SAME worktree** after reverting:
  `test result: ok. 1 passed; 0 failed; … finished in 1.01s`. A RED that never reached an assertion
  is not evidence; this one did, and its control passes;
- the worktree was removed and the removal **re-verified from the repo root**
  (`git worktree list` now shows only the main tree and the four sibling `agent-*` worktrees, which
  were left alone). Main tree clean at `a2ebc8a`, `r16` back to `expected_status: 308`.

### `0086`'s four authoring constraints, RE-VERIFIED mechanically

Not taken on the state-3 session's word — a subagent or prior-session finding is a claim, not a
result.

| constraint | **MEASURED** |
|---|---|
| 18 probes / 18 distinct paths | **18 / 18** |
| 18 routes / 18 distinct prefixes | **18 / 18** |
| no prefix shadows another | **shadowing pairs: NONE** |
| each probe selects a DIFFERENT route | **distinct routes selected: 18**; routes selected by >1 probe: NONE |
| no unprobed route, no unmatched probe | **unprobed routes: NONE; unmatched probes: NONE** |
| `{{ADMIN_PORT}}` must NOT appear | **0** occurrences in all three fixture files |
| `location` NOT allow-listed | **0** occurrences of `"location"` in `tests/differential/src/lib.rs` |

The route-selection check was computed by first-match-wins prefix semantics over the parsed
`expectations.yaml`, i.e. the same rule the router uses — not by eye.

**`envoy-rust.yaml` is still mechanically derivable from `envoy.yaml` by exactly three hunks**, so the
two configs cannot silently drift:

```
$ diff envoy.yaml envoy-rust.yaml   → exactly 3 hunks
  +node: {id: x, cluster: y}
  -socket_address: { address: 0.0.0.0,   port_value: {{PORT}} }
  +socket_address: { address: 127.0.0.1, port_value: {{PORT}} }
  -admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
```

**The entire route table is byte-identical between the two configs.** That property is what makes the
fixture a real differential rather than two independently-authored configs, and it is preserved.

## S4.4 — gate (c): conformance suites

```
$ cargo test -p h2spec-conformance --test h2spec_runner -- --nocapture
exit=0
h2spec_runner: h2spec not found — skipping locally
test h2spec_pass_rate_gate ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**The local `ok` is NOT conformance evidence.** The gate self-skips silently on this host, and the
`h2spec not found — skipping locally` line is visible **only under `--nocapture`** — which is exactly
why a bare `h2spec_pass_rate_gate ... ok` inside a workspace sweep must never be read as conformance.
It is quoted here so the honest local state is on the record.

**CI is authoritative, and CI ran it for real.** In run `30639873487` the workflow installed h2spec
`2.6.0` (`ci.yml:43-49`, `h2spec --version`) and then:

```
2026-07-31T14:52:39.6834560Z test h2spec_pass_rate_gate ... ok
2026-07-31T14:52:39.6835079Z test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Per **ADR-0163** the CI gate is **NOT vacuous** — that was measured and SETTLED at the 75.2 review and
is not re-raised here. `tests/conformance/h2spec/known-failures.txt` is **byte-unchanged at 21 lines**;
**no line was trimmed**, and no fixture was weakened. Gate (c) **GREEN (CI-authoritative)**.

## S4.5 — gate (d): fuzzers

**`76.2` adds NO fuzz target, so gate (d) requires no new `cargo fuzz` run and no `ci.yml` edit.**
Measured rather than asserted:

```
$ git diff --name-only 0ea2de1 HEAD | grep -E 'fuzz|ci\.yml'
  (no output — no fuzz file and no ci.yml file changed)
$ git diff --stat 0ea2de1 HEAD -- .github/workflows/ci.yml
  (empty — byte-unchanged)
$ git ls-files '*/fuzz/fuzz_targets/*.rs'   → 5 targets across 5 crates
```

The five pre-existing targets (`parse_bootstrap`, `jwt_parse`, `cdn_loop_parse`,
`accesslog_format_parse`, `grpc_health_decode`) are unchanged and all five ran clean in CI's `fuzz`
job on the base commit — **conclusion `success`, 13 steps**. Gate (d) **GREEN (not applicable, and
verified as not applicable)**.

## S4.6 — the six-gate adjudication

| gate | verdict | evidence |
|---|---|---|
| **(a)** all new/changed differential fixtures green | **GREEN** | `0086` `ok` in isolation (1.02s) and in **all three** parallel sweeps; audited by a Docker poll resolving a real `envoyproxy/envoy:v1.33.0` container matching the `ENVOY_TARGET.md` digest, and by an **independent** negative control on probe `r16` (`upstream status 308 != expected 307`) under full mutation hygiene with a same-worktree unmutated control |
| **(b)** all pre-existing differential fixtures still green | **GREEN** | 3 sweeps × 163 binaries; sum invariant **2148**; intersection = the documented 5-member deterministic core; the 3-member tail each **passes in isolation**, was **absent from ≥2 sweeps**, **never reached an assertion**, and touches **no** `redirect:` route. ADR-0164's four-part test satisfied on all 8 |
| **(c)** conformance suites at the declared threshold | **GREEN (CI-authoritative)** | local run self-skips (quoted honestly); CI installed h2spec 2.6.0 and passed `3 passed; 0 failed`; `known-failures.txt` byte-unchanged at 21 lines (ADR-0163) |
| **(d)** any new fuzzer clean on its short-budget CI run | **GREEN (n/a, verified)** | no fuzz target added; `ci.yml` byte-unchanged; the 5 existing targets unchanged and the CI `fuzz` job `success` at 13 steps |
| **(e)** build / clippy / fmt / test / deny all clean | **GREEN** | build exit 0 (14 `Compiling`); clippy exit 0 (14 `Checking`, `-D warnings`); fmt exit 0, **zero bytes**; test = gate (b); deny `advisories ok, bans ok, licenses ok, sources ok` |
| **(f)** `REVIEW.md` approved | **OPEN — legitimately, BY DESIGN** | `REVIEW.md` does **not** exist. State 5 is a **separate session** (§5.1; ADR-0127 — a verifier must not review what it verified). This session does **not** claim gate (f); it is the next session's to close |

**Five of six gates are GREEN. Gate (f) is OPEN by construction at state 4.** No gate REDded, so
**there is no §5.2 re-entry to state 3.**

## S4.7 — spot-verification of the implementation itself

The gate is about commands, but a verifier that only reads exit codes verifies nothing. These were
re-checked directly on disk.

- **T-C9 was deliberately FLIPPED and RENAMED.** `build_response_redirect_is_not_implemented_placeholder`
  no longer exists as a test; the name survives **only inside the new test's doc comment**, which
  explains the flip. The replacement,
  `build_response_redirect_emits_301_and_location` (`crates/envoy-http1/src/hcm.rs`), asserts the 301,
  the exact `location`, the **absence of `content-type`**, and the `direct_response` detail string.
- **`synth_501` was correctly NOT deleted** — defined at `hcm.rs:2501`, still consumed by the chunked
  `Transfer-Encoding` path at `hcm.rs:915`.
- **CF-76-2 is genuinely CLOSED.** `crates/envoy-config/src/rds.rs` no longer contains
  `if let crate::RouteAction` (grep: 0 hits); it is an **exhaustive `match`** with a `Redirect` arm
  calling the shared `crate::bootstrap::validate_redirect_oneofs` (`rds.rs:159`), the same function
  the boot path calls (`bootstrap.rs:4111`, defined `bootstrap.rs:2674`). The `DirectResponse` arm is
  an explicit `=> {}` whose comment names the OPEN **ADR-0028** deferral. **ADR-0028 is NOT lifted** —
  no `validate_hcm`, no `InvalidStatusCode`, no `validate_data_source` was added to the RDS path.
- **M-1 + M-2 are genuinely CLOSED.** `pub enum RouteAction` (`bootstrap.rs:2253`) now carries its own
  doc block, and that block is **corrected** — it describes the THREE-way oneof
  (`direct_response:` / `route:` / `redirect:`), not the stale two-way text, and it carries a standing
  warning that a FOURTH variant will **not** fail the build everywhere it must. `RedirectResponseCode`
  (`:2178`) retains its own separate, correct doc block. **Nothing is orphaned and nothing is stale.**
- **`BEHAVIOR_CONTRACT.md` Phase 76** (`:2957`) is present and structurally complete: §A (16 `R` rows
  + 4 `Q` authority rows + 2 `E` edge rows + the derived rules (a)-(e)), §B (the header set, the
  no-`content-type` finding), §C (the five reason phrases and why they are not differentially
  witnessed), §D (access-log observables), §E (the `HEADER_ALLOW_LIST` standing prohibition),
  §F (**eight** not-measured items, of which **7 and 8 were CREATED by this implementation** and are
  **unwitnessed by construction**).

**A standing caution restated, because it is the phase's most dangerous failure mode:** `location` is
**not** allow-listed, so `diff_headers` compares it **value-exact**, and that comparison **is** `0086`'s
entire witness. Adding `location` to `HEADER_ALLOW_LIST` would silently vacate every `location`
assertion in the corpus **while leaving the fixture green** — success-shaped failure. It was verified
absent (0 hits) and must stay absent.

## S4.8 — findings from this verification session

**(1) A DOCUMENTED HOST STALL DID NOT REPRODUCE, AND THAT IS ITSELF A MEASUREMENT.** The state-3
session recorded — and the handoff instructed this session to budget for — a hard stall on
`xds_rds_hot_reload` that forced its sweep to be killed after ~11 minutes and left gate (b)
unadjudicated. Three consecutive whole-workspace sweeps here completed in ~5½ minutes each with all
163 binaries reporting. **An environmental stall is not a property of the tree; do not inherit a
budget for one without re-measuring, and do not read its absence as a different tree.**

**(2) THE ONE RED SITTING ON THE PHASE'S OWN SURFACE WAS THE ONE WORTH CHASING.** Seven of the eight
REDs were trivially off-surface. `xds_rds_hot_reload_fixture` was not — Task 8 rewrote exactly that
file's validation path. Adjudicating it required reading the failure TEXT (a container-startup
timeout, assertion never reached) rather than its NAME. **When a RED's name overlaps the phase's
surface, the failure text is the only thing that separates a coincidence from a regression.**

**(3) A `docker ps` CENSUS IS UNSAFE TO READ OFF THE FIRST LINE WHILE A PARALLEL WORKSTREAM IS
LIVE.** The first container returned during the `0086` audit was a sibling's
`envoyproxy/envoy:contrib-v1.37.2` — a plausible-looking "the harness is running against the wrong
image" alarm. Resolving container ID → image ID → tag → `ENVOY_TARGET.md` digest showed our own
container was the correctly pinned `v1.33.0` (`56da5afd7df3`). **Adjudicate by ID, never by the first
matching line.**

**(4) THE `$5`/`$7` VACUOUS-CENSUS TRAP WAS RE-CONFIRMED LIVE, NOT MERELY CITED.** The same CI log
that yields `binaries=163 passed=2148 failed=0` under awk fields `$4`/`$6` yields a clean-looking,
entirely false `binaries=163 passed=0 failed=0` under `$5`/`$7`. **The wrong recipe does not error —
it returns a believable zero. Disbelieve a zero.**

**(5) THE PLAN'S OWN TEST-COUNT PREDICTION WAS WRONG BY ~3×, AND THE MEASUREMENT IS THE ANSWER.**
`PLAN.md` §6 predicted `passed≈2168` from "~30 new tests"; the measured attribute delta is **+11**.
Both arithmetic identities close exactly on 11 (`162+1=163`, `2137+11=2148`). The gap is the
table-driven 22-cell design the plan itself chose. **A prediction inside a landed artifact is not a
baseline; the diff is.**

## S4.9 — what this session did NOT do

- **No review.** `REVIEW.md` was not created. State 5 is a separate session (ADR-0127).
- **No code fix.** No gate REDded, so there was no §5.2 re-entry. Nothing was "fixed while I was in
  there."
- **No ROADMAP status cell flipped.** A state-4 commit flips none. Row `76.2` stays `planned`, `76`
  stays `in-progress`, `76.1` stays `done`.
- **No edit to `76.2/SPEC.md`, `76.2/PLAN.md`, `76/SPEC.md`, or any of `76.1`'s four artifacts**
  (D-3.5). Verified byte-unchanged.
- **No new ADR.** Head **ADR-0170**, next free **ADR-0171**, re-derived on disk. Nothing here is a new
  decision: the gate is §7.5, the flake adjudication is ADR-0164, the h2spec question is ADR-0163, and
  the state separation is ADR-0127.
- **No banked carry-forward fixed** (§6.3). CF-76-1, CF-75-6, CF-75-5, CF-75-4, CF-75-3, CF-75-2 and
  the `76.1` review's remaining Minors/Nits all remain open and untouched.
- **No `known-failures.txt` trim, no `ci.yml` edit, no `HEADER_ALLOW_LIST` change, no fixture
  weakened.**
- **No `stop` file.** The stop condition was RE-MEASURED (not inherited) and **all three legs came
  back FALSE**: (i) 107 rows with **two** still non-`done` (`76`, `76.2`); (ii) `76.2` is implemented
  but **not `done`** — it has no `REVIEW.md`, and states 5 and 6 both remain; (iii) **four** of the 11
  family headings still carry **zero** rows (`### HTTP/3 + QUIC family` `ROADMAP.md:122`,
  `### gRPC family` `:126`, `### Runtime + hot restart family` `:183`, `### WASM host family` `:185`),
  re-derived by slicing each `### ` heading to the next — the naive `awk` for this under-reports 4 as 1.

## S4.10 — hand-off to §5 state 5

**State 4 is COMPLETE.** Five of the six §7.5 gates are GREEN on measured, quoted evidence; gate (f)
is OPEN by design. `STATE.md` advances to **§5 state 5** with
`## Next expected skill` = **`superpowers:requesting-code-review`**.

The state-5 session should read, in this order: the **Deviations D-1..D-6** table at the top of this
file (three of the six are defects in `PLAN.md`'s own *pre-flighted* literal Rust, so a reviewer
diffing plan text against landed code will find intentional differences); then **S4.7** above, which
records what this verification checked directly on disk so the review need not re-derive it; then
**§F items 7 and 8** of `BEHAVIOR_CONTRACT.md`'s Phase 76 section, which are this phase's two
**unwitnessed-by-construction** choices and are the most likely place for a reviewer to find
something worth saying.

**The reviewer must not fix what it grades** (ADR-0127; ADR-0165), and **must not** flip a ROADMAP
status cell — row `76.2` stays `planned` until its own state-6 close-out, at which point parent row
`76` closes with it.

## S4.11 — the ADR-0035 relocation, checked mechanically

The state advance relocated the standing **14**-line set (Active-phase `**id:**` / `**slug:**` /
`**directory:**` / `**status:**` + its `_Historical_` pointer; the next-skill pointer + scope block +
the Standing-traps line + its `_Historical_` pointer; last-commit + pointer; last-updated + pointer;
and the §5.1 doctrine bullet). Both files were backed up first, and **the superseded lines were
captured from the pre-edit backup BEFORE anything was mutated** — capturing afterwards archives the
NEW text and silently passes. Each captured line was asserted to start with its expected prefix
(14/14) before a single byte was written.

| check | result |
|---|---|
| **PER-FILE** delta on each of the 14 (a COMBINED count is invariant by construction and false-passes) | **14/14**: `STATE.md` 1→**0**, `STATE_HISTORY.md` **+1** each |
| §4.1(9)(d) decomposition of `(old STATE.md) − (new STATE.md)` | **22** non-blank removed = **14** relocated + **8** superseded IN PLACE |
| the 8 in-place lines | the ACTIVE-phase `### Sub-phase 76.2 …` Notes subsection, which §4.1(9)(b) **keeps** in `STATE.md` and retires only at close-out |
| history lines lost | **0** |
| `STATE.md` headings | **9 → 9**, with exactly **one** intended change (the Notes rename `state-3 implementation` → `state-4 verification`) |
| duplicated non-blank lines in `STATE.md` | **0** |
| duplicated non-blank lines in `STATE_HISTORY.md` | 204 → 205, **exactly +1** — the per-section `_Relocated at …_` marker appears 4× BY DESIGN |
| `STATE_HISTORY.md` headings | **628 → 628**, unchanged |
| archive headers matched | by **EXACT whole-line equality**, each resolving to exactly one line; all four resolved **before** any write, so a miss aborts before mutating |
| new lines byte-identical to superseded? | **none** — a byte-identical rewrite fails the per-file check BY DESIGN |

**ADR-0160 token sweep** (retention verified mechanically, never by eye). The Standing-traps line
grew **47 154 → 49 917** characters, 472 → 490 distinct backticked tokens. Three tokens showed a
count drop and each was adjudicated rather than waved through:

- **`PLAN.md` in the traps line, 10× → 9×.** Still present nine times; the count fell only because the
  new head paragraph replaced the old one. **No enduring trap lost.**
- **`superpowers:executing-plans` and `superpowers:subagent-driven-development` in the doctrine
  bullet, 1× → 0×.** These named the NEXT skill while state 3 was next. The bullet's function is to
  name the next skill, and state 5's is `superpowers:requesting-code-review`, which the new bullet
  names. **This is the intended supersession that ADR-0160 exists to permit** — the old wording is
  preserved verbatim in `STATE_HISTORY.md` (+1 there, 0 in `STATE.md`), which is the whole point of
  relocating before rewriting.

No other backticked token was dropped from either line.
