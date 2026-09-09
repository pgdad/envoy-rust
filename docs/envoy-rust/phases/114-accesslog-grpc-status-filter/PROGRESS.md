# Phase 114 — PROGRESS

> §5 **state 3** — the implementation of
> `docs/envoy-rust/phases/114-accesslog-grpc-status-filter/PLAN.md`.
> Appended to on each task completion, with REAL quoted command output. Written
> for a stranger with zero prior context (D-3.4).
>
> **The unit:** the `grpc_status_filter` access-log FILTER arm — the SEVENTH of
> upstream Envoy's twelve `envoy.config.accesslog.v3.AccessLogFilter` oneof arms
> — gating a sink's per-record emission on the request's **UNGATED** effective
> gRPC status: the response `grpc-status` header if present, else the phase-110
> `http_to_grpc_status` map over the response code. Witnessed by NEW differential
> fixture `0094-accesslog-grpc-status-filter`.
>
> **Where `PLAN.md` and `SPEC.md` disagree, `PLAN.md` wins** (`ADR-0197`, which
> corrects five landed `SPEC.md` claims).

---

## Session preconditions, re-derived from disk before Task 1

`git rev-parse HEAD` = `c9136ae5f89ada5f1e64e93e2a1a62a19bc5fcc9`, the phase-114
§5 state-2 **CI-record** commit; `git status --porcelain` empty; branch `main`;
`git fetch origin --prune` exit **0** with `origin/main` at the same SHA. The
`## Last commit` block of `STATE.md` was read STRUCTURALLY and already carries a
`CI CONFIRMED` answer for `adc4c7d2aafa8487442a005714e322e8e7217538`, so **no
outstanding CI-record commit was owed** and none was written.

**The §5 state-3 detection rule was re-derived, not inherited:** the phase
directory holds `SPEC.md` (275 lines) and `PLAN.md` (1567 lines) and NO
`PROGRESS.md` and NO `REVIEW.md`.

**Baseline `cargo build --workspace --all-targets` exit 0** before any edit.

### The three stop-condition legs, re-measured from disk — ALL THREE FALSE

- **Leg (i) — FALSE.** `ROADMAP.md`: **122 rows / 121 `done` / 0 `in-progress` /
  1 `planned`**, the `planned` row being `114` at file line **196**. Driven from
  the `^\| [0-9]` prefix with status at field **4** of a `' | '` (spaces) split;
  the status buckets SUM to the row count (121 + 1 = 122 ✓). The forbidden
  `NF == 6` filter was run as a control and reads **120**, dropping exactly the
  two rows with unescaped in-cell pipes (file lines 168 at NF=7 and 169 at
  NF=10). Those are append-only history and were NOT "fixed". This session does
  not touch `ROADMAP.md`, so leg (i) is unchanged at its end.
- **Leg (ii) — FALSE.** **14** crates
  (`envoy-{accesslog,admin,bin,cluster,config,filter,health,http1,http2,jwt,listener,stats,tcp,tls}`).
  `envoy-http3` / `envoy-grpc` / `envoy-wasm` / `envoy-protos` / `envoy-runtime`
  all re-confirmed absent by `test -d`. Over the **28** manifests of
  `git ls-files '*Cargo.toml'` (the OTHER method, `crates/*/Cargo.toml` + root,
  gives 15 — the denominator is method-dependent and this is the set counted):
  `quinn` / `wasmtime` / `tonic` / `opentelemetry` / `prost` = **0** each.
  **Positive control, IDENTICAL invocation shape: `tokio` = 19 of 28.**
