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

---

## Task 3 — the `GrpcStatusFilter` config surface and the MEASURED token grammar

**Status: COMPLETE.** Commit: `phase 114 task 3: the GrpcStatusFilter config arm and its MEASURED token grammar`.

### A THIRD PLAN trap, hit and recovered: "add to `mod tests`" is not "append at EOF"

The first insertion of the four tests went to the last column-0 `}` in
`bootstrap.rs`, on the assumption that it closes `mod tests`. **It does not.**
The run was RED for the wrong reason:

```
error[E0425]: cannot find type `AccessLogFilter` in this scope
```

`AccessLogFilter` is in scope under `mod tests`' `use super::*`, so an
unresolved `AccessLogFilter` is a *location* failure, not a missing-item
failure. Censused structurally:

```
6102:#[cfg(test)]    6103:mod tests {              <- closes at 20338
20344/20345: mod serialize_roundtrip_tests {
20898/20899: mod typed_per_filter_config_tests {
21184/21185: mod per_route_absent_filter_tests {
21381/21382: mod csrf_validator_tests {
21617/21618: mod cdn_loop_config_tests {
21769/21770: mod set_metadata_config_tests {
21884/21885: mod header_to_metadata_config_tests {
21974/21975: mod header_to_metadata_validator_tests {
22103/22104: mod json_format_value_tests {         <- where the tests actually landed
```

`bootstrap.rs` carries **TEN** column-0 `#[cfg(test)] mod` blocks. `mod tests`
spans lines **6103–20338**; the file is 22169 lines. The insert was reverted with
`git checkout --` (the file was otherwise untouched at that point, so the revert
was total, and `git status --porcelain` was re-checked empty) and redone at the
FIRST column-0 `}` after `mod tests {`, with the neighbours asserted before the
splice.

### Steps 1–2 — the failing tests, RUN and SEEN to fail

```
$ cargo test -p envoy-config --lib grpc_status_filter
error[E0433]: cannot find type `GrpcStatusToken` in this scope
error[E0425]: cannot find function `resolve_grpc_status_token` in this scope
error[E0609]: no field `grpc_status_filter` on type `bootstrap::AccessLogFilter`
error: could not compile `envoy-config` (lib test) due to 18 previous errors
```

RED for exactly the two reasons `PLAN.md` Task 3 Step 2 predicts.

### Step 3 — the implementation

The seventh `Option` arm on `AccessLogFilter`, then `GrpcStatusFilter`,
`GrpcStatusToken` (untagged, `Num` FIRST — arm order is load-bearing),
`GRPC_STATUS_FILTER_NAMES` (17 entries, index IS the code) and
`resolve_grpc_status_token`, all verbatim from the plan. Three re-exports added
to `envoy-config`'s `lib.rs`.

### Step 4 — the `E0063` blast radius, driven from the compiler's error list

**Never from a text match.** The compiler reported **FIFTEEN** exhaustive
literals across two crates:

```
crates/envoy-config/src/bootstrap.rs   4   (14616, 14654, 14714, 14906)
crates/envoy-http1/src/hcm.rs         11   (4837, 4991, 5074, 5114, 5130,
                                            5151, 5158, 5298, 5463, 5483, 10754)
```

The `envoy-http1` set is why `PLAN.md`'s Task 3 `git add` list names that file —
adding a `pub` field to `AccessLogFilter` is a cross-crate change. Each literal
was located by brace-matching from the compiler-reported column to its own
closing brace, and `grpc_status_filter: None,` inserted at that literal's indent.

A guard was then run over both files asserting that **no** inserted line sits
inside a literal carrying `..AccessLogFilter::default()` / `..Default::default()`
— those absorb the field silently and must NOT be edited:

```
suspect functional-update literals touched: 0
grpc_status_filter: None,  ->  crates/envoy-http1/src/hcm.rs:11
                               crates/envoy-config/src/bootstrap.rs:4
```

By construction this could not have gone wrong: `rustc` only reports the
exhaustive ones. The guard exists because the plan names four textual
false-positive classes that a `grep`-driven sweep would have hit.

### A FOURTH finding — `E0027`, which the plan does not name

Beyond the fifteen `E0063`s the compiler also raised, at the Task-3 boundary:

