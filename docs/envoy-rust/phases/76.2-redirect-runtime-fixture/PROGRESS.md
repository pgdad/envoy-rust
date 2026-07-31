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