- **Leg (iii) — FALSE.** **11** `### ` family headings, of which **ONE** carries
  ZERO rows (`### WASM host family`). Censused from a single `/^### /` rule that
  seeds every heading at 0, because a naive `awk` never emits the zero-row one:
  10 / 5 / 3 / 14 / 3 / 4 / 6 / **31** / 6 / **0** / 13, with **27** rows before
  the first `### ` heading, summing to 122 ✓. The filing defect is intact and was
  NOT repaired: **7** rows whose title begins `Observability family:` sit
  physically under `### Deprecated / edge features`.

  ⚠ **One inherited figure did not reproduce.** The handoff states the LOGICAL
  Observability family is **38** (= the heading slice's 31 + the 7 misfiled).
  Counting instead by the row TITLE field (`$2 ~ /^Observability family/`) gives
  **34** (= 27 + 7). The gap is **4** rows filed under the Observability heading
  whose titles read `Observability / HTTP-filters family:` (ids 34, 35, 36, 37) —
  they belong to the heading slice but not to the title census. Both numbers are
  defensible; they answer different questions. Neither is load-bearing for leg
  (iii), which turns on the heading count and the zero-row heading.

**Overall: the mission is NOT complete and NOT ONE LEG HOLDS.** `ADR-0167`
DECISION 2 governs. `ls stop` returns `No such file or directory` and **no
`stop` file was created**.

---

## A PLAN prediction corrected at Task 1 — the numstat, not the edit

`PLAN.md` Task 1 Step 3 predicts `git diff --numstat` will read
`14	14	crates/envoy-http2/src/hcm.rs` and instructs: *"If the two numbers
differ, text was altered; revert and redo."*

**The measured numstat is `10	10`.** The two numbers do **not** differ, so the
step's own stated invariant — equal insertions and deletions, nothing but
position changed — HOLDS. What is wrong is the predicted *magnitude*: git found
a more compact minimal edit than the 14-line block move, because four of the
moved lines are the bare `///` separators and the closing `}`/blank, which align
against the destination's own text and are therefore not re-emitted.

**A numstat is a rendering of a diff algorithm's choice, not a property of the
edit.** So the move was verified against the property the step actually cares
about, by a stronger check than the one the plan names:

```
before lines: 7762  after lines: 7762
MULTISET IDENTICAL: True
BYTE COUNT before/after: 352248 352248 equal: True
TEXT CHANGED AT ALL: True
```

The line multiset and the byte count are both invariant while the text is not —
which is exactly and only "position changed". `PLAN.md` is landed and is NOT
edited; this is the forward correction.

---

## Task 1 — RIDER: re-attach `finalize_h2_stream`'s doc comment (CF-113-7)

**Status: COMPLETE.** Commit: `phase 114 rider: re-attach finalize_h2_stream's stolen doc comment (CF-113-7)`.

This is a deliberate **rider**, not a deliverable — `SPEC.md` §5 non-goal 6
authorises it because this phase touches this file, and requires it be taken in
its own commit, explicitly labelled. It is taken FIRST because it moves every
line number below 989 in a 7761-line file.

### Step 1 — locate by TEXT, assert the anchors are unique

```
$ grep -cF '/// 06.2 Task 7: factored per-stream finalization — sends the' crates/envoy-http2/src/hcm.rs
1
$ grep -cF 'fn h2_grpc_status() -> Option<String> {' crates/envoy-http2/src/hcm.rs
1
$ grep -cF '#[allow(clippy::too_many_arguments)]' crates/envoy-http2/src/hcm.rs
1
```

All three unique, so the third was usable as an anchor (the plan warns it may
not be). Line numbers re-derived at HEAD, not inherited: the doc block at
**989**, the helper at **1009**, file length **7761**.

**The defect, confirmed on disk before the edit.** `h2_grpc_status()`'s own
10-line doc comment sat *below* `finalize_h2_stream`'s 10-line doc comment with
no `///`-blank separator, so rustdoc concatenated them: `finalize_h2_stream`'s
prose documented a function whose entire body is `None`, and `finalize_h2_stream`
itself carried no doc comment at all. `fmt` and `clippy` cannot see this.

### Step 2 — the move

The 14-line run (file lines 999–1012: the helper's doc comment, the `fn`, and
its trailing blank) was cut and re-inserted immediately above file line 989. The
destination index needed no adjustment because everything removed sits BELOW it —
asserted in the script rather than assumed.

### Step 3 — byte-neutrality (see the correction above)

Resulting order, read back from disk:

```
/// The `%GRPC_STATUS%` backing value for the HTTP/2 access-log record.
///
/// ALWAYS `None` (CF-113-2). H2's gRPC status would have to come from the
/// response TRAILER block, and `finalize_h2_stream` MOVES `trailers` into
/// `send_envoy_response` before it builds the record — so the value is not
/// live at the record build. Phase 113 is HTTP/1.1-only by charter.
///
/// This is a named function rather than an inline `None` so the boundary is
/// pinned by `h2_grpc_status_is_absent` below: a phase that lifts it must
/// change a tested function, not silently edit a struct literal.
fn h2_grpc_status() -> Option<String> {
    None
}

/// 06.2 Task 7: factored per-stream finalization — sends the
/// downstream response via `send_envoy_response`, then (if the HCM
/// config carries access-log sinks) builds an `AccessLogRecord` and
/// emits it once per sink. Mirrors the H1 factored join-point at
/// `envoy_http1::serve_connection`'s tail.
///
/// Per PLAN-write SPEC correction 2 the access-log dispatch lands
/// AFTER `send_envoy_response` returns (covers both the empty-body
/// `send_response(.., end_of_stream=true)` branch and the non-empty
/// `send_data(.., end_of_stream=true)` branch uniformly).
#[allow(clippy::too_many_arguments)]
async fn finalize_h2_stream(
```

Byte-identical to the block `PLAN.md` Step 2 specifies.

### Step 4 — verify nothing broke

```
$ cargo test -p envoy-http2 --lib h2_grpc_status
   Compiling envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.59s
     Running unittests src/lib.rs (target/debug/deps/envoy_http2-3fa56dc263ab8ca6)

running 1 test
test hcm::h2_grpc_status_boundary_tests::h2_grpc_status_is_absent ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 125 filtered out; finished in 0.00s
```

`h2_grpc_status_is_absent` PASSES, as predicted — it calls
`super::h2_grpc_status()` and is position-independent.

```
$ cargo clippy -p envoy-http2 --all-targets -- -D warnings
    Checking envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.01s
clippy_exit=0
$ cargo fmt --all -- --check
fmt_exit=0
```

Both silent. The clippy run emitted a real `Checking envoy-http2` line, so it was
not a cached no-op.

### Step 5 — commit

`PLAN.md`'s `git add` list is used verbatim, **plus this `PROGRESS.md`**: the §5
state machine requires appending to `PROGRESS.md` on each task completion, and
the phase-113 state-3 precedent (`abbe107` … `0b0c7ea`) carried it in all ten
task commits. The commit MESSAGE is the plan's, verbatim.

**`CF-113-7` is CONSUMED.**

---

## Task 2 — widen `http_to_grpc_status` to `pub`, narrowly (PV-3)

**Status: COMPLETE.** Commit: `phase 114 task 2: narrow pub on http_to_grpc_status so both codecs share ONE map (PV-3)`.

This **DEPARTS** from phase 113's `ADR-0193` DECISION 6, deliberately and on a
scope argument: that decision refused a visibility widening because
`envoy-accesslog` is a LEAF crate and calling `envoy_http1::grpc` from it would
be a dependency **cycle**. The caller here is `envoy-http2`, which already
depends on `envoy-http1`, so no cycle exists on this edge.

### Steps 1–2 — the failing test, RUN and SEEN to fail

```
$ cargo test -p envoy-http2 --lib the_phase_110_map_is_reachable
   Compiling envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
error[E0425]: cannot find function `http_to_grpc_status` in crate `envoy_http1`
    --> crates/envoy-http2/src/hcm.rs:7768:33
     |
7768 |         assert_eq!(envoy_http1::http_to_grpc_status(404), 12);
     |                                 ^^^^^^^^^^^^^^^^^^^ not found in `envoy_http1`

error[E0425]: cannot find function `http_to_grpc_status` in crate `envoy_http1`
    --> crates/envoy-http2/src/hcm.rs:7769:33
     |
7769 |         assert_eq!(envoy_http1::http_to_grpc_status(200), 2);
     |                                 ^^^^^^^^^^^^^^^^^^^ not found in `envoy_http1`

error: could not compile `envoy-http2` (lib test) due to 2 previous errors
```

RED for exactly the reason `PLAN.md` Task 2 Step 2 predicts.

### A SECOND PLAN correction — the `pub use` insertion point

**`PLAN.md` Task 2 Step 3 says to add the re-export line "immediately above
`pub use response::{Http1Response, Response};`". Doing that literally FAILS the
plan's own `cargo fmt --all -- --check` gate.** rustfmt sorts the `pub use`
block alphabetically, and `grpc` sorts between `error` and `hcm`, not above
`response`:

```
$ cargo fmt --all -- --check
Diff in /home/esa/git/envoy-rust/crates/envoy-http1/src/lib.rs:31:
 pub use error::Http1Error;
+pub use grpc::http_to_grpc_status; // 114: the ONE HTTP->gRPC map, shared with envoy-http2.
 pub use hcm::{BuildOutcome, HCM, HCMConfig, HCMStats, build_response};
Diff in /home/esa/git/envoy-rust/crates/envoy-http1/src/lib.rs:37:
-pub use grpc::http_to_grpc_status; // 114: the ONE HTTP->gRPC map, shared with envoy-http2.
 pub use response::{Http1Response, Response};
fmt_exit=1
```

`cargo fmt --all` was run and the line moved to the sorted position. **This is
consistent with the prototype rather than a divergence from it**, and the
numstat proves it: `PLAN.md`'s own measured LoC table lists
`crates/envoy-http1/src/lib.rs` at **1 insertion / 0 deletions**, and the
post-`fmt` diff here reads exactly `1	0`. Had the prototype's line stayed above
`pub use response::`, that file would have shown a deletion too. So the plan's
*prose* mis-describes where its own *measured* line ended up. `PLAN.md` is landed
and is NOT edited; this is the forward correction.

### Step 3 — the widening, item-level only

`pub(crate) fn http_to_grpc_status` → `pub fn`, with the five-line comment
`PLAN.md` specifies, and ONE `pub use` in `lib.rs`. The module declaration and
both mutating/gating items were left exactly as they were — a `pub use`
re-exports an item out of a `pub(crate)` module without widening the module.

### Step 4 — GREEN, and nothing else leaked

```
$ cargo test -p envoy-http2 --lib the_phase_110_map_is_reachable
running 1 test
test hcm::h2_grpc_status_code_tests::the_phase_110_map_is_reachable_from_http2 ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 126 filtered out; finished in 0.00s
```

The three leak checks, which are the load-bearing half of this task:

```
pub(crate) mod grpc;                 = 1
pub(crate) fn is_grpc_request        = 1
pub(crate) fn apply_grpc_local_reply = 1
```

`1`, `1`, `1` as `PLAN.md` predicts. `is_grpc_request` and
`apply_grpc_local_reply` — the two items the module doc's hazard is actually
about — remain unreachable from outside `envoy-http1`.

### Boundary gates

```
$ cargo build --workspace --all-targets   -> build_exit=0
$ cargo clippy --workspace --all-targets --all-features -- -D warnings -> clippy_exit=0
$ cargo fmt --all -- --check              -> fmt_exit=0
```

Per-file numstat at this task's commit:

```
6	1	crates/envoy-http1/src/grpc.rs
1	0	crates/envoy-http1/src/lib.rs
10	0	crates/envoy-http2/src/hcm.rs
```

`grpc.rs` 6/1 and `lib.rs` 1/0 match `PLAN.md`'s measured table cell-for-cell.