```
error[E0027]: pattern does not mention field `grpc_status_filter`
    --> crates/envoy-config/src/bootstrap.rs:5816:9
5816 |       let AccessLogFilter {
```

`validate_access_log_filter`'s destructure has **no `..`**, so the seventh
binding is compiler-forced at the moment the field appears — at Task 3 — while
the plan schedules the destructure growth in Task **4**. The binding was
therefore added here, as the minimal edit that makes the crate compile.

**Its consumer is Task 4's token loop, so at THIS boundary it is unused:**

```
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
error: unused variable: `grpc_status_filter`
    --> crates/envoy-config/src/bootstrap.rs:5823:9
5823 |         grpc_status_filter,
     |         ^^^^^^^^^^^^^^^^^^ help: try ignoring the field: `grpc_status_filter: _`
     = note: `-D unused-variables` implied by `-D warnings`
clippy_exit=101
```

**This needs no new ADR — `PLAN.md`'s own Global Constraints already govern it**,
citing `ADR-0194` DECISION 2: *"a whole-slice prototype validates the SLICE,
never a TASK BOUNDARY — these are the plan's gates for the executor to run, not a
measured claim about each boundary."* This is the same failure class phase 113
measured at its Task-1 boundary, recurring for the same structural reason.

**The deferral, and its limits.** Only the `unused_variables` arm of `-D warnings`
is deferred, from Task 3 to Task 4 — the first boundary at which a consumer
exists. `cargo build --workspace --all-targets`, `cargo fmt --all -- --check` and
the task's own tests are required green HERE and were:

```
build_exit=0        (sole warning: the unused binding above)
fmt_exit=0
```

**Nothing is suppressed.** No `#[allow(unused_variables)]` and no
`grpc_status_filter: _` was added anywhere, precisely so that a forgotten
attribute cannot outlive the gap. Task 4 discharges `-D warnings` in full.

### Step 5 — GREEN

```
$ cargo test -p envoy-config --lib grpc_status_filter
running 4 tests
test bootstrap::tests::grpc_status_filter_rejects_every_measured_reject ... ok
test bootstrap::tests::grpc_status_filter_accepts_every_measured_token ... ok
test bootstrap::tests::grpc_status_filter_exclude_defaults_false ... ok
test bootstrap::tests::grpc_status_filter_mixed_token_list_deserializes ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 716 filtered out; finished in 0.00s
```

4 passed, as predicted. The arithmetic corroborates `PLAN.md`'s end-of-phase
target independently: `4 + 716 = 720`, and the plan's Task 4 Step 5 states the
baseline is **716** and the crate finishes at **722** after the six tests Tasks 3
and 4 add. Four are here; Task 4 adds two.

Per-file numstat at this task's commit:

```
202	0	crates/envoy-config/src/bootstrap.rs
20	19	crates/envoy-config/src/lib.rs
11	0	crates/envoy-http1/src/hcm.rs
```

`lib.rs`'s 19 deletions are rustfmt re-flowing the `pub use bootstrap::{…}`
block around the three new names, not removed exports.

---

## Task 4 — validation: the seventh arm and a fail-loud bad-token error

**Status: COMPLETE.** Commit: `phase 114 task 4: validate the seventh AccessLogFilter arm fail-loud on a bad status token`.

### A FIFTH trap, hit and recovered: an anchor that is NOT unique

The first scripted edit aborted on its own uniqueness assertion:

```
AssertionError: ('        assert!(matches!(\n            err,\n            crate::ConfigEr', 2)
```

That `assert!(matches!(…AmbiguousAccessLogFilter…))` run occurs **twice** in
`bootstrap.rs`. The script asserts `count == 1` **before** any write and writes
only at the end, so nothing was modified — `git status --porcelain` was re-checked
empty afterwards. The anchor was extended to include the preceding all-arms
literal tail and the `validate_access_logs(…)` call, which is unique, and the
edit was redone. **Assert the anchor occurs exactly once, and do the write last.**

### Steps 1–2 — the failing tests, RUN and SEEN to fail

`six_arm_cardinality_counts_every_arm` → `seven_arm_cardinality_counts_every_arm`,
a seventh entry in its `single_arms` vector, `assert_eq!(single_arms.len(), 7)`,
and the all-arms literal's `grpc_status_filter` flipped `None` → `Some(…)` (the
binding renamed `all_six` → `all_seven` to match). Plus the two new tests.

```
$ cargo test -p envoy-config --lib grpc_status_filter_bad_token
error[E0599]: no variant named `UnknownGrpcStatus` found for enum `ConfigError`
error: could not compile `envoy-config` (lib test) due to 1 previous error; 1 warning emitted
```

RED for exactly the reason `PLAN.md` Task 4 Step 2 predicts. (The trailing
"1 warning emitted" is the Task-3 unused destructure binding, still outstanding
at this point and discharged below.)

### Steps 3–4 — the error variant, the validator, and three stale doc counts

`ConfigError::UnknownGrpcStatus { token: String }` added immediately after
`UnknownResponseFlag`, its exact analogue. In `validate_access_log_filter`:
`grpc_status_filter.is_some(),` appended to the `set_arms` array, and the token
loop spliced before the closing `Ok(())`. The seventh destructure binding was
already present — Task 3 was forced to add it by `E0027`.

**The `set_arms` growth is the load-bearing half of this task**, and it is the
one the compiler does NOT force: the array is not length-checked, so an arm
present in the struct but missing from the array counts as ZERO and turns a
valid single-arm filter into `AmbiguousAccessLogFilter{"no filter variant is
set"}`. `seven_arm_cardinality_counts_every_arm` is what catches that, and it
passes.

Splice location verified structurally rather than by line number: the loop sits
at line 5899 and `validate_access_log_filter` closes at 5915.

**Beyond what the plan names, THREE doc statements became factually false** the
moment the seventh arm landed, and all three were corrected:

| site | was | now |
|---|---|---|
| the `AccessLogFilter` struct doc | *"This type models SIX oneof arms"* | SEVEN, naming `grpc_status_filter` (phase 114) |
| `validate_access_logs`' contract doc item 3 | *"Phases 70/71/72/73/74 give SIX arms"* | *"Phases 70/71/72/73/74/114 give SEVEN arms"* |
| `validate_access_log_filter`'s own doc | *"cardinality, all SIX arms"* | *"all SEVEN arms"* |

`PLAN.md` Step 4 names only the second. The other two sit in the same two files
this task edits and would otherwise have been left asserting a count the code
contradicts.

### Step 5 — GREEN, and the Task-3 deferral discharged

```
$ cargo test -p envoy-config --lib grpc_status
running 6 tests
test bootstrap::tests::grpc_status_filter_accepts_every_measured_token ... ok
test bootstrap::tests::grpc_status_filter_rejects_every_measured_reject ... ok
test bootstrap::tests::grpc_status_filter_exclude_defaults_false ... ok
test bootstrap::tests::grpc_status_filter_mixed_token_list_deserializes ... ok
test bootstrap::tests::grpc_status_filter_bad_token_is_fail_loud ... ok
test bootstrap::tests::grpc_status_filter_empty_statuses_loads ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 716 filtered out; finished in 0.00s

$ cargo test -p envoy-config --lib seven_arm_cardinality
running 1 test
test bootstrap::tests::seven_arm_cardinality_counts_every_arm ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 721 filtered out; finished in 0.00s

$ cargo test -p envoy-config --lib
test result: ok. 722 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

**`722 passed; 0 failed` is exactly the figure `PLAN.md` §6.1 states the
prototype measured for this crate at the end of the phase.** The crate is now
complete for this phase (Tasks 5–10 touch other crates), so the two numbers are
measuring the same thing and they agree.

Boundary gates — **all three green, including `-D warnings`**:

```
build_exit=0
clippy_exit=0     <- the Task-3 unused-binding deferral is DISCHARGED IN FULL
fmt_exit=0
```

No `#[allow]` was added at Task 3 and none was removed here; the warning went
away because its consumer landed, which is the only correct way for it to go
away.

Per-file numstat at this task's commit:

```
59	10	crates/envoy-config/src/bootstrap.rs
6	0	crates/envoy-config/src/lib.rs
```

Cumulative for `envoy-config` across Tasks 3+4: `bootstrap.rs` 261/10 and
`lib.rs` 26/19, against `PLAN.md`'s measured 249/4 and 26/19. `lib.rs` matches
exactly. `bootstrap.rs` runs +12 insertions and +6 deletions over the prototype —
the three stale doc-count corrections above, which the prototype did not make.
